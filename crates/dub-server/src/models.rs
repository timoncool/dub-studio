//! Динамический резолв активного варианта модели для КАЖДОГО движка — переключение квантов.
//!
//! Порт-баг, который это чинит: пути моделей морозились в AppState при старте, а `set_opts` был
//! заглушкой — скачанный альт-квант (Higgs q6_k, Roformer Q5_0, Parakeet fp32, Gemma q8_0) никогда
//! не применялся, работали только дефолты. Теперь резолв идёт ПРИ КАЖДОЙ джобе (analyze/render):
//!   env-override → сохранённый выбор (models/active.json) → скан установленного → дефолт.
//! Так «скачал/выбрал квант → применился» без рестарта сервера.
//!
//! active.json = {"tts":"q6_k","asr":"fp32","mt":"q8_0","sep":"Q5_0"} — токен варианта на движок.
//! Пишется при завершении скачки компонента (setup) и при смене в дропдауне (POST /engine/select).

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Прочитать сохранённый выбор вариантов (models/active.json). Нет файла/битый → пусто (=авто).
pub fn load_selection(mroot: &Path) -> Value {
    std::fs::read_to_string(mroot.join("active.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Default::default()))
}

/// Записать/обновить один слот выбора (engine -> variant) атомарно.
pub fn set_selection(mroot: &Path, engine: &str, variant: &str) -> std::io::Result<()> {
    let _held = selection_writes();
    let mut v = load_selection(mroot);
    v.as_object_mut()
        .expect("load_selection returns object")
        .insert(engine.to_string(), Value::String(variant.to_string()));
    if engine == "or_tts_on" {
        v["tts_provider"] = (if variant == "1" {
            "openrouter"
        } else {
            "local"
        })
        .into();
    }
    write_selection(mroot, &v)
}

/// Держится от чтения active.json до записи обратно (ручки настроек, агент и джобы пишут его
/// одновременно) и делит между писателями один active.json.tmp. Не реентерабелен.
pub(crate) fn selection_writes() -> std::sync::MutexGuard<'static, ()> {
    static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Записать весь выбор атомарно (tmp + rename) под уже взятым selection_writes.
pub fn write_selection(mroot: &Path, selection: &Value) -> std::io::Result<()> {
    let _ = std::fs::create_dir_all(mroot);
    let tmp = mroot.join("active.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(selection).unwrap_or_default(),
    )?;
    std::fs::rename(&tmp, mroot.join("active.json"))?;
    apply_faces_backend(mroot);
    Ok(())
}

/// Модели кастинга (лица и голос, крейт dub-faces) идут туда же, куда локальные стадии: читают
/// DUB_FACES_BACKEND при загрузке. Ставится на старте и при каждой записи выбора — переключение без рестарта.
pub fn apply_faces_backend(mroot: &Path) {
    std::env::set_var("DUB_FACES_BACKEND", stage_backend(mroot, "local_backend"));
}

/// Выбор для ответов API. Секреты не уходят никогда: вместо ключа OpenRouter — `or_key_set`, пароль
/// вырезан из `proxy_url`, вместо него — `proxy_password_set`.
/// `llm_provider`/`vision_provider` в ответе — действующий провайдер стадии, даже если в active.json его ещё нет
/// (прежние флаги or_llm_on/or_vision_on).
pub fn public_selection(mroot: &Path) -> Value {
    let mut public = redact_selection(
        &load_selection(mroot),
        crate::credentials::openrouter_source().is_some(),
        crate::credentials::proxy_password().is_some(),
    );
    let slots = public.as_object_mut().expect("redact_selection returns object");
    slots.insert(
        "google_key_set".into(),
        crate::credentials::google_api_key().is_some().into(),
    );
    slots.insert("tts_provider".into(), tts_provider(mroot).into());
    for (key, stage) in [("llm_provider", "llm"), ("vision_provider", "vision")] {
        slots.insert(key.into(), Value::String(llm_backend(mroot, stage).as_str().into()),
        );
    }
    public
}

pub(crate) fn redact_selection(selection: &Value, or_key_set: bool, proxy_password_set: bool,
) -> Value {
    let mut slots = selection.as_object().cloned().unwrap_or_default();
    slots.remove("or_key");
    slots.remove("google_key");
    let mut inline_password = false;
    if let Some(url) = slots.get("proxy_url").and_then(Value::as_str).map(str::to_owned) {
        let (bare, password) = split_proxy_password(&url);
        inline_password = password.is_some();
        slots.insert("proxy_url".into(), Value::String(bare));
    }
    slots.insert("or_key_set".into(), Value::Bool(or_key_set));
    slots.insert("proxy_password_set".into(), Value::Bool(proxy_password_set || inline_password),
    );
    Value::Object(slots)
}

/// Снять слот выбора, если он указывает на этот вариант (вариант удалён — резолв возьмёт установленный).
pub fn clear_selection_if(mroot: &Path, engine: &str, variant: &str) -> std::io::Result<()> {
    let _held = selection_writes();
    let mut v = load_selection(mroot);
    let obj = v.as_object_mut().expect("load_selection returns object");
    if obj.get(engine).and_then(Value::as_str) != Some(variant) {
        return Ok(());
    }
    obj.remove(engine);
    write_selection(mroot, &v)
}

fn pick<'a>(sel: &'a Value, engine: &str) -> Option<&'a str> {
    sel.get(engine).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Отобразить id компонента манифеста -> список (slot, значение) для записи выбора при скачивании.
/// Пусто — компонент не является переключаемым вариантом модели (движок/рантайм/OCR и т.п.).
/// ASR-варианты пишут ДВА слота: движок (asr_engine) + вариант этого движка (asr-квант / whisper-модель),
/// чтобы скачивание Whisper-модели сразу делало Whisper активным движком (и наоборот для Parakeet).
pub fn component_selection(id: &str) -> Vec<(&'static str, String)> {
    match id {
        "higgs" => vec![("tts", "q8_0".into())],
        "higgs-q6_k" => vec![("tts", "q6_k".into())],
        "higgs-q4_k_m" => vec![("tts", "q4_k_m".into())],
        "drama3" => vec![("tts", DRAMA_TTS.into())],
        "qwen3-tts" => vec![("tts", "qwen3-tts".into())],
        "qwen3-tts-q8_0" => vec![("tts", "qwen3-tts-q8_0".into())],
        SUPERTONIC_TTS => vec![("tts", SUPERTONIC_TTS.into())],
        "parakeet" => vec![("asr_engine", "parakeet".into()), ("asr", "int8".into())],
        "parakeet-ultra" => vec![("asr_engine", "parakeet".into()), ("asr", "ultra-int8".into())],
        "crisperwhisper-turbo" => vec![("asr_engine", "whisper".into())],
        "gemma" => vec![("mt", "q4_0".into())],
        "gemma-q5_0" => vec![("mt", "q5_0".into())],
        "gemma-q6_k" => vec![("mt", "q6_k".into())],
        "gemma-q8_0" => vec![("mt", "q8_0".into())],
        "roformer" => vec![("sep", "Q8_0".into())],
        "roformer-q5" => vec![("sep", "Q5_0".into())],
        "roformer-q4" => vec![("sep", "Q4_0".into())],
        _ => vec![],
    }
}

/// Разрешённые слоты для прямой установки через POST /engine/select {key,value} (без скачивания):
/// переключение движка/модели/кванта + видимые в настройках лимиты RAM. Возврат true, если слот допустим.
pub fn is_selection_key(key: &str) -> bool {
    crate::checks::is_switch(key) || matches!(
        key,
        "tts" | "asr" | "mt" | "sep" | "asr_engine"
            // Backend КАЖДОЙ локальной стадии независимо (auto|gpu|cpu): любой движок на любой инстанс.
            // gpu = CUDA, cpu = без NVIDIA. sep=сепарация(BSRoformer CUDA/CPU-сборка), diar=диаризация
            // (Sortformer onnx CUDA-EP/CPU), asr=локальный ASR (Parakeet onnx / Whisper CTranslate2).
            | "local_backend" | "sep_backend" | "diar_backend" | "asr_backend"
            | "tts_backend"     // voicing engines in audiocpp_server (Higgs, Supertonic): auto|gpu|vulkan|cpu
            // Лимиты RAM (видимые контролы в настройках, НЕ авто-магия): против OOM на слабой памяти.
            | "llama_ubatch"    // размер prefill-батча Gemma (меньше = меньше пиковый буфер графа prefill)
            | "higgs_ref_secs"  // длина реф-клипа клона голоса (меньше = меньше prefill Higgs; <12с спасает 32ГБ)
            | "bench"           // пер-стадийный бенчмарк (bench.json + ⏱ в журнале); галка в настройках, ВЫКЛ по умолчанию
            | "duck_on"         // дакинг фона под дубляжом (приглушать фон под речью); ВЫКЛ по умолчанию — не всем нужен
            | "qc_duration"     // "1" -> строгий контроль длительности/растяжения; "0" -> без ограничений
            | "multitake"       // "1" -> генерировать 3 дубля каждой фразы и выбирать лучший по таймингу; "0" -> один дубль (быстро)
            | "breath_on"       // "1" -> авто-вставка легких вдохов в паузах между фразами; "0" -> выкл
            | "speech_rate_on"  // "1" -> адаптация темпа генерации TTS под длину текста/слота; "0" -> дефолт темп
            | "lead_into_silence" // "1" -> не влезшая фраза начинается раньше, в тишину перед репликой, вместо ускорения
            | "emo_ref_on"      // "1" -> эмоциональный референс сцены (перенос эмоций из оригинального вокала); "0" -> выкл
            | "drama_vram_gb"   // видеопамять для Drama 3, ГБ; "0" (по умолчанию) — авто, см. drama_vram_limit_mb
            | "tts_idle_unload_min" // через сколько минут простоя выгружать модель озвучки (0 — никогда)
            | "cloud_daily_limit_usd" // дневной лимит облака для агентов и программ, USD (0 — без лимита)
            | "auto_shorten"    // "1" (по умолчанию) -> не влезшие в слот фразы рендер сокращает через LLM и озвучивает заново
            // Провайдер перевода и отдельно vision: local (своя Gemma) | server (локальный OpenAI-совместимый
            // сервер: Ollama, LM Studio, vLLM) | openrouter. Значение проверяет select_model.
            | "llm_provider"
            | "vision_provider"
            // Локальный OpenAI-совместимый сервер: адрес и модели из его /v1/models. Ключ — в хранилище секретов.
            | "srv_url"
            | "srv_llm"
            | "srv_vision"
            // Облачные модели (OpenRouter) — опциональная замена тяжёлого локального LLM/TTS. Всё ВЫКЛ по умолчанию.
            // Ключ OpenRouter сюда не входит: он в хранилище секретов (credentials), ручка /engine/openrouter/settings.
            | "or_llm_on"       // прежний флаг перевода через OpenRouter; читается, только пока llm_provider не задан
            | "or_llm"          // id LLM-модели перевода (напр. "google/gemini-2.5-flash")
            | "or_vision_on"    // прежний флаг vision через OpenRouter; читается, только пока vision_provider не задан
            | "or_vision"       // id vision-модели (пусто -> or_llm, если он принимает картинки)
            | "or_tts_on"       // "1" -> TTS через облако вместо локального Higgs
            | "tts_provider" | "google_tts_model" | "google_tts_mode"
            | "or_tts_model"    // id TTS-модели OpenRouter (напр. "openai/gpt-4o-mini-tts")
            | "or_tts_voice"    // голос по умолчанию для облачного TTS (напр. "alloy")
            | "or_tts_autocast" // БЕТА: автокастинг голосов по полу спикера (муж->муж/жен->жен); ВКЛ по умолчанию
            | "or_asr_on"       // "1" -> транскрипция (ASR) через OpenRouter вместо локального Parakeet/Whisper
            | "cloud_ok_recognition" | "cloud_ok_translation" | "cloud_ok_vision" | "cloud_ok_voicing" // разрешил ли человек отправлять стадию в облако
            | "or_asr"          // id STT-модели OpenRouter (напр. "openai/whisper-large-v3")
            | "or_concurrency"  // число параллельных облачных запросов (чанки в N потоков; OpenRouter ~50 конкур.)
            // Прокси (режим, тип, адрес) меняется только через /engine/proxy/settings: там пароль уходит в хранилище
            // секретов, а маршрут запросов перестраивается сразу.
    )
}

/// Допустимое значение слота, у которого значения — перечень. Для прочих слотов — любое непустое.
pub fn is_selection_value(key: &str, value: &str) -> bool {
    match key {
        "llm_provider" | "vision_provider" => LlmBackend::parse(value).is_some(),
        "tts_provider" => matches!(value, "local" | "openrouter" | "google"),
        "cloud_ok_recognition" | "cloud_ok_translation" | "cloud_ok_vision" | "cloud_ok_voicing" => matches!(value, "0" | "1"),
        "drama_vram_gb" => value.parse::<u32>().is_ok_and(|gb| gb == 0 || (DRAMA_MIN_LIMIT_GB..=256).contains(&gb)),
        "tts_idle_unload_min" => matches!(value, "0" | "5" | "15" | "30" | "60"),
        "google_tts_mode" => matches!(value, "standard" | "batch" | "auto"),
        "cloud_daily_limit_usd" => value.parse::<f64>().is_ok_and(|v| (0.0..=10_000.0).contains(&v)),
        "google_tts_model" => {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        }
        _ => true,
    }
}

/// Backend конкретной локальной стадии по ключу (sep_backend/diar_backend/asr_backend): "cpu"/"gpu"
/// перекрывают; "auto"/пусто -> сначала общий local_backend, затем GPU, только если карта и драйвер годятся
/// под CUDA-сборку стадии ([`gpu_runs`]) — иначе «авто» идёт на CPU, а не падает на CUDA.
/// Любой движок на любой инстанс — стадии независимы.
pub fn stage_backend(mroot: &Path, key: &str) -> &'static str {
    let sel = load_selection(mroot);
    let pick_bk = |k: &str| -> Option<&'static str> {
        match pick(&sel, k) {
            Some("cpu") => Some("cpu"),
            Some("gpu") | Some("cuda") => Some("gpu"),
            _ => None,
        }
    };
    pick_bk(key)
        .or_else(|| {
            if key == "local_backend" { None } else { pick_bk("local_backend") }
        })
        .unwrap_or(if gpu_runs(key) { "gpu" } else { "cpu" })
}

