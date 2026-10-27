//! Сайдкар llama-server (llama.cpp). Поднимаем один процесс на свободном 127.0.0.1:<порт>,
//! ждём готовности по /health, глушим на Drop. Это штатный способ работы llama.cpp с мультимодальностью:
//! OpenAI-совместимый /v1/chat/completions, картинки через --mmproj (vision-проектор Gemma).
//!
//! Соответствие питону (ctx_translate.py): одна загрузка Gemma -> много сфокусированных вызовов, всё на GPU.
//! n_gpu_layers=-1 (все слои на GPU), n_ctx=12288, flash_attn=true — как в Llama(...) там.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::LlmError;

/// Микробатч vision-режима по умолчанию: с запасом больше потолка токенов кадра Gemma 4 (~1100).
const VISION_UBATCH: u32 = 2048;
/// Логический батч llama-server по умолчанию (--batch-size); не меньше микробатча.
const DEFAULT_BATCH: u32 = 2048;

/// Последние N строк stderr — для внятной диагностики, если сервер упал/не поднялся.
type LogTail = Arc<Mutex<VecDeque<String>>>;

type LogFile = Option<Arc<Mutex<std::fs::File>>>;

/// The lines where llama-server (at `--log-verbosity 4`) names where the model went: the device, the layers offloaded
/// to it and the weights on each buffer. The same filter as the other studios' llama-server.
type DeviceLines = Arc<Mutex<Vec<String>>>;
const DEVICE_MARKERS: [&str; 3] = ["using device", "offloaded", "model buffer size"];

fn drain_to_tail<R: std::io::Read + Send + 'static>(reader: R, tail: LogTail, file: LogFile, devices: DeviceLines) {
    std::thread::spawn(move || {
        let r = BufReader::new(reader);
        for line in r.lines().map_while(Result::ok) {
            if DEVICE_MARKERS.iter().any(|m| line.contains(m)) {
                if let Ok(mut d) = devices.lock() {
                    d.push(line.trim().to_string());
                }
            }
            if let Some(f) = &file {
                if let Ok(mut f) = f.lock() {
                    let _ = writeln!(f, "{line}");
                }
            }
            if let Ok(mut t) = tail.lock() {
                if t.len() >= 40 {
                    t.pop_front();
                }
                t.push_back(line);
            }
        }
    });
}

fn tail_text(tail: &LogTail) -> String {
    tail.lock()
        .map(|t| t.iter().map(String::as_str).collect::<Vec<_>>().join(" | "))
        .unwrap_or_default()
}

/// Положительный u32 из env-переменной (пусто/0/мусор -> None). Общий парсер knob'ов llama-server.
fn env_u32_pos(name: &str) -> Option<u32> {
    std::env::var(name).ok().and_then(|s| s.trim().parse::<u32>().ok()).filter(|n| *n > 0)
}

/// Параметры запуска llama-server. Пути к бинарю/модели/mmproj + серверные knob'ы Gemma.
pub struct ServerOpts {
    /// Путь к llama-server(.exe). По умолчанию ищется в tools/llama рядом с репо (resolve сервером).
    pub bin: PathBuf,
    /// GGUF основной модели (Gemma-4 12B QAT q4_0).
    pub model: PathBuf,
    /// mmproj GGUF (vision-проектор). None -> текстовый режим без картинок.
    pub mmproj: Option<PathBuf>,
    /// Слои на GPU. -1 = все (как n_gpu_layers=-1 в питоне).
    pub n_gpu_layers: i32,
    /// Контекст. Питон ctx_translate: n_ctx=12288.
    pub ctx_size: u32,
    /// Размер prefill-батча (-ub). None -> дефолт llama. Видимая настройка «Экономия RAM»: меньше =
    /// меньше пиковый буфер графа prefill (против OOM на 32ГБ). Приоритетнее env DUB_STUDIO_LLAMA_UBATCH.
    pub ubatch: Option<u32>,
    /// Секунд ждать готовности (загрузка 7ГБ GGUF в VRAM небыстрая).
    pub ready_timeout_secs: u64,
    /// Файл, куда дописывается весь вывод сервера (stdout+stderr); None — только хвост в памяти.
    pub log_file: Option<PathBuf>,
}

