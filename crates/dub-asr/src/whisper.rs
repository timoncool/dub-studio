//! Whisper-движок ASR как альтернатива Parakeet: обёртка над standalone-бинарём Purfview
//! (whisper-standalone-win, faster-whisper на CTranslate2). Запускаем `whisper-faster.exe` сабпроцессом,
//! просим `--output_format json --word_timestamps True`, парсим словные таймстемпы -> те же Word/Segment,
//! что у Parakeet (единый контракт сегментации `segment_words`). Так пользователь может выбрать движок
//! ASR (Parakeet/Whisper), РАЗНЫЕ модели (tiny…large-v3/turbo) и РАЗНЫЕ кванты (compute_type).
//!
//! Модель ищется локально (`--model <size> --model_dir <dir>`, каталог `faster-whisper-<size>`), сеть
//! глушим `HF_HUB_OFFLINE=1` (оффлайн-first). Диаризация остаётся на Sortformer (как у Parakeet): для
//! per-speaker раскладываем слова whole-clip по репликам, затем сегментируем внутри каждой.

use crate::hallucination::{hallucination_kind, is_hallucination, HallucinationRules};
use crate::segment::{segment_words, Segment, Word, SEG_MAX_GAP, SEG_MAX_DUR};
use crate::{load_wav_16k_mono, AsrEngine, AsrError, SpeakerSegment, Turn, TARGET_SR};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Разбор JSON faster-whisper: словный поток настоящих сегментов и отдельно сегменты-кандидаты в
/// галлюцинации (целиком, со своими словами). Кандидаты не смешиваются со словами соседних реплик:
/// дальше каждый становится ОТДЕЛЬНЫМ сегментом, и решение «скрыть или оставить» принимает analyze
/// по голосу на этом интервале.
#[derive(Debug, Default)]
struct Parsed {
    words: Vec<Word>,
    suspects: Vec<Segment>,
}

impl Parsed {
    fn shift(&mut self, off: f64) {
        for w in &mut self.words {
            w.start += off;
            w.end += off;
        }
        for s in &mut self.suspects {
            s.start += off;
            s.end += off;
            for w in &mut s.words {
                w.start += off;
                w.end += off;
            }
        }
    }

    fn append(&mut self, mut other: Parsed) {
        self.words.append(&mut other.words);
        self.suspects.append(&mut other.suspects);
    }

    /// Сегменты дубляжа: обычная сегментация настоящих слов + кандидаты отдельными сегментами, по времени.
    fn into_segments(self) -> Vec<Segment> {
        let mut out = segment_words(&self.words, SEG_MAX_GAP, SEG_MAX_DUR);
        out.extend(self.suspects);
        out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        out
    }
}

/// Порог «длинного» файла, сек: короче — монолитный прогон как раньше (питон-паритет).
const PAR_MIN_DUR: f64 = 600.0;
/// Целевой размер окна параллельного прогона, сек (~5 мин: баланс стартовой цены сабпроцесса и параллели).
const PAR_WIN_SECS: f64 = 300.0;
/// Радиус поиска самой тихой точки вокруг номинальной границы окна (smart-split из transcribe-rs), сек.
const PAR_SEARCH_SECS: f64 = 8.0;
/// Максимум одновременных сабпроцессов whisper-faster (CPU-инференс; больше — RAM×N и диминишинг).
const PAR_MAX_WORKERS: usize = 4;

/// Длительность WAV по заголовку (сек) — дёшево, без чтения сэмплов.
fn wav_duration_secs(wav: &Path) -> Option<f64> {
    let r = hound::WavReader::open(wav).ok()?;
    let spec = r.spec();
    if spec.sample_rate == 0 || spec.channels == 0 {
        return None;
    }
    Some(r.duration() as f64 / spec.sample_rate as f64)
}