/// Whether this card runs the CUDA build of a stage (`key`, as in [`stage_backend`]): recognition (transcribe.cpp and the
/// audio.cpp server), diarization (the audio.cpp server) and separation (BSRoformer.cpp) have CUDA 12.9 builds beside
/// the CUDA 13 ones, so they run on any card one of them takes; the other stages have CUDA 13 builds only.
pub fn gpu_runs(key: &str) -> bool {
    let gpu = crate::hw::gpu_report();
    if matches!(key, "asr_backend" | "sep_backend" | "diar_backend") { gpu.cuda_build.is_some() } else { gpu.cuda13_ok }
}

/// The transcribe.cpp build of the recognition device (`asr_backend`, else `local_backend`), as the audio.cpp server's
/// ([`crate::acpp::build_for`], as Speech Studio picks it): the card's CUDA build (13, or 12.9 where the card or driver
/// does not run 13), Vulkan, or the processor's. A card chosen that runs no CUDA build gets the CUDA build, which
/// refuses with the reason.
pub fn tcpp_build(mroot: &Path) -> dub_asr::tcpp::Build {
    // a Mac's one build (Metal and the processor) lies in the processor build's folder
    if cfg!(target_os = "macos") {
        return dub_asr::tcpp::Build::Cpu;
    }
    match crate::acpp::build_for(server_choice(mroot, "asr_backend")) {
        Ok(crate::acpp::Build::Cuda12) => dub_asr::tcpp::Build::Cuda12,
        Ok(crate::acpp::Build::Vulkan) => dub_asr::tcpp::Build::Vulkan,
        Ok(crate::acpp::Build::Cpu | crate::acpp::Build::Metal) => dub_asr::tcpp::Build::Cpu,
        Ok(crate::acpp::Build::Cuda13) | Err(_) => dub_asr::tcpp::Build::Cuda,
    }
}

/// Why voice separation cannot run here as set: its GPU build without a usable card, or a processor without the
/// floor the GPU build's ggml-cpu library is built on (the processor build has none).
pub fn sep_unavailable(mroot: &Path) -> Option<String> {
    gpu_stage_unavailable(mroot, "sep_backend", &t!("stage-name-sep"))
        .or_else(|| (stage_backend(mroot, "sep_backend") == "gpu").then(|| crate::hw::cpu_floor("BSRoformer")).flatten())
}