impl ServerOpts {
    pub fn new(bin: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        ServerOpts {
            bin: bin.into(),
            model: model.into(),
            mmproj: None,
            n_gpu_layers: -1,
            ctx_size: 12288,
            ubatch: None,
            ready_timeout_secs: 300,
            log_file: None,
        }
    }

    pub fn with_mmproj(mut self, mmproj: impl Into<PathBuf>) -> Self {
        self.mmproj = Some(mmproj.into());
        self
    }

    pub fn with_log_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.log_file = Some(path.into());
        self
    }

    /// Задать prefill-батч (из настройки «Экономия RAM»). None -> дефолт llama.
    pub fn with_ubatch(mut self, ubatch: Option<u32>) -> Self {
        self.ubatch = ubatch.filter(|n| *n > 0);
        self
    }
}

/// Живой llama-server. Держит дочерний процесс; base_url для клиента. Drop = kill (важно: резидентный
/// GGUF 5-7ГБ иначе держит VRAM — питон делает release() перед NVENC-берном по той же причине).
pub struct LlamaServer {
    child: Child,
    port: u16,
    base_url: String,
    log_tail: LogTail,
    _tracked: dub_core::proc::ChildGuard,
}

/// Подобрать свободный TCP-порт на 127.0.0.1 (bind :0 -> ОС выдаёт порт, тут же освобождаем).
/// Тонкая гонка (порт может занять другой процесс между bind и стартом llama), но на localhost-сайдкаре
/// приемлемо — тот же паттерн, что desktop-оболочка использует для dub-server.
fn free_port() -> Result<u16, LlmError> {
    let l = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| LlmError::Spawn(format!("bind free-port: {e}")))?;
    let p = l
        .local_addr()
        .map_err(|e| LlmError::Spawn(format!("local_addr: {e}")))?
        .port();
    Ok(p)
}

/// The devices the llama-server build can put layers on (`--list-devices`: `  CUDA0: <card> (<total> MiB, <free> MiB
/// free)`), and what it said when it found none: its ggml init lines, or its whole answer.
fn list_devices(bin: &Path) -> Result<(Vec<String>, String), LlmError> {
    let mut cmd = Command::new(bin);
    cmd.arg("--list-devices");
    let out = dub_core::proc::output(&mut cmd).map_err(|e| LlmError::Spawn(format!("llama-server --list-devices: {e}")))?;
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let devices: Vec<String> = lines
        .iter()
        .skip_while(|l| !l.starts_with("Available devices"))
        .skip(1)
        .take_while(|l| l.split_once(':').is_some_and(|(id, _)| !id.is_empty() && !id.contains(' ')))
        .map(|l| l.to_string())
        .collect();
    let said: Vec<&str> = lines.iter().copied().filter(|l| l.contains("ggml_cuda_init") || l.contains("ggml_vulkan")).collect();
    let detail = if said.is_empty() { lines.join(" | ") } else { said.join(" | ") };
    Ok((devices, detail))
}