/// Записать 16k mono f32-сэмплы как PCM16 WAV (окно параллельного прогона).
fn write_wav_16k_mono(path: &Path, samples: &[f32]) -> Result<(), AsrError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| AsrError::Io(e.to_string()))?;
    for &x in samples {
        let v = (x.clamp(-1.0, 1.0) * 32767.0) as i16;
        w.write_sample(v).map_err(|e| AsrError::Io(e.to_string()))?;
    }
    w.finalize().map_err(|e| AsrError::Io(e.to_string()))
}

/// Самая тихая точка (мин. RMS по 25мс-кадрам) в ±search_secs от номинального семпла — граница окна
/// в тишине, а не посреди слова (transcribe-rs vad_chunked.rs smart_split, упрощённый порт).
fn quietest_near(samples: &[f32], sr: usize, nominal: usize, search_secs: f64) -> usize {
    let half = (search_secs * sr as f64) as usize;
    let a = nominal.saturating_sub(half);
    let b = (nominal + half).min(samples.len());
    let frame = sr / 40; // 25мс
    if b <= a || frame == 0 || b - a < frame {
        return nominal.min(samples.len());
    }
    let mut best = (f64::MAX, nominal);
    let mut i = a;
    while i + frame <= b {
        let rms: f64 = samples[i..i + frame].iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / frame as f64;
        if rms < best.0 {
            best = (rms, i + frame / 2);
        }
        i += frame;
    }
    best.1
}

/// Движок Whisper (standalone faster-whisper). Держит параметры запуска; модель грузится сабпроцессом
/// на каждый вызов (бинарь — отдельный процесс, состояние между вызовами не держим).
///
/// `Clone` дёшев (только пути/строки) — нужен для ПАРАЛЛЕЛЬНОГО оконного прогона: каждое окно
/// транскрибируется отдельным сабпроцессом `whisper-faster.exe` в своём потоке (движок CPU-bound,
/// монолитный прогон простаивал ядра). Общего состояния между вызовами нет — распараллеливание тривиально.
#[derive(Clone)]
pub struct WhisperAsr {
    /// Путь к whisper-faster.exe.
    bin: PathBuf,
    /// Каталог с моделями (внутри — `faster-whisper-<size>`).
    model_dir: PathBuf,
    /// Имя модели: tiny|base|small|medium|large-v3|large-v3-turbo.
    model: String,
    /// compute_type (квант): int8|int8_float32|float32|float16|…
    compute: String,
    /// Устройство исполнения: cpu|cuda.
    device: String,
}

impl WhisperAsr {
    pub fn new(
        bin: impl AsRef<Path>,
        model_dir: impl AsRef<Path>,
        model: impl Into<String>,
        compute: impl Into<String>,
        device: impl Into<String>,
    ) -> Self {
        Self {
            bin: bin.as_ref().to_path_buf(),
            model_dir: model_dir.as_ref().to_path_buf(),
            model: model.into(),
            compute: compute.into(),
            device: device.into(),
        }
    }

    /// Авто-выбор пути: короткий файл (< PAR_MIN_DUR) — монолитный прогон КАК РАНЬШЕ (паритет);
    /// длинный — параллельные окна (N сабпроцессов whisper-faster на CPU-ядра, см. run_words_windowed).
    fn run_words_auto(&self, wav: &Path, lang: &str) -> Result<Parsed, AsrError> {
        let dur = wav_duration_secs(wav).unwrap_or(0.0);
        if dur < PAR_MIN_DUR {
            return self.run_words(wav, lang, None);
        }
        self.run_words_windowed(wav, lang)
    }