/// The build of the separation engine for its device (`sep_backend`, else `local_backend`), as Speech Studio picks it
/// ([`crate::acpp::build_for`]): the card's CUDA build (13, or 12.9 where the card or driver does not run 13), Vulkan,
/// or the processor's. A card chosen that runs no CUDA build gets the CUDA build, which refuses with the reason.
pub fn sep_build(mroot: &Path) -> dub_sep::asset::Build {
    if cfg!(target_os = "macos") {
        return dub_sep::asset::Build::Cpu;
    }
    match crate::acpp::build_for(server_choice(mroot, "sep_backend")) {
        Ok(crate::acpp::Build::Cuda12) => dub_sep::asset::Build::Cuda12,
        Ok(crate::acpp::Build::Vulkan) => dub_sep::asset::Build::Vulkan,
        Ok(crate::acpp::Build::Cpu | crate::acpp::Build::Metal) => dub_sep::asset::Build::Cpu,
        Ok(crate::acpp::Build::Cuda13) | Err(_) => dub_sep::asset::Build::Cuda13,
    }
}

/// The separation engine's program for its device ([`sep_build`]).
pub fn sep_cli(repo_root: &Path, mroot: &Path) -> PathBuf {
    dub_sep::engine_cli_of(repo_root, sep_build(mroot).name())
}

/// Why a local stage set to «GPU (CUDA)» cannot run on this machine: its CUDA build would fall back to the processor
/// on its own, so the stage stops before it starts and names the reason. None — the stage is not on the GPU or the
/// card runs the stage's CUDA build ([`gpu_runs`]). `stage` is the stage's name for the message.
pub fn gpu_stage_unavailable(mroot: &Path, key: &str, stage: &str) -> Option<String> {
    if stage_backend(mroot, key) != "gpu" || gpu_runs(key) {
        return None;
    }
    let gpu = crate::hw::gpu_report();
    let card = gpu.summary().unwrap_or_else(|| "-".to_string());
    let why = match gpu.reason {
        Some("no_nvidia") => t!("stage-gpu-why-no-nvidia"),
        Some("no_device") => t!("stage-gpu-why-no-device"),
        Some("driver_old") => t!("stage-gpu-why-driver-old", card = card, min = gpu.min_driver),
        Some("gpu_old") => t!("stage-gpu-why-gpu-old", card = card, min = gpu.min_compute.clone()),
        _ => t!("stage-gpu-why-cuda-init", card = card),
    };
    Some(t!("stage-needs-cuda", stage = stage.to_string(), why = why))
}

/// Глобальный backend локальных стадий (обратная совместимость: пресеты/старые вызовы).
/// Per-stage: `stage_backend(mroot, "<sep|diar|asr>_backend")`.
pub fn local_backend(mroot: &Path) -> &'static str {
    stage_backend(mroot, "local_backend")
}

/// API-ключ OpenRouter: переменная окружения OPENROUTER_API_KEY, иначе хранилище секретов. Нет -> None.
pub fn openrouter_key() -> Option<String> {
    crate::credentials::openrouter_api_key().map(|(key, _)| key)
}

/// Режим прокси из active.json. Прежние версии знали только proxy_on + proxy_url: включённый прокси с адресом —
/// свой, иначе — как в Windows (прежний «выключенный» прокси тоже пускал запросы по переменным окружения).
pub fn proxy_mode(sel: &Value) -> dub_llm::net::ProxyMode {
    use dub_llm::net::ProxyMode;
    if let Some(mode) = pick(sel, "proxy_mode") {
        if let Some(mode) = ProxyMode::parse(mode) {
            return mode;
        }
        tracing::error!("proxy_mode {mode:?} in active.json is not recognized; using the Windows setting");
    }
    if pick(sel, "proxy_on") == Some("1") && pick(sel, "proxy_url").is_some() {
        ProxyMode::Custom
    } else {
        ProxyMode::System
    }
}

/// Тип прокси для адреса без схемы.
pub fn proxy_kind(sel: &Value) -> dub_llm::net::ProxyKind {
    pick(sel, "proxy_kind").and_then(dub_llm::net::ProxyKind::parse).unwrap_or_default()
}

/// Адрес своего прокси, по которому идут запросы: адрес из active.json (без пароля) и пароль из хранилища.
pub(crate) fn proxy_address(sel: &Value, password: Option<&str>) -> Option<String> {
    pick(sel, "proxy_url").map(|url| proxy_with_password(url, password))
}

/// Маршрут прокси из active.json и хранилища секретов: адрес лежит без пароля, пароль подставляется из хранилища.
pub fn proxy_settings(mroot: &Path) -> dub_llm::net::ProxySettings {
    let sel = load_selection(mroot);
    dub_llm::net::ProxySettings {
        mode: proxy_mode(&sel),
        kind: proxy_kind(&sel),
        address: proxy_address(&sel, crate::credentials::proxy_password().as_deref()),
    }
}

/// Сделать настройки прокси маршрутом всех запросов приложения (старт сервера и каждое сохранение формы).
/// Свой адрес, который не читается, — ошибка в лог: запросы идут напрямую, окно показывает причину.
pub fn apply_proxy_route(mroot: &Path) {
    let settings = proxy_settings(mroot);
    if settings.mode == dub_llm::net::ProxyMode::Custom {
        match dub_llm::net::normalize(settings.address.as_deref().unwrap_or_default(), settings.kind) {
            Ok(url) => tracing::info!("proxy: custom, {}", dub_llm::net::masked(url.as_str())),
            Err(e) => tracing::error!("proxy: the custom address is unreadable ({e:#}); requests go direct until it is fixed in the settings"),
        }
    } else {
        tracing::info!("proxy: {}", settings.mode.as_str());
    }
    dub_llm::net::set(settings);
}

/// Разбор `[scheme://][user[:password]@]host…` (схему ureq допускает опустить): (до userinfo, user, password,
/// после `@`).
fn proxy_userinfo(url: &str) -> Option<(&str, &str, Option<&str>, &str)> {
    let start = url.find("://").map_or(0, |scheme| scheme + 3);
    let rest = &url[start..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let at = rest[..authority_end].rfind('@')?;
    let (user, password) = match rest[..at].split_once(':') {
        Some((user, password)) => (user, Some(password)),
        None => (&rest[..at], None),
    };
    Some((&url[..start], user, password, &rest[at + 1..]))
}

/// Отделить пароль от адреса прокси: (адрес без пароля, пароль как есть — %XX раскодированы). Логин остаётся
/// в адресе.
pub fn split_proxy_password(url: &str) -> (String, Option<String>) {
    match proxy_userinfo(url) {
        Some((head, user, Some(password), tail)) => (
            format!("{head}{user}@{tail}"),
            Some(dub_llm::net::decode_userinfo(password)).filter(|p| !p.is_empty()),
        ),
        _ => (url.to_string(), None),
    }
}

/// Есть ли в адресе прокси логин (`user@`), к которому относится пароль.
pub fn proxy_has_user(url: &str) -> bool {
    proxy_userinfo(url).is_some_and(|(_, user, _, _)| !user.is_empty())
}

/// Подставить пароль (как есть, не %XX) в адрес с логином и без пароля; адрес со своим паролем или без логина —
/// как есть. Пароль кодируется %XX: с / ? # @ : адрес иначе читался бы с чужими хостом и портом.
pub fn proxy_with_password(url: &str, password: Option<&str>) -> String {
    match (proxy_userinfo(url), password.filter(|p| !p.is_empty())) {
        (Some((head, user, None, tail)), Some(password)) if !user.is_empty() => {
            format!("{head}{user}:{}@{tail}", dub_llm::net::encode_userinfo(password))
        }
        _ => url.to_string(),
    }
}

/// Кто переводит (stage "llm") или смотрит кадры (stage "vision").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmBackend {
    /// Своя Gemma через llama-server.
    Local,
    /// Локальный OpenAI-совместимый сервер пользователя (Ollama, LM Studio, vLLM, llama-server).
    Server,
    /// OpenRouter.
    OpenRouter,
    /// Агент, подключённый по MCP: студия спрашивает его то же, что спросила бы свою модель, с теми же
    /// инструкциями и схемой ответа (YuE2 `AssistantProvider::Agent`).
    Agent,
}

impl LlmBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            LlmBackend::Local => "local",
            LlmBackend::Server => "server",
            LlmBackend::OpenRouter => "openrouter",
            LlmBackend::Agent => "agent",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "local" => Some(LlmBackend::Local),
            "server" => Some(LlmBackend::Server),
            "openrouter" => Some(LlmBackend::OpenRouter),
            "agent" => Some(LlmBackend::Agent),
            _ => None,
        }
    }
}