impl LlamaServer {
    /// Поднять сервер и дождаться готовности (/health -> ok). Ошибка если бинарь/модель отсутствуют,
    /// процесс упал или не поднялся за ready_timeout_secs.
    pub fn start(opts: &ServerOpts) -> Result<Self, LlmError> {
        if !opts.bin.is_file() {
            return Err(LlmError::BinaryMissing(opts.bin.clone()));
        }
        if !opts.model.is_file() {
            return Err(LlmError::ModelMissing(opts.model.clone()));
        }
        let ngl = std::env::var("DUB_STUDIO_LLAMA_NGL").ok().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(opts.n_gpu_layers);
        if ngl != 0 {
            let (devices, detail) = list_devices(&opts.bin)?;
            if devices.is_empty() {
                return Err(LlmError::NoGpuDevice { detail });
            }
            tracing::info!("llama-server {}: devices {}", opts.bin.display(), devices.join("; "));
        }
        let port = free_port()?;
        // Env-оверрайды памяти для слабых машин (баг-репорт: OOM «prefill graph» на 32ГБ RAM — на 64ГБ
        // тот же клип проходит). Дают подобрать без пересборки:
        //   DUB_STUDIO_LLAMA_CTX   — контекст (меньше = меньше KV-кэш),
        //   DUB_STUDIO_LLAMA_NGL   — слои на GPU (-1 все; можно уменьшить, если не хватает VRAM),
        //   DUB_STUDIO_LLAMA_UBATCH/_BATCH — размер батча prefill: ПРЯМОЙ рычаг против «prefill graph»
        //   (граф вычислений prefill масштабируется от ubatch; меньше ubatch = меньше пиковый буфер).
        let ctx = env_u32_pos("DUB_STUDIO_LLAMA_CTX").unwrap_or(opts.ctx_size);
        let mut cmd = Command::new(&opts.bin);
        cmd.arg("-m")
            .arg(&opts.model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-ngl")
            .arg(ngl.to_string())
            .arg("-c")
            .arg(ctx.to_string())
            // flash-attn как flash_attn=True в питоне (макс. GPU-ускорение). В новых сборках это enum on/off/auto.
            .arg("--flash-attn")
            .arg("on")
            // один запрос за раз (пайплайн последовательный) — не раздуваем KV-кэш параллельными слотами.
            .arg("--parallel")
            .arg("1")
            // ОБЯЗАТЕЛЬНО: Jinja-движок шаблонов. Без него llama-server игнорирует chat_template_kwargs
            // (enable_thinking:false из client.rs) -> Gemma-4 «думает», сжигает max_tokens на reasoning_content,
            // а content приходит ПУСТЫМ -> перевод/rewrite молча откатываются на оригинал (сломан весь MT).
            .arg("--jinja")
            .arg("--log-verbosity")
            .arg("4")
            .arg("--log-colors")
            .arg("off");
        // Лимит батча prefill против OOM «prefill graph» на слабой RAM. Приоритет: настройка UI
        // (opts.ubatch) -> env DUB_STUDIO_LLAMA_UBATCH -> дефолт llama (не передаём флаг).
        let ubatch = opts.ubatch.or_else(|| env_u32_pos("DUB_STUDIO_LLAMA_UBATCH"));
        let batch = env_u32_pos("DUB_STUDIO_LLAMA_BATCH");
        let vision = opts.mmproj.as_ref().is_some_and(|m| m.is_file());
        if vision {
            // Картинка кодируется некаузальным вниманием и обязана целиком влезать в один микробатч
            // (иначе GGML_ASSERT n_ubatch >= n_tokens роняет сервер на первом кадре). Кадр Gemma 4 — до
            // ~1100 токенов, поэтому потолок токенов картинки всегда равен микробатчу.
            let ub = ubatch.unwrap_or(VISION_UBATCH);
            cmd.arg("-ub").arg(ub.to_string());
            cmd.arg("-b").arg(batch.unwrap_or(DEFAULT_BATCH).max(ub).to_string());
            cmd.arg("--image-max-tokens").arg(ub.to_string());
            // --mmproj + дефолтный offload на GPU (быстрее); проектор Gemma лёгкий (~175МБ).
            if let Some(mmproj) = &opts.mmproj {
                cmd.arg("--mmproj").arg(mmproj);
            }
        } else {
            if let Some(v) = ubatch {
                cmd.arg("-ub").arg(v.to_string());
            }
            if let Some(v) = batch {
                cmd.arg("-b").arg(v.max(ubatch.unwrap_or(0)).to_string());
            }
        }

        // ОБА пайпа дренируем в потоки. Если пайп не читать, буфер переполнится и llama-server
        // заблокируется на write -> сервер «висит» и не доходит до готовности (классический дедлок).
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        // На Windows cudart/ggml DLL лежат рядом с llama-server.exe (tools/llama) — бинарь находит их
        // сам, доп. настройка PATH не нужна.

        let log_file: LogFile = match &opts.log_file {
            Some(path) => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| LlmError::LogFile { path: dir.to_path_buf(), error: e.to_string() })?;
                }
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| LlmError::LogFile { path: path.clone(), error: e.to_string() })?;
                let _ = writeln!(f, "==== {:?} (port {port})", cmd);
                Some(Arc::new(Mutex::new(f)))
            }
            None => None,
        };
        let mut child = dub_core::proc::dies_with_parent(&mut cmd)
            .spawn()
            .map_err(|e| LlmError::Spawn(format!("spawn llama-server: {e}")))?;

        let base_url = format!("http://127.0.0.1:{port}");
        let log_tail: LogTail = Arc::new(Mutex::new(VecDeque::new()));
        let devices: DeviceLines = Arc::new(Mutex::new(Vec::new()));
        if let Some(out) = child.stdout.take() {
            drain_to_tail(out, log_tail.clone(), log_file.clone(), devices.clone());
        }
        if let Some(err) = child.stderr.take() {
            drain_to_tail(err, log_tail.clone(), log_file.clone(), devices.clone());
        }

        let tracked = dub_core::proc::track(child.id());
        let mut srv = LlamaServer {
            child,
            port,
            base_url,
            log_tail,
            _tracked: tracked,
        };
        srv.wait_ready(opts.ready_timeout_secs)?;
        let devices = devices.lock().map(|d| d.clone()).unwrap_or_default();
        for line in &devices {
            tracing::info!("llama-server: {line}");
        }
        if ngl != 0 {
            if let Some(line) = devices.iter().find(|l| l.contains("offloaded 0/")) {
                return Err(LlmError::NoGpuDevice { detail: line.clone() });
            }
        }
        Ok(srv)
    }

    /// Опрос /health до status=ok или таймаута. Fail-fast: если процесс уже завершился (нет VRAM /
    /// битый gguf / занятый порт), сразу отдаём ошибку с хвостом stderr, не ждём весь таймаут.
    fn wait_ready(&mut self, timeout_secs: u64) -> Result<(), LlmError> {
        let health = format!("{}/health", self.base_url);
        let client = crate::net::local_builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| LlmError::Spawn(format!("http client: {e}")))?;
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            // Процесс упал -> не ждём таймаут впустую.
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(LlmError::ExitedEarly { status: status.to_string(), stderr: tail_text(&self.log_tail) });
            }
            match client.get(&health).send() {
                Ok(r) if r.status().is_success() => {
                    // /health отдаёт {"status":"ok"} когда модель загружена и слоты готовы.
                    if let Ok(v) = r.json::<serde_json::Value>() {
                        if v.get("status").and_then(|s| s.as_str()) == Some("ok") {
                            return Ok(());
                        }
                    } else {
                        return Ok(()); // 200 без тела тоже трактуем как готовность
                    }
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(LlmError::NotReady { secs: timeout_secs, port: self.port, stderr: tail_text(&self.log_tail) });
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Явно остановить сервер (kill + wait). Идемпотентно.
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Найти llama-server(.exe) в каталоге бинарей llama.cpp. Env DUB_STUDIO_LLAMA_BIN -> прямой путь;
/// иначе <models_or_tools_root>/llama-server(.exe). Возвращает путь (существование проверяет start()).
pub fn resolve_llama_bin(tools_llama_dir: &Path) -> PathBuf {
    if let Ok(v) = std::env::var("DUB_STUDIO_LLAMA_BIN") {
        return PathBuf::from(v);
    }
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    tools_llama_dir.join(name)
}