    /// ПАРАЛЛЕЛЬНЫЙ оконный прогон длинного файла (PARALLEL_FINDINGS §1-2). Наш whisper-faster.exe
    /// v1.0.1 НЕ имеет `--batched` (проверено --help; флаг появился в XXL r239.1), а движок у нас
    /// CPU-bound (device=cpu из настроек) и монолитный прогон простаивает ядра. Поэтому параллель =
    /// N НЕЗАВИСИМЫХ сабпроцессов по окнам ~PAR_WIN_SECS: для CPU-инференса «VRAM ×N» из ресёрча не
    /// применим, а RAM на копию модели терпим при ≤PAR_MAX_WORKERS. Границы окон — по МИНИМУМУ
    /// энергии в ±PAR_SEARCH_SECS от номинала (smart-split из transcribe-rs vad_chunked.rs: режем в
    /// тишине, не посреди слова) → окна БЕЗ overlap → дубликатов слов на швах нет по построению.
    fn run_words_windowed(&self, wav: &Path, lang: &str) -> Result<Parsed, AsrError> {
        let (samples, _wav_sr) = load_wav_16k_mono(wav)?;
        let sr = TARGET_SR as usize;
        let total = samples.len() as f64 / sr as f64;
        // Границы: каждые PAR_WIN_SECS, сдвиг к самой тихой точке в окрестности.
        let mut bounds: Vec<usize> = vec![0];
        let mut t = PAR_WIN_SECS;
        while t < total - 1.0 {
            bounds.push(quietest_near(&samples, sr, (t * sr as f64) as usize, PAR_SEARCH_SECS));
            t += PAR_WIN_SECS;
        }
        bounds.push(samples.len());
        bounds.dedup();
        if bounds.len() <= 2 {
            return self.run_words(wav, lang, None); // одно окно — обычный путь
        }

        // Temp-каталог под окна: рядом с исходным WAV (тот же диск, живёт только на время прогона).
        let stem = wav.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
        let tmp_dir = wav.parent().unwrap_or(Path::new(".")).join(format!("wsp_par_{stem}"));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).map_err(|e| AsrError::Io(e.to_string()))?;

        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
        // На CUDA каждый сабпроцесс грузит копию модели в VRAM (large-v3 ≈ 3ГБ) и GPU уже сатурирован
        // одним инференсом — 2 воркера (перекрытие загрузки/декода); на CPU — до PAR_MAX_WORKERS ядер.
        let max_workers = if self.device == "cuda" { 2 } else { PAR_MAX_WORKERS };
        let workers = max_workers.min(bounds.len() - 1).max(1);
        // CPU-потоки на сабпроцесс: делим ядра между воркерами (флаг --threads есть в v1.0.1 help).
        let threads_per = (cores / workers).max(2);