/// Провайдер стадии "llm" | "vision": llm_provider / vision_provider; пока они не заданы — прежние флаги
/// or_llm_on / or_vision_on (с ключом OpenRouter), иначе своя Gemma.
pub fn llm_backend(mroot: &Path, stage: &str) -> LlmBackend {
    let key = if stage == "vision" { "vision_provider" } else { "llm_provider" };
    let sel = load_selection(mroot);
    if let Some(value) = pick(&sel, key) {
        match LlmBackend::parse(value) {
            Some(backend) => return backend,
            None => tracing::error!("{key}={value:?} in active.json is not recognized; using the old or_*_on flags"),
        }
    }
    // Прежний формат: vision шёл тем же провайдером, что перевод (or_vision_on выбирал лишь отдельную модель).
    let legacy_stage = if stage == "vision" { "llm" } else { stage };
    if openrouter_stage_on(mroot, legacy_stage) {
        LlmBackend::OpenRouter
    } else {
        LlmBackend::Local
    }
}

/// Адрес локального OpenAI-совместимого сервера по умолчанию — Ollama; LM Studio (1234), vLLM (8000) и
/// llama-server задаются в настройках.
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:11434";

/// Адрес локального сервера из настроек или адрес по умолчанию.
pub fn server_url(mroot: &Path) -> String {
    pick(&load_selection(mroot), "srv_url").unwrap_or(DEFAULT_SERVER_URL).to_string()
}

/// Модель локального сервера для стадии "llm" | "vision". Пусто — не выбрана.
pub fn server_model(mroot: &Path, stage: &str) -> String {
    let key = if stage == "vision" { "srv_vision" } else { "srv_llm" };
    pick(&load_selection(mroot), key).unwrap_or("").to_string()
}

/// Идёт ли через OpenRouter хоть одна стадия (перевод, vision, TTS, ASR) — стоит ли считать затраты.
pub fn openrouter_any_on(mroot: &Path) -> bool {
    llm_backend(mroot, "llm") == LlmBackend::OpenRouter
        || llm_backend(mroot, "vision") == LlmBackend::OpenRouter
        || (tts_provider(mroot) == "openrouter" && openrouter_stage_on(mroot, "tts"))
        || openrouter_stage_on(mroot, "asr")
}

/// Нужна ли своя Gemma: перевод или vision идут через неё.
pub fn local_gemma_needed(mroot: &Path) -> bool {
    llm_backend(mroot, "llm") == LlmBackend::Local || llm_backend(mroot, "vision") == LlmBackend::Local
}

/// Включён ли облачный путь для стадии `stage` ("llm"|"vision"|"tts"|"asr") по флагу or_*_on.
/// Требует ключ OpenRouter — без ключа облако невозможно. Для "llm"/"vision" это только прежний формат
/// настроек: провайдера стадии решает `llm_backend`.
pub fn openrouter_stage_on(mroot: &Path, stage: &str) -> bool {
    if crate::credentials::openrouter_source().is_none() {
        return false;
    }
    let sel = load_selection(mroot);
    let flag = match stage {
        "llm" => "or_llm_on",
        "vision" => "or_vision_on",
        "tts" => "or_tts_on",
        "asr" => "or_asr_on",
        _ => return false,
    };
    pick(&sel, flag) == Some("1")
}

pub fn tts_provider(mroot: &Path) -> &'static str {
    let sel = load_selection(mroot);
    match pick(&sel, "tts_provider") {
        Some("google") => "google",
        Some("openrouter") => "openrouter",
        Some("local") => "local",
        _ if pick(&sel, "or_tts_on") == Some("1") => "openrouter",
        _ => "local",
    }
}

pub fn cloud_tts_on(mroot: &Path) -> bool {
    tts_provider(mroot) != "local"
}

pub fn tts_model(mroot: &Path) -> String {
    if tts_provider(mroot) == "google" {
        pick(&load_selection(mroot), "google_tts_model")
            .unwrap_or(crate::google_tts::DEFAULT_MODEL)
            .to_string()
    } else {
        openrouter_model(mroot, "tts")
    }
}

pub fn google_tts_batch(mroot: &Path) -> bool {
    tts_provider(mroot) == "google"
        && pick(&load_selection(mroot), "google_tts_mode") == Some("batch")
}

/// id облачной модели для стадии: "llm" -> or_llm; "vision" -> or_vision, пусто -> or_llm;
/// "tts" -> or_tts_model. НИКАКОГО хардкода id — модель только из выбора юзера (динамический список из
/// API, юзер выбирает сам). Пусто -> вызывающий обязан честно упасть с понятной ошибкой «модель не выбрана».
pub fn openrouter_model(mroot: &Path, stage: &str) -> String {
    let sel = load_selection(mroot);
    match stage {
        "llm" => pick(&sel, "or_llm").unwrap_or("").to_string(),
        "vision" => pick(&sel, "or_vision").or_else(|| pick(&sel, "or_llm")).unwrap_or("").to_string(),
        "tts" => pick(&sel, "or_tts_model").unwrap_or("").to_string(),
        "asr" => pick(&sel, "or_asr").unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// Включена ли облачная транскрипция (ASR через OpenRouter) — флаг + ключ + выбранная модель.
pub fn openrouter_asr_on(mroot: &Path) -> bool {
    openrouter_stage_on(mroot, "asr") && !openrouter_model(mroot, "asr").trim().is_empty()
}

/// Сколько облачных запросов гнать параллельно (чанки в N потоков). Настройка or_concurrency, дефолт 6,
/// клэмп 1..=16 (OpenRouter держит ~50 конкурентных; 6 — безопасно и быстро, юзер может поднять).
pub fn openrouter_concurrency(mroot: &Path) -> usize {
    sel_num(mroot, "or_concurrency").map(|n| n as usize).unwrap_or(6).clamp(1, 16)
}

/// Голос облачного TTS по умолчанию (or_tts_voice). Без хардкода — пусто, если не задан (при автокастинге
/// голос подбирается по полу спикера; при отсутствии и того, и другого TTS честно падает).
pub fn openrouter_tts_voice(mroot: &Path) -> String {
    pick(&load_selection(mroot), "or_tts_voice").unwrap_or("").to_string()
}

/// БЕТА: включён ли автокастинг облачных голосов по полу спикера. ВКЛ по умолчанию (это и есть желаемое
/// поведение); ВЫКЛ ("0") -> все спикеры одним дефолтным голосом (or_tts_voice).
pub fn openrouter_autocast(mroot: &Path) -> bool {
    pick(&load_selection(mroot), "or_tts_autocast") != Some("0")
}

/// Включён ли пер-стадийный бенчмарк (галка в настройках -> active.json "bench"="1"). По умолчанию ВЫКЛ:
/// фоновый семплер NVML/sysinfo и bench.json нужны только для сравнения настроек, не в обычной работе.
pub fn bench_enabled(mroot: &Path) -> bool {
    pick(&load_selection(mroot), "bench") == Some("1")
}

/// Включён ли дакинг фона под дубляжом (приглушать фон под речью). ВЫКЛ по умолчанию — не всем нужен;
/// без него фон в дубляже звучит на полной громкости под голосом. Настройка "duck_on"="1".
pub fn duck_enabled(mroot: &Path) -> bool {
    pick(&load_selection(mroot), "duck_on") == Some("1")
}

/// Прочитать числовой слот выбора (llama_ubatch/higgs_ref_secs) из active.json. Значение может лежать
/// строкой ("256") или числом — обе формы принимаем. None/пусто -> дефолт у вызывающего.
pub fn sel_num(mroot: &Path, key: &str) -> Option<f64> {
    let v = load_selection(mroot);
    match v.get(key) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Длина реф-клипа клона голоса в секундах: настройка higgs_ref_secs (видимая в UI), дефолт 12.0.
/// Пользователь на 32ГБ RAM может уменьшить (баг-репорт: >12с не влезает в prefill Higgs, ручная резка <12с спасает).
pub fn higgs_ref_secs(mroot: &Path) -> f64 {
    sel_num(mroot, "higgs_ref_secs").filter(|s| *s > 0.0 && *s <= 60.0).unwrap_or(12.0)
}

/// Выбор ASR-движка для одной джобы: Parakeet и Nemotron на сервере audio.cpp, Parakeet Ultra на transcribe.cpp (GGUF)
/// либо CrisperWhisper на сервере audio.cpp.
#[derive(Debug, Clone)]
pub enum AsrChoice {
    /// A GGUF model run by transcribe.cpp (`dub_asr::tcpp`), on the build of the recognition backend (Parakeet Ultra:
    /// audio.cpp does not read its built-in VAD head).
    Ggml { models_root: PathBuf, build: dub_asr::tcpp::Build, model: &'static dub_asr::tcpp::Model },
    /// CrisperWhisper 2.0 turbo in the studio's audiocpp_server on the build of the recognition device (`choice`, see
    /// `acpp::build_for`); the language of «auto» is heard by VoxLingua107 on the transcribe.cpp build of that device.
    CrisperWhisper { repo_root: PathBuf, models_root: PathBuf, choice: &'static str, build: dub_asr::tcpp::Build },
    /// A model of transcribe.cpp's catalogue (Parakeet TDT v3, Nemotron 3.5) run by the studio's audiocpp_server on the
    /// build of the recognition device, reading the same GGUF; `build` is transcribe.cpp's for the language id.
    Server { repo_root: PathBuf, models_root: PathBuf, choice: &'static str, build: dub_asr::tcpp::Build, model: &'static dub_asr::tcpp::Model },
}

impl AsrChoice {
    /// Строка для лога `[models]` — видно, каким движком реально пойдёт транскрипция.
    /// ВАЖНО: участвует в param_hash ASR-стадии (analyze.rs) — только СТАБИЛЬНЫЕ токены (вариант
    /// каталога, не абсолютный путь), иначе кэш/чекпоинты инвалидируются от переноса репо между
    /// машинами/папками (ревью-находка).
    pub fn describe(&self) -> String {
        match self {
            AsrChoice::Ggml { model, .. } => format!("transcribe.cpp ({})", model.file),
            AsrChoice::CrisperWhisper { choice, .. } => format!("CrisperWhisper 2.0 turbo (intended, {choice})"),
            AsrChoice::Server { model, .. } => format!("audiocpp_server ({})", model.file),
        }
    }

    /// What the window names as the recogniser: the model, its quantization and the transcribe.cpp build, or the
    /// server's model and the device of its build.
    pub fn label(&self) -> String {
        match self {
            AsrChoice::Ggml { model, build, .. } => {
                let quant = model.file.trim_end_matches(".gguf").rsplit('-').next().unwrap_or(model.file);
                let on = match build {
                    dub_asr::tcpp::Build::Cuda => "CUDA",
                    dub_asr::tcpp::Build::Cuda12 => "CUDA 12",
                    dub_asr::tcpp::Build::Vulkan => "Vulkan",
                    dub_asr::tcpp::Build::Cpu => "CPU",
                };
                format!("{} {quant} ({on})", model.name)
            }
            AsrChoice::CrisperWhisper { choice, .. } => format!("CrisperWhisper 2.0 turbo ({})", server_device(choice)),
            AsrChoice::Server { model, choice, .. } => {
                let quant = model.file.trim_end_matches(".gguf").rsplit('-').next().unwrap_or(model.file);
                format!("{} {quant} (audio.cpp, {})", model.name, server_device(choice))
            }
        }
    }

    /// Whether it transcribes `lang` (`""` or `auto` is any).
    pub fn knows(&self, lang: &str) -> bool {
        match self {
            AsrChoice::Ggml { model, .. } | AsrChoice::Server { model, .. } => dub_asr::tcpp::knows(model, lang),
            AsrChoice::CrisperWhisper { .. } => {
                let l = lang.trim().to_ascii_lowercase();
                let primary = l.split(['-', '_']).next().unwrap_or("");
                primary.is_empty() || primary == "auto" || acpp_server::language::whisper_code(primary).is_ok()
            }
        }
    }

    /// A choice for a job that never recognises anything (verification, composition): Parakeet v3 under no folder.
    pub fn unused() -> AsrChoice {
        AsrChoice::Ggml { models_root: PathBuf::new(), build: dub_asr::tcpp::Build::Cuda, model: dub_asr::tcpp::model("parakeet-v3").expect("parakeet-v3 is in dub_asr::tcpp::MODELS") }
    }
}

/// What the window names as the device of a server build.
fn server_device(choice: &str) -> &'static str {
    match crate::acpp::build_for(choice) {
        Ok(crate::acpp::Build::Cuda13 | crate::acpp::Build::Cuda12) | Err(_) => "CUDA",
        Ok(crate::acpp::Build::Vulkan) => "Vulkan",
        Ok(crate::acpp::Build::Cpu) => "CPU",
        Ok(crate::acpp::Build::Metal) => "Metal",
    }
}

/// The device the settings choose for a stage run in audiocpp_server (`asr_backend`, `tts_backend`, `diar_backend`):
/// the stage's own choice, else the common one — `cpu`, `vulkan`, `gpu` or `auto`; [`crate::acpp::build_for`] turns it
/// into the server's build.
pub fn server_choice(mroot: &Path, key: &str) -> &'static str {
    let sel = load_selection(mroot);
    let choice = |k: &str| -> Option<&'static str> {
        match pick(&sel, k) {
            Some("cpu") => Some("cpu"),
            Some("vulkan") => Some("vulkan"),
            Some("gpu") | Some("cuda") => Some("gpu"),
            _ => None,
        }
    };
    choice(key).or_else(|| if key == "local_backend" { None } else { choice("local_backend") }).unwrap_or("auto")
}

/// Скачан ли CrisperWhisper (папка репозитория Hugging Face как есть).
fn crisperwhisper_installed(mroot: &Path) -> bool {
    mroot.join(crate::acpp::CRISPERWHISPER_DIR).join("model.safetensors").is_file()
}

/// Резолв активного ASR: выбран движок whisper и CrisperWhisper скачан — CrisperWhisper на сервере; Parakeet v3 и
/// Nemotron — на сервере audio.cpp, Parakeet Ultra — на transcribe.cpp. Нескачанный CrisperWhisper задача докачивает до
/// распознавания (ensure_job_components), так что к резолву внутри задачи он уже на диске.
pub fn resolve_asr_choice(repo_root: &Path, mroot: &Path, sel: &Value) -> AsrChoice {
    if pick(sel, "asr_engine") == Some("whisper") && crisperwhisper_installed(mroot) {
        return crisperwhisper_choice(repo_root, mroot);
    }
    let model = dub_asr::tcpp::chosen(pick(sel, "asr_engine").unwrap_or(""), pick(sel, "asr"))
        .or_else(|| dub_asr::tcpp::model("parakeet-v3"))
        .expect("parakeet-v3 is in dub_asr::tcpp::MODELS");
    if crate::acpp::SERVER_ASR.contains(&model.id) {
        return AsrChoice::Server {
            repo_root: repo_root.to_path_buf(),
            models_root: mroot.to_path_buf(),
            choice: server_choice(mroot, "asr_backend"),
            build: tcpp_build(mroot),
            model,
        };
    }
    AsrChoice::Ggml { models_root: mroot.to_path_buf(), build: tcpp_build(mroot), model }
}

/// CrisperWhisper as this job's recogniser, whatever the settings chose.
pub fn crisperwhisper_choice(repo_root: &Path, mroot: &Path) -> AsrChoice {
    AsrChoice::CrisperWhisper {
        repo_root: repo_root.to_path_buf(),
        models_root: mroot.to_path_buf(),
        choice: server_choice(mroot, "asr_backend"),
        build: tcpp_build(mroot),
    }
}

/// The components CrisperWhisper runs on here: its weights, the language id with the transcribe.cpp build it runs
/// on, and the server build of the recognition device with its CUDA runtime.
pub fn crisperwhisper_components(mroot: &Path) -> Vec<&'static str> {
    let mut ids = vec!["crisperwhisper-turbo", "langid", tcpp_build(mroot).dir()];
    ids.extend(server_components(mroot, "asr_backend"));
    ids
}

/// The server build of a stage's device (`asr_backend`, `tts_backend`, `diar_backend`) and its CUDA runtime: none
/// when DUB_STUDIO_AUDIOCPP_SERVER names a program, or when the card is chosen and runs no CUDA build (the stage says
/// so when it starts).
pub fn server_components(mroot: &Path, key: &str) -> Vec<&'static str> {
    if std::env::var_os("DUB_STUDIO_AUDIOCPP_SERVER").is_some() {
        return Vec::new();
    }
    let Ok(build) = crate::acpp::build_for(server_choice(mroot, key)) else { return Vec::new() };
    std::iter::once(crate::acpp::component(build)).chain(crate::acpp::runtime_component(build)).collect()
}