        // Окна: (индекс, старт-семпл, конец-семпл, путь temp wav). Пишем WAV заранее (I/O дёшев).
        let mut wins: Vec<(usize, usize, usize, PathBuf)> = Vec::new();
        for (i, w) in bounds.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            if b <= a {
                continue;
            }
            let p = tmp_dir.join(format!("win_{i:03}.wav"));
            write_wav_16k_mono(&p, &samples[a..b])?;
            wins.push((i, a, b, p));
        }
        drop(samples);

        // Пачки по `workers` потоков: spawn → join (простая, предсказуемая параллель без пула).
        let mut all = Parsed::default();
        for batch in wins.chunks(workers) {
            let mut handles = Vec::new();
            for (i, a, _b, p) in batch.iter().cloned() {
                let eng = self.clone();
                let lang = lang.to_string();
                handles.push(dub_core::proc::spawn(move || -> Result<Parsed, AsrError> {
                    let off = a as f64 / TARGET_SR as f64;
                    // 1 ретрай на окно: транзиентный сбой сабпроцесса не валит весь файл сразу.
                    let mut parsed = match eng.run_words(&p, &lang, Some(threads_per)) {
                        Ok(w) => w,
                        Err(_) => eng.run_words(&p, &lang, Some(threads_per)).map_err(|e| {
                            AsrError::Parakeet(format!("whisper window {i} (offset {off:.0}s): {e}"))
                        })?,
                    };
                    parsed.shift(off);
                    Ok(parsed)
                }));
            }
            for h in handles {
                match h.join() {
                    Ok(Ok(parsed)) => all.append(parsed),
                    Ok(Err(e)) => {
                        let _ = std::fs::remove_dir_all(&tmp_dir);
                        return Err(e);
                    }
                    Err(_) => {
                        let _ = std::fs::remove_dir_all(&tmp_dir);
                        return Err(AsrError::Parakeet("whisper: a window thread panicked".into()));
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&tmp_dir);
        all.words.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal));
        Ok(all)
    }

    /// Прогнать whisper-faster.exe на WAV, вернуть словный поток (абсолютные секунды). lang="auto" ->
    /// авто-детект (флаг --language не передаём). Оффлайн: HF_HUB_OFFLINE=1, модель из --model_dir.
    /// `threads`: Some(n) -> явный `--threads n` (оконная параллель делит ядра); None -> дефолт движка
    /// (короткий путь БЕЗ изменений — паритет).
    fn run_words(&self, wav: &Path, lang: &str, threads: Option<usize>) -> Result<Parsed, AsrError> {
        let cwd = std::env::current_dir().ok();
        let abs = |p: &Path| -> PathBuf {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                match &cwd {
                    Some(c) => c.join(p),
                    None => p.to_path_buf(),
                }
            }
        };
        // Свежий выходной каталог рядом с WAV — читаем единственный *.json, старьё не мешает.
        let stem = wav.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
        let out_dir = abs(wav.parent().unwrap_or(Path::new("."))).join(format!("wsp_{stem}"));
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).map_err(|e| AsrError::Io(e.to_string()))?;

        let mut cmd = Command::new(abs(&self.bin));
        cmd.arg(abs(wav))
            .arg("--model").arg(&self.model)
            .arg("--model_dir").arg(abs(&self.model_dir))
            .arg("--task").arg("transcribe")
            .arg("--output_format").arg("json")
            .arg("--output_dir").arg(&out_dir)
            .arg("--compute_type").arg(&self.compute)
            .arg("--device").arg(&self.device)
            .arg("--word_timestamps").arg("True")
            .arg("--beep_off");
        // Анти-галлюцинации (ENGINES_FINDINGS §3, arxiv 2501.11378: VAD до декодера = 0.2% галлюцинаций
        // против 21.3%; condition_on_previous_text тянет «Субтитры создавал…» через паузы; temp-fallback
        // на no_speech = петли). Флаги подтверждены по --help нашего XXL r245.4.
        let is_xxl_flags = self.bin.file_name().and_then(|s| s.to_str()).is_some_and(|n| n.to_ascii_lowercase().contains("xxl"));
        if is_xxl_flags {
            cmd.arg("--vad_filter").arg("True")
                .arg("--vad_method").arg("silero_v5_fw")
                .arg("--condition_on_previous_text").arg("False")
                .arg("--beam_size").arg("1")
                .arg("--temperature").arg("0")
                .arg("--hallucination_silence_threshold").arg("2");
        }
        // XXL-сборка: батч-инференс внутри движка (--batched, Silero-VAD окна ≤30с пачками; появился в
        // r239.1 — старый onefile r192 флага НЕ знает, ему не передаём). Главный рычаг скорости на GPU.
        let is_xxl = self
            .bin
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().contains("xxl"));
        if is_xxl {
            cmd.arg("--batched");
        }
        // Явные CPU-потоки (флаг есть в v1.0.1): оконная параллель делит ядра между сабпроцессами.
        if let Some(n) = threads {
            cmd.arg("--threads").arg(n.to_string());
        }
        // Язык: конкретный код -> фиксируем (быстрее и точнее), "auto"/пусто -> авто-детект.
        let l = lang.trim();
        if !l.is_empty() && l != "auto" {
            cmd.arg("--language").arg(l);
        }
        // Оффлайн: не ходить в сеть за моделью (веса уже на диске).
        cmd.env("HF_HUB_OFFLINE", "1").env("TRANSFORMERS_OFFLINE", "1");
        if let Some(dir) = self.bin.parent() {
            cmd.current_dir(abs(dir)); // ради bundled CTranslate2/oneDNN-DLL рядом с бинарём
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let out = dub_core::proc::output(&mut cmd).map_err(|e| AsrError::Parakeet(format!("whisper spawn: {e}")))?;
        if !out.status.success() {
            // АВТО-ФОЛБЭК cuda -> cpu: дефолт девайса теперь cuda (быстрее в разы на NVIDIA), но на
            // машине без CUDA/либ сабпроцесс падает — повторяем ОДИН раз на cpu с безопасным int8
            // (float16 на CPU роняет CTranslate2 — гард #41). device=="cpu" сюда не зайдёт (нет рекурсии).
            if self.device == "cuda" {
                let mut fb = self.clone();
                fb.device = "cpu".into();
                if matches!(fb.compute.as_str(), "float16" | "bfloat16" | "int8_float16" | "int8_bfloat16") {
                    fb.compute = "int8".into();
                }
                return fb.run_words(wav, lang, threads);
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut tail: Vec<&str> = stderr.lines().chain(stdout.lines()).rev().take(10).collect();
            tail.reverse();
            return Err(AsrError::Parakeet(format!(
                "whisper exit code {:?}: {}",
                out.status.code(),
                tail.join(" | ")
            )));
        }

        // Читаем единственный *.json из out_dir.
        let json_path = std::fs::read_dir(&out_dir)
            .map_err(|e| AsrError::Io(e.to_string()))?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .ok_or_else(|| AsrError::Parakeet("whisper: no JSON output".into()))?;
        let txt = std::fs::read_to_string(&json_path)
            .map_err(|e| AsrError::WavRead(json_path.display().to_string(), e.to_string()))?;
        let parsed = parse_whisper_json(&txt);
        let _ = std::fs::remove_dir_all(&out_dir);
        parsed
    }
}

/// Результат пакетного прогона: каталог вывода (вызывающий удаляет) и JSON каждого входного файла.
struct Batch {
    out_dir: PathBuf,
    jsons: Vec<Result<String, AsrError>>,
}

impl WhisperAsr {
    /// Один сабпроцесс на список файлов (filelist). `words` — просить словные таймстемпы.
    fn run_filelist(&self, files: &[PathBuf], lang: &str, words: bool) -> Result<Batch, AsrError> {
        let cwd = std::env::current_dir().ok();
        let abs = |p: &Path| -> PathBuf {
            if p.is_absolute() { p.to_path_buf() } else { cwd.as_ref().map(|c| c.join(p)).unwrap_or_else(|| p.to_path_buf()) }
        };
        let Some(first) = files.first() else {
            return Ok(Batch { out_dir: PathBuf::new(), jsons: Vec::new() });
        };
        let parent = first.parent().unwrap_or(Path::new(".")).to_path_buf();
        let out_dir = abs(&parent).join(if words { "wsp_words" } else { "wsp_qc" });
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).map_err(|e| AsrError::Io(format!("{}: {e}", out_dir.display())))?;
        let list = out_dir.join("files.txt");
        let body = files.iter().map(|f| abs(f).to_string_lossy().to_string()).collect::<Vec<_>>().join("\r\n");
        std::fs::write(&list, body).map_err(|e| AsrError::Io(format!("{}: {e}", list.display())))?;
        let mut cmd = Command::new(abs(&self.bin));
        cmd.arg(&list)
            .arg("--model").arg(&self.model)
            .arg("--model_dir").arg(abs(&self.model_dir))
            .arg("--task").arg("transcribe")
            .arg("--output_format").arg("json")
            .arg("--output_dir").arg(&out_dir)
            .arg("--compute_type").arg(&self.compute)
            .arg("--device").arg(&self.device)
            .arg("--beep_off");
        let is_xxl = self.bin.file_name().and_then(|s| s.to_str()).is_some_and(|n| n.to_ascii_lowercase().contains("xxl"));
        if is_xxl {
            cmd.arg("--batched");
            // QC коротких клипов (ENGINES_FINDINGS §3.2): VAD до декодера + без истории + temp 0 +
            // жёстче no_speech — гул/тишина возвращают ПУСТО (= брак по qc_similarity), а не галлюцинацию.
            cmd.arg("--vad_filter").arg("True")
                .arg("--vad_method").arg("silero_v5_fw")
                .arg("--condition_on_previous_text").arg("False")
                .arg("--beam_size").arg("1")
                .arg("--temperature").arg("0")
                .arg("--no_speech_threshold").arg("0.5")
                .arg("--word_timestamps").arg("True")
                .arg("--hallucination_silence_threshold").arg("2");
        } else if words {
            cmd.arg("--word_timestamps").arg("True");
        }
        let l = lang.trim();
        if !l.is_empty() && l != "auto" {
            cmd.arg("--language").arg(l);
        }
        cmd.env("HF_HUB_OFFLINE", "1").env("TRANSFORMERS_OFFLINE", "1");
        if let Some(dir) = self.bin.parent() {
            cmd.current_dir(abs(dir));
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = dub_core::proc::output(&mut cmd).map_err(|e| AsrError::Parakeet(format!("whisper spawn: {e}")))?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&out_dir);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let mut tail: Vec<&str> = stderr.lines().rev().take(10).collect();
            tail.reverse();
            return Err(AsrError::Parakeet(format!("whisper batch exit code {:?}: {}", out.status.code(), tail.join(" | "))));
        }
        let jsons = files
            .iter()
            .map(|f| {
                let stem = f
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| AsrError::Io(format!("file name {}", f.display())))?;
                let jp = out_dir.join(format!("{stem}.json"));
                std::fs::read_to_string(&jp).map_err(|e| AsrError::WavRead(jp.display().to_string(), e.to_string()))
            })
            .collect();
        Ok(Batch { out_dir, jsons })
    }
}