/// Распознаватель transcribe.cpp задачи: модель из кэша процесса выгружается, когда задача отпускает движок, чтобы
/// видеопамять досталась озвучке и переводу.
struct Unloading(dub_asr::tcpp::TcppEngine);

impl Drop for Unloading {
    fn drop(&mut self) {
        dub_asr::tcpp::unload();
    }
}

impl dub_asr::AsrEngine for Unloading {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<dub_asr::Segment>, dub_asr::AsrError> {
        self.0.transcribe(wav, lang)
    }
    fn transcribe_turns(&mut self, wav: &Path, turns: &[dub_asr::Turn], lang: &str) -> Result<Vec<dub_asr::SpeakerSegment>, dub_asr::AsrError> {
        self.0.transcribe_turns(wav, turns, lang)
    }
    fn transcribe_many(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<String, dub_asr::AsrError>> {
        self.0.transcribe_many(files, lang)
    }
    fn transcribe_many_words(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<dub_asr::Word>, dub_asr::AsrError>> {
        self.0.transcribe_many_words(files, lang)
    }
}

/// A recogniser in the server of a job: its weights leave the card when the job lets it go, the server stays.
struct ServerUnloading(dub_asr::acpp::AcppAsr, &'static str);

impl Drop for ServerUnloading {
    fn drop(&mut self) {
        crate::acpp::unload(&[self.1]);
    }
}

impl dub_asr::AsrEngine for ServerUnloading {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<dub_asr::Segment>, dub_asr::AsrError> {
        self.0.transcribe(wav, lang)
    }
    fn transcribe_turns(&mut self, wav: &Path, turns: &[dub_asr::Turn], lang: &str) -> Result<Vec<dub_asr::SpeakerSegment>, dub_asr::AsrError> {
        self.0.transcribe_turns(wav, turns, lang)
    }
    fn transcribe_many(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<String, dub_asr::AsrError>> {
        self.0.transcribe_many(files, lang)
    }
    fn transcribe_many_words(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<dub_asr::Word>, dub_asr::AsrError>> {
        self.0.transcribe_many_words(files, lang)
    }
}

/// Построить ASR-движок из выбора (boxed trait-object): analyze не знает деталей резолва. CrisperWhisper поднимает
/// audiocpp_server, и его отказ — ошибка с причиной.
pub fn build_engine(choice: &AsrChoice) -> Result<Box<dyn dub_asr::AsrEngine>, String> {
    build_engine_hinted(choice, None)
}

/// [`build_engine`] with words and spellings to expect in the recording (CrisperWhisper's context text; Parakeet and
/// Nemotron take none). Nemotron is told the language «auto» hears.
pub fn build_engine_hinted(choice: &AsrChoice, prompt: Option<String>) -> Result<Box<dyn dub_asr::AsrEngine>, String> {
    match choice {
        AsrChoice::Ggml { models_root, build, model } => {
            let _ = prompt;
            if matches!(build, dub_asr::tcpp::Build::Cuda | dub_asr::tcpp::Build::Cuda12) {
                if let Some(why) = gpu_stage_unavailable(models_root, "asr_backend", &t!("stage-name-asr")) {
                    return Err(why);
                }
            }
            Ok(dub_asr::tcpp::with_heard_language(Box::new(Unloading(dub_asr::tcpp::TcppEngine::new(models_root, *build, model))), models_root, *build, model))
        }
        AsrChoice::CrisperWhisper { repo_root, models_root, choice, build } => {
            let id = crate::acpp::CRISPERWHISPER;
            let server = crate::acpp::server(repo_root, models_root, crate::acpp::build_for(choice)?, id)?;
            let language = dub_asr::acpp::Language::Whisper {
                engine_dir: dub_asr::tcpp::engine_dir(models_root, *build),
                langid: dub_asr::tcpp::helper_path(models_root, &dub_asr::tcpp::LANGID),
            };
            Ok(Box::new(ServerUnloading(dub_asr::acpp::AcppAsr::new(server, id, language).with_prompt(prompt), id)))
        }
        AsrChoice::Server { repo_root, models_root, choice, build, model } => {
            let _ = prompt;
            let id = model.id;
            // Nemotron is told the language; Parakeet finds it itself
            let language = if id == crate::acpp::NEMOTRON { dub_asr::acpp::Language::Told } else { dub_asr::acpp::Language::Own };
            let server = crate::acpp::server(repo_root, models_root, crate::acpp::build_for(choice)?, id)?;
            let engine: Box<dyn dub_asr::AsrEngine> = Box::new(ServerUnloading(dub_asr::acpp::AcppAsr::new(server, id, language), id));
            Ok(dub_asr::tcpp::with_heard_language(engine, models_root, *build, model))
        }
    }
}

/// Higgs TTS: папки higgs-{q8_0,q6_k,q4_k_m}, внутри файл {q}.gguf. Возврат (каталог, квант-строка
/// для audiocpp load_model). Env DUB_STUDIO_HIGGS_MODEL (портатив) имеет приоритет. Выбранная Drama 3 —
/// (`drama3`, [`DRAMA_TTS`]) и без скачанных файлов: её загрузка скажет, чего нет, а не подменит её Higgs.
pub fn resolve_tts(mroot: &Path, sel: &Value) -> (PathBuf, String) {
    if let Ok(env) = std::env::var("DUB_STUDIO_HIGGS_MODEL") {
        let d = PathBuf::from(env);
        let q = d
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|n| n.strip_prefix("higgs-"))
            .unwrap_or("q8_0")
            .to_string();
        return (d, q);
    }
    if pick(sel, "tts") == Some(DRAMA_TTS) {
        return (mroot.join(DRAMA_TTS), DRAMA_TTS.to_string());
    }
    if pick(sel, "tts") == Some(SUPERTONIC_TTS) {
        return (crate::supertonic::dir(mroot), SUPERTONIC_TTS.to_string());
    }
    if let Some(choice) = pick(sel, "tts").filter(|choice| qwen_quant(choice).is_some()) {
        return (qwentts::dir(mroot), choice.to_string());
    }
    let has = |q: &str| {
        mroot.join(format!("higgs-{q}")).join(format!("{q}.gguf")).is_file()
    };
    let ret = |q: &str| (mroot.join(format!("higgs-{q}")), q.to_string());
    if let Some(q) = pick(sel, "tts") {
        if has(q) {
            return ret(q);
        }
    }
    for q in ["q8_0", "q6_k", "q4_k_m"] {
        if has(q) {
            return ret(q);
        }
    }
    ret("q8_0") // дефолт-fallback (может ещё не быть скачан)
}

/// Supertonic 3 в слоте `tts`: десять готовых голосов, без клона.
pub const SUPERTONIC_TTS: &str = "supertonic3";

/// Квант Qwen3-TTS по значению слота `tts` (`qwen3-tts` — Q4_K_M, `qwen3-tts-q8_0`); None — выбран не Qwen.
pub fn qwen_quant(tts: &str) -> Option<&'static str> {
    match tts {
        "qwen3-tts" => Some("Q4_K_M"),
        "qwen3-tts-q8_0" => Some("Q8_0"),
        _ => None,
    }
}

/// Почему Qwen3-TTS на этой машине не пойдёт: его ggml считает на CUDA 13 (cuBLAS из models/higgs-engine).
pub fn qwen_unavailable() -> Option<String> {
    if cfg!(not(windows)) {
        return Some(t!("qwen-no-linux"));
    }
    if let Some(why) = crate::hw::cpu_floor("Qwen3-TTS") {
        return Some(why);
    }
    let gpu = crate::hw::gpu_report();
    if gpu.cuda13_ok {
        return None;
    }
    Some(t!("qwen-needs-cuda", card = gpu.summary().unwrap_or_else(|| "-".to_string())))
}

/// Значение слота `tts` и папка весов Drama 3 (apiplant/drama-q4) в каталоге моделей.
pub const DRAMA_TTS: &str = "drama3";

/// Видеопамять, в которую Drama 3 входит целиком: карты «24 ГБ» проходят, 20-гиговые — нет. На меньшей карте эксперты
/// модели (18 ГБ из 21) живут в оперативной памяти, а видеокарта держит кэш нужных сейчас (движок engine-0.1.3+).
pub const DRAMA_FULL_VRAM: u64 = 23 << 30;
/// Карта «6 ГБ»: остальной модели нужно около 4,8 ГБ, кэшу экспертов остаётся немного; карта «4 ГБ» не годится.
pub const DRAMA_MIN_VRAM: u64 = 11 << 29;
/// Наименьший лимит видеопамяти в настройках, ГБ: меньше — кэшу экспертов рядом с остальной моделью места нет.
pub const DRAMA_MIN_LIMIT_GB: u32 = 5;
/// Эксперты в оперативной памяти — 18 ГБ закреплённой памяти: машинам «32 ГБ» хватает, «16 ГБ» — нет.
pub const DRAMA_MIN_RAM: u64 = 28 << 30;

/// Видеопамять для Drama 3 из настроек (`drama_vram_gb`), МБ; 0 — авто: вся модель, если входит в свободную
/// видеопамять, иначе свободная без шестнадцатой доли (256 МБ–1 ГБ). Лимит не меньше полной модели держит её целиком.
pub fn drama_vram_limit_mb(mroot: &Path) -> u32 {
    pick(&load_selection(mroot), "drama_vram_gb").and_then(|v| v.parse::<u32>().ok()).map_or(0, |gb| gb.saturating_mul(1024))
}

/// Веса и движок Drama 3 на месте (последний слой весов и DLL).
pub fn drama_installed(mroot: &Path) -> bool {
    mroot.join(DRAMA_TTS).join("layer39.safetensors").is_file() && drama_dll(mroot).is_file()
}

pub fn drama_dll(mroot: &Path) -> PathBuf {
    mroot.join("drama-engine").join(if cfg!(windows) {
        "drama_engine.dll"
    } else if cfg!(target_os = "macos") {
        "libdrama_engine.dylib"
    } else {
        "libdrama_engine.so"
    })
}

/// Почему Drama 3 на этой машине не пойдёт: None — карта NVIDIA с CUDA 13 не меньше [`DRAMA_MIN_VRAM`], и если модель
/// не входит в видеопамять целиком (карта меньше [`DRAMA_FULL_VRAM`] или такой лимит в настройках), оперативной
/// памяти не меньше [`DRAMA_MIN_RAM`].
pub fn drama_unavailable(mroot: &Path) -> Option<String> {
    if let Some(why) = crate::hw::cpu_floor("Drama 3") {
        return Some(why);
    }
    let gpu = crate::hw::gpu_report();
    let card = gpu.summary().unwrap_or_else(|| "-".to_string());
    if !gpu.cuda13_ok {
        return Some(t!("drama-needs-cuda", card = card));
    }
    let gb = |bytes: u64| format!("{:.0}", bytes as f64 / (1u64 << 30) as f64);
    let vram = crate::hw::total_vram();
    if vram < DRAMA_MIN_VRAM {
        return Some(t!("drama-needs-vram", card = card, gb = gb(vram)));
    }
    let limit = drama_vram_limit_mb(mroot) as u64 * (1 << 20);
    if vram < DRAMA_FULL_VRAM || (limit > 0 && limit < DRAMA_FULL_VRAM) {
        let ram = crate::hw::total_ram();
        if ram < DRAMA_MIN_RAM {
            return Some(t!("drama-needs-ram", gb = gb(ram)));
        }
    }
    None
}

/// Roformer сепарация: models/bsroformer/voc_fv6-{Q8_0,Q5_0,Q4_0}.gguf (все в одном каталоге).
/// Env DUB_STUDIO_BSROFORMER_MODEL имеет приоритет.
pub fn resolve_sep(mroot: &Path, sel: &Value) -> PathBuf {
    if let Ok(env) = std::env::var("DUB_STUDIO_BSROFORMER_MODEL") {
        return PathBuf::from(env);
    }
    let f = |q: &str| mroot.join("bsroformer").join(format!("voc_fv6-{q}.gguf"));
    if let Some(q) = pick(sel, "sep") {
        let p = f(q);
        if p.is_file() {
            return p;
        }
    }
    for q in ["Q8_0", "Q5_0", "Q4_0"] {
        let p = f(q);
        if p.is_file() {
            return p;
        }
    }
    f("Q8_0")
}

/// Gemma MT + vision: папки mt-q8_0/mt-q6_k/mt-q5_0 + mt (q4_0-дефолт). Возврат (модель, mmproj).
/// Каталог годится, только если есть И модель, И mmproj (полускачанный игнорируется). Имя файла не
/// важно — берём любой .gguf (mmproj по подстроке). Env-root уже учтён в mroot.
pub fn resolve_mt(mroot: &Path, sel: &Value) -> (PathBuf, PathBuf) {
    let dir_for = |q: &str| {
        if q == "q4_0" { mroot.join("mt") } else { mroot.join(format!("mt-{q}")) }
    };
    let find = |dir: &Path, want_mmproj: bool| -> Option<PathBuf> {
        let mut hit: Option<PathBuf> = None;
        for e in std::fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("gguf") {
                continue;
            }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if name.contains("mmproj") == want_mmproj
                && hit.as_ref().map(|h| p < *h).unwrap_or(true)
            {
                hit = Some(p);
            }
        }
        hit
    };
    let try_dir = |q: &str| {
        let d = dir_for(q);
        match (find(&d, false), find(&d, true)) {
            (Some(m), Some(mm)) => Some((m, mm)),
            _ => None,
        }
    };
    if let Some(q) = pick(sel, "mt") {
        if let Some(r) = try_dir(q) {
            return r;
        }
    }
    for q in ["q8_0", "q6_k", "q5_0", "q4_0"] {
        if let Some(r) = try_dir(q) {
            return r;
        }
    }
    (
        mroot.join("mt").join("gemma-4-12b-it-qat-q4_0.gguf"),
        mroot.join("mt").join("mmproj-gemma-4-12b-it-qat-q4_0.gguf"),
    )
}

#[cfg(test)]
mod asr_variant_tests {
    use super::*;

    #[test]
    fn parakeet_components_map_to_their_asr_slot() {
        for (id, variant) in [("parakeet", "int8"), ("parakeet-ultra", "ultra-int8")] {
            let sel = component_selection(id);
            assert_eq!(sel, vec![("asr_engine", "parakeet".to_string()), ("asr", variant.to_string())], "{id}");
        }
    }

    fn file_of(sel: Value) -> &'static str {
        match resolve_asr_choice(Path::new("."), Path::new("no-models"), &sel) {
            AsrChoice::Ggml { model, .. } | AsrChoice::Server { model, .. } => model.file,
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn parakeet_v3_and_nemotron_run_in_the_server_and_ultra_on_transcribe_cpp() {
        let on = |sel: Value| match resolve_asr_choice(Path::new("."), Path::new("no-models"), &sel) {
            AsrChoice::Server { model, .. } => format!("server {}", model.id),
            AsrChoice::Ggml { model, .. } => format!("transcribe.cpp {}", model.id),
            AsrChoice::CrisperWhisper { .. } => "crisperwhisper".to_string(),
        };
        assert_eq!(on(serde_json::json!({})), "server parakeet-v3");
        assert_eq!(on(serde_json::json!({ "asr_engine": "nemotron" })), "server nemotron");
        assert_eq!(on(serde_json::json!({ "asr": "ultra-int8" })), "transcribe.cpp parakeet-ultra");
    }

    #[test]
    fn each_selected_variant_resolves_to_its_model() {
        assert_eq!(file_of(serde_json::json!({})), "parakeet-tdt-0.6b-v3-Q8_0.gguf");
        assert_eq!(file_of(serde_json::json!({ "asr": "fp32" })), "parakeet-tdt-0.6b-v3-Q8_0.gguf", "the fp32 choice of 4.3 and earlier");
        assert_eq!(file_of(serde_json::json!({ "asr": "ultra" })), "parakeet-ultra-Q8_0.gguf", "the Ultra fp32 choice of 4.3 and earlier");
        assert_eq!(file_of(serde_json::json!({ "asr": "ultra-int8" })), "parakeet-ultra-Q8_0.gguf");
    }
}

#[cfg(test)]
mod resolve_live_tests {
    use super::*;

    // Диагностический (марафон QC): резолв ASR на живых путях репо. На CI без models/ — скип.
    #[test]
    fn resolve_asr_choice_on_live_repo() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap();
        let mroot = repo.join("models");
        if !mroot.join("active.json").is_file() {
            eprintln!("skip: нет models/active.json");
            return;
        }
        let sel = load_selection(&mroot);
        let choice = resolve_asr_choice(repo, &mroot, &sel);
        eprintln!("sel = {}", redact_selection(&sel, false, false));
        eprintln!("resolved = {}", choice.describe());
        if pick(&sel, "asr_engine") == Some("whisper") && crisperwhisper_installed(&mroot) {
            assert!(
                choice.describe().starts_with("CrisperWhisper"),
                "active.json просит whisper (и CrisperWhisper скачан), но резолв дал: {}", choice.describe()
            );
        }
    }
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_proxy_password_is_split_off_and_put_back() {
        assert_eq!(
            split_proxy_password("http://alice:p%40ss:w@proxy.lan:3128"),
            ("http://alice@proxy.lan:3128".to_string(), Some("p@ss:w".to_string()))
        );
        assert_eq!(split_proxy_password("http://alice:p@ss@proxy.lan:3128").1.as_deref(), Some("p@ss"), "a raw @ as older versions wrote it");
        assert_eq!(split_proxy_password("socks5://proxy.lan:1080"), ("socks5://proxy.lan:1080".to_string(), None));
        assert_eq!(split_proxy_password("http://alice@proxy.lan:3128"), ("http://alice@proxy.lan:3128".to_string(), None));
        assert_eq!(split_proxy_password("http://alice:@proxy.lan:3128"), ("http://alice@proxy.lan:3128".to_string(), None));
        assert_eq!(proxy_with_password("http://alice@proxy.lan:3128", Some("p@ss:w")), "http://alice:p%40ss%3Aw@proxy.lan:3128");
        assert_eq!(proxy_with_password("http://alice@proxy.lan:3128", Some("pa/ss?#")), "http://alice:pa%2Fss%3F%23@proxy.lan:3128");
        for password in ["p@ss:w", "pa/ss?#", "100%", "пароль"] {
            let (_, back) = split_proxy_password(&proxy_with_password("http://alice@proxy.lan:3128", Some(password),
            ));
            assert_eq!(back.as_deref(), Some(password));
        }
        assert_eq!(proxy_with_password("http://alice:own@proxy.lan:3128", Some("stored")), "http://alice:own@proxy.lan:3128");
        assert_eq!(proxy_with_password("http://proxy.lan:3128", Some("stored")), "http://proxy.lan:3128");
        assert_eq!(proxy_with_password("http://alice@proxy.lan:3128", None), "http://alice@proxy.lan:3128");
        assert!(proxy_has_user("http://alice@proxy.lan:3128") && !proxy_has_user("http://proxy.lan:3128"));
        assert!(!proxy_has_user("http://proxy.lan:3128/path@x"));
        assert_eq!(split_proxy_password("alice:hunter2@proxy.lan:3128"), ("alice@proxy.lan:3128".to_string(), Some("hunter2".to_string())));
    }

    #[test]
    fn the_public_selection_carries_flags_instead_of_secrets() {
        let selection = json!({
            "tts": "q6_k",
            "or_key": "sk-or-v1-secret",
            "proxy_on": "1",
            "proxy_url": "http://alice:hunter2@proxy.lan:3128",
        });
        let public = redact_selection(&selection, true, false);
        let text = public.to_string();
        assert!(!text.contains("sk-or-v1-secret") && !text.contains("hunter2") && !text.contains("\"or_key\""));
        assert_eq!(public["proxy_url"], "http://alice@proxy.lan:3128");
        assert_eq!(public["or_key_set"], true);
        assert_eq!(public["proxy_password_set"], true, "a legacy inline password still counts as set");
        assert_eq!(public["tts"], "q6_k");

        let bare = redact_selection(&json!({ "proxy_url": "socks5://proxy.lan:1080" }), false, false,
        );
        assert_eq!(bare["or_key_set"], false);
        assert_eq!(bare["proxy_password_set"], false);
        assert_eq!(bare["proxy_url"], "socks5://proxy.lan:1080");
    }

    #[test]
    fn secrets_are_not_selection_slots() {
        assert!(!is_selection_key("or_key") && !is_selection_key("proxy_url") && !is_selection_key("srv_key"));
        assert!(is_selection_key("or_llm_on") && is_selection_key("llm_provider") && is_selection_key("srv_url"));
        assert!(!is_selection_key("proxy_on") && !is_selection_key("proxy_mode"), "the proxy changes only through its form");
        assert!(is_selection_value("vision_provider", "server") && !is_selection_value("llm_provider", "ollama"));
    }
}

#[cfg(test)]
mod provider_tests {
    #[test]
    fn drama_runs_below_24_gb_with_its_experts_in_ram_and_is_refused_without_enough_of_either() {
        let root = scratch("drama-limits");
        let why = |vram: &str, ram: &str| {
            std::env::set_var("DUB_STUDIO_SIMULATE_VRAM_GB", vram);
            std::env::set_var("DUB_STUDIO_SIMULATE_RAM_GB", ram);
            let why = drama_unavailable(&root);
            std::env::remove_var("DUB_STUDIO_SIMULATE_VRAM_GB");
            std::env::remove_var("DUB_STUDIO_SIMULATE_RAM_GB");
            why
        };
        if !crate::hw::gpu_report().cuda13_ok {
            assert!(why("16", "64").is_some(), "no CUDA 13 card: refused");
            return;
        }
        assert_eq!(why("16", "64"), None, "16 GB card, 64 GB of RAM: the experts go to RAM");
        assert_eq!(why("24", "8"), None, "24 GB card: the whole model on it, RAM does not matter");
        assert_eq!(why("6", "64"), None, "6 GB card, 64 GB of RAM: a small expert cache");
        assert!(why("4", "64").expect("4 GB card: no room for the expert cache").contains('4'));
        assert!(why("16", "16").expect("16 GB card, 16 GB of RAM: the experts do not fit in RAM").contains("16"));
        set_selection(&root, "drama_vram_gb", "8").unwrap();
        assert_eq!(drama_vram_limit_mb(&root), 8192);
        assert!(why("24", "16").is_some(), "8 GB limit on a 24 GB card puts the experts in RAM: 16 GB of RAM is too little");
        assert!(is_selection_value("drama_vram_gb", "0") && is_selection_value("drama_vram_gb", "6") && !is_selection_value("drama_vram_gb", "4"));
        std::fs::remove_dir_all(&root).ok();
    }

    use super::*;
    use dub_llm::net::{ProxyKind, ProxyMode};
    use serde_json::json;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dub-models-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn each_stage_has_its_own_provider() {
        let root = scratch("backend");
        assert_eq!(llm_backend(&root, "llm"), LlmBackend::Local);
        set_selection(&root, "llm_provider", "openrouter").unwrap();
        set_selection(&root, "vision_provider", "server").unwrap();
        assert_eq!(llm_backend(&root, "llm"), LlmBackend::OpenRouter);
        assert_eq!(llm_backend(&root, "vision"), LlmBackend::Server);
        assert!(!local_gemma_needed(&root));
        set_selection(&root, "vision_provider", "local").unwrap();
        assert!(local_gemma_needed(&root));
        assert_eq!(server_url(&root), DEFAULT_SERVER_URL);
        set_selection(&root, "srv_url", "http://192.168.1.5:1234/v1").unwrap();
        set_selection(&root, "srv_vision", "qwen2.5-vl").unwrap();
        assert_eq!(server_url(&root), "http://192.168.1.5:1234/v1");
        assert_eq!(server_model(&root, "vision"), "qwen2.5-vl");
        assert_eq!(server_model(&root, "llm"), "");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_old_proxy_switch_reads_as_a_mode() {
        let custom = json!({ "proxy_on": "1", "proxy_url": "http://proxy.lan:3128" });
        assert_eq!(proxy_mode(&custom), ProxyMode::Custom);
        assert_eq!(proxy_mode(&json!({ "proxy_on": "0", "proxy_url": "http://proxy.lan:3128" })), ProxyMode::System);
        assert_eq!(proxy_mode(&json!({ "proxy_on": "1" })), ProxyMode::System, "on without an address was never a route");
        assert_eq!(proxy_mode(&json!({})), ProxyMode::System);
        assert_eq!(proxy_mode(&json!({ "proxy_mode": "off", "proxy_on": "1", "proxy_url": "h:1" })), ProxyMode::Off);
        assert_eq!(proxy_kind(&json!({ "proxy_kind": "socks5" })), ProxyKind::Socks5);
        assert_eq!(proxy_kind(&json!({})), ProxyKind::Http);
    }
}