impl AsrEngine for WhisperAsr {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError> {
        Ok(self.run_words_auto(wav, lang)?.into_segments())
    }

    /// Пакет: ОДИН сабпроцесс на весь список (filelist .txt — Purfview поддерживает; старт процесса
    /// дорогой, поэтому не по-файлово). JSONы читаем по stem'ам входных файлов. Сбой пакета -> Err с его
    /// причиной у каждого файла.
    /// Сегменты с сильным признаком галлюцинации (титр субтитровщика, звук в скобках, ни букв, ни цифр) в
    /// текст не входят: иначе фантомная фраза на тихом клипе давала бы ложное сходство с ожидаемым.
    /// Фразы из списка («Thank you.», «Watch out!») остаются: перевод их и правда содержит.
    fn transcribe_many(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<String, AsrError>> {
        let batch = match self.run_filelist(files, lang, false) {
            Ok(b) => b,
            Err(e) => {
                let msg = e.to_string();
                return files.iter().map(|_| Err(AsrError::Io(msg.clone()))).collect();
            }
        };
        let out = batch
            .jsons
            .into_iter()
            .map(|j| {
                let v: serde_json::Value = serde_json::from_str(&j?).map_err(|e| AsrError::Io(format!("the whisper answer is not JSON: {e}")))?;
                let segs = v
                    .get("segments")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| AsrError::Io("the whisper answer has no segments".to_string()))?;
                Ok(segs
                    .iter()
                    .filter_map(|s| s.get("text").and_then(|t| t.as_str()))
                    .filter(|t| !hallucination_kind(t, HallucinationRules::Whisper).is_some_and(|k| k.is_strong()))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string())
            })
            .collect();
        let _ = std::fs::remove_dir_all(&batch.out_dir);
        out
    }

    /// Пакет со словными таймингами (один сабпроцесс на список, как transcribe_many). Слова всех
    /// сегментов: в озвученной фразе «Watch out!» — настоящая реплика, а не галлюцинация.
    fn transcribe_many_words(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<Word>, AsrError>> {
        let batch = match self.run_filelist(files, lang, true) {
            Ok(b) => b,
            Err(e) => {
                let msg = e.to_string();
                return files.iter().map(|_| Err(AsrError::Parakeet(msg.clone()))).collect();
            }
        };
        let out = batch
            .jsons
            .into_iter()
            .map(|j| j.and_then(|txt| parse_whisper_json(&txt)).map(|p| p.into_segments().into_iter().flat_map(|s| s.words).collect()))
            .collect();
        let _ = std::fs::remove_dir_all(&batch.out_dir);
        out
    }

    /// Per-speaker: whole-clip ОДИН прогон (сабпроцесс дорогой на старт), затем слова раскладываем по
    /// репликам (по середине слова во временном окне реплики; слово вне всех окон -> ближайшая реплика),
    /// внутри каждой — обычная сегментация. Времена уже абсолютные.
    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError> {
        eprintln!("[asr] Whisper transcribe_turns: {} turns, wav={}", turns.len(), wav.display());
        let parsed = self.run_words_auto(wav, lang)?;
        if turns.is_empty() {
            // нет реплик -> single-speaker (0), как fallback
            return Ok(parsed
                .into_segments()
                .into_iter()
                .map(|s| SpeakerSegment { start: s.start, end: s.end, text: s.text, speaker: 0 })
                .collect());
        }
        // реплика, чьё окно содержит середину; иначе — ближайшая по центру окна.
        let turn_of = |mid: f64| -> usize {
            turns.iter().position(|t| mid >= t.start && mid <= t.end).unwrap_or_else(|| {
                turns
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let da = (mid - (a.start + a.end) / 2.0).abs();
                        let db = (mid - (b.start + b.end) / 2.0).abs();
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            })
        };
        // Ведро слов на реплику.
        let mut buckets: Vec<Vec<Word>> = vec![Vec::new(); turns.len()];
        for w in parsed.words {
            buckets[turn_of((w.start + w.end) / 2.0)].push(w);
        }
        let mut out = Vec::new();
        for (i, ws) in buckets.into_iter().enumerate() {
            if ws.is_empty() {
                continue;
            }
            let spk = turns[i].speaker;
            for s in segment_words(&ws, SEG_MAX_GAP, SEG_MAX_DUR) {
                out.push(SpeakerSegment { start: s.start, end: s.end, text: s.text, speaker: spk });
            }
        }
        for s in parsed.suspects {
            let spk = turns[turn_of((s.start + s.end) / 2.0)].speaker;
            out.push(SpeakerSegment { start: s.start, end: s.end, text: s.text, speaker: spk });
        }
        out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        Ok(out)
    }
}

/// Распарсить JSON faster-whisper (openai-формат) в словный поток. Берём words каждого сегмента
/// ({word,start,end}); если у сегмента нет words — сам сегмент как одно «слово» (fallback). Ведущий
/// пробел в word тримим (segment_words соединяет через " "). Ответ, который не JSON или без списка
/// segments, — ошибка распознавания, а не пустая речь.
/// Сегмент, который целиком — галлюцинация Whisper (is_hallucination), уходит в `suspects` отдельным
/// сегментом со своими словами и в общий поток не попадает.
fn parse_whisper_json(txt: &str) -> Result<Parsed, AsrError> {
    let v: serde_json::Value =
        serde_json::from_str(txt).map_err(|e| AsrError::Parakeet(format!("whisper: the output is not JSON: {e}")))?;
    let mut out = Parsed::default();
    let segs = v
        .get("segments")
        .and_then(|s| s.as_array())
        .ok_or_else(|| AsrError::Parakeet("whisper: the output has no segments list".into()))?;
    for seg in segs {
        let text = seg.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let seg_start = seg.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let seg_end = seg.get("end").and_then(|x| x.as_f64()).unwrap_or(seg_start).max(seg_start);
        let mut words = Vec::new();
        match seg.get("words").and_then(|w| w.as_array()) {
            Some(arr) if !arr.is_empty() => {
                for w in arr {
                    let word = w
                        .get("word")
                        .or_else(|| w.get("text"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if word.is_empty() {
                        continue;
                    }
                    let start = w.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
                    let end = w.get("end").and_then(|x| x.as_f64()).unwrap_or(start);
                    words.push(Word { word, start, end: end.max(start) });
                }
            }
            _ => {
                // сегмент без словных таймстемпов — одно «слово» на весь сегмент
                if !text.is_empty() {
                    words.push(Word { word: text.clone(), start: seg_start, end: seg_end });
                }
            }
        }
        if words.is_empty() {
            continue;
        }
        let seg_text = if text.is_empty() {
            words.iter().map(|w| w.word.as_str()).collect::<Vec<_>>().join(" ")
        } else {
            text
        };
        if is_hallucination(&seg_text, HallucinationRules::Whisper) {
            let start = words.first().map(|w| w.start).unwrap_or(seg_start);
            let end = words.iter().map(|w| w.end).fold(start, f64::max);
            out.suspects.push(Segment { start, end, text: seg_text, words });
        } else {
            out.words.extend(words);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_word_timestamps() {
        let j = r#"{"segments":[
            {"start":0.0,"end":0.8,"text":" Hello world.","words":[
                {"word":" Hello","start":0.0,"end":0.4,"probability":0.9},
                {"word":" world.","start":0.4,"end":0.8,"probability":0.8}]}],
            "language":"en"}"#;
        let ws = parse_whisper_json(j).unwrap().words;
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].word, "Hello");
        assert_eq!(ws[1].word, "world.");
        assert!((ws[1].end - 0.8).abs() < 1e-6);
    }

    #[test]
    fn falls_back_to_segment_when_no_words() {
        let j = r#"{"segments":[{"start":1.0,"end":2.0,"text":" No words here"}],"language":"ru"}"#;
        let ws = parse_whisper_json(j).unwrap().words;
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].word, "No words here");
    }

    #[test]
    fn garbage_is_an_error_and_no_segments_is_silence() {
        assert!(parse_whisper_json("not json").is_err());
        assert!(parse_whisper_json("{}").is_err());
        assert!(parse_whisper_json(r#"{"segments":[],"language":"en"}"#).unwrap().words.is_empty());
    }

    #[test]
    fn a_credit_over_silence_becomes_its_own_segment() {
        let j = r#"{"segments":[
            {"start":0.0,"end":1.2,"text":" Мы уходим","words":[
                {"word":" Мы","start":0.0,"end":0.4},{"word":" уходим","start":0.45,"end":1.2}]},
            {"start":1.3,"end":3.0,"text":" Субтитры сделал DimaTorzok","words":[
                {"word":" Субтитры","start":1.3,"end":2.0},{"word":" сделал","start":2.0,"end":2.4},
                {"word":" DimaTorzok","start":2.4,"end":3.0}]},
            {"start":3.1,"end":3.8,"text":" завтра.","words":[{"word":" завтра.","start":3.1,"end":3.8}]}]}"#;
        let p = parse_whisper_json(j).unwrap();
        let heard: Vec<&str> = p.words.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(heard, ["Мы", "уходим", "завтра."]);
        assert_eq!(p.suspects.len(), 1);
        assert_eq!(p.suspects[0].text, "Субтитры сделал DimaTorzok");
        let segs = p.into_segments();
        // Зазор 0.1с слил бы титр с репликой в один сегмент; кандидат держится отдельно.
        assert!(segs.iter().any(|s| s.text == "Субтитры сделал DimaTorzok"));
        assert!(segs.iter().all(|s| !s.text.contains("уходим") || !s.text.contains("DimaTorzok")));
    }
}
