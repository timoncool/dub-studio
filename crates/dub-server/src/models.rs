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
    std::fs::rename(&tmp, mroot.join("active.json"))
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
        "parakeet" => vec![("asr_engine", "parakeet".into()), ("asr", "int8".into())],
        "parakeet-fp32" => vec![("asr_engine", "parakeet".into()), ("asr", "fp32".into())],
        "parakeet-ultra" => vec![("asr_engine", "parakeet".into()), ("asr", "ultra".into())],
        "parakeet-ultra-int8" => vec![("asr_engine", "parakeet".into()), ("asr", "ultra-int8".into())],
        "whisper-tiny" => vec![("asr_engine", "whisper".into()), ("whisper_model", "tiny".into()),
        ],
        "whisper-base" => vec![("asr_engine", "whisper".into()), ("whisper_model", "base".into()),
        ],
        "whisper-small" => vec![("asr_engine", "whisper".into()), ("whisper_model", "small".into()),
        ],
        "whisper-medium" => vec![("asr_engine", "whisper".into()), ("whisper_model", "medium".into()),
        ],
        "whisper-large-v3" => vec![("asr_engine", "whisper".into()), ("whisper_model", "large-v3".into()),
        ],
        "whisper-large-v3-turbo" => {
            vec![("asr_engine", "whisper".into()), ("whisper_model", "large-v3-turbo".into()),
            ]
        }
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
    matches!(
        key,
        "tts" | "asr" | "mt" | "sep" | "asr_engine" | "whisper_model" | "whisper_compute" | "whisper_device"
            // Backend КАЖДОЙ локальной стадии независимо (auto|gpu|cpu): любой движок на любой инстанс.
            // gpu = CUDA, cpu = без NVIDIA. sep=сепарация(BSRoformer CUDA/CPU-сборка), diar=диаризация
            // (Sortformer onnx CUDA-EP/CPU), asr=локальный ASR (Parakeet onnx / Whisper CTranslate2).
            | "local_backend" | "sep_backend" | "diar_backend" | "asr_backend"
            // Лимиты RAM (видимые контролы в настройках, НЕ авто-магия): против OOM на слабой памяти.
            | "llama_ubatch"    // размер prefill-батча Gemma (меньше = меньше пиковый буфер графа prefill)
            | "higgs_ref_secs"  // длина реф-клипа клона голоса (меньше = меньше prefill Higgs; <12с спасает 32ГБ)
            | "bench"           // пер-стадийный бенчмарк (bench.json + ⏱ в журнале); галка в настройках, ВЫКЛ по умолчанию
            | "duck_on"         // дакинг фона под дубляжом (приглушать фон под речью); ВЫКЛ по умолчанию — не всем нужен
            | "qc_asr"          // "1" -> авто-проверка услышанного текста через Whisper ASR; "0" -> выкл (быстрый синтез)
            | "qc_duration"     // "1" -> строгий контроль длительности/растяжения; "0" -> без ограничений
            | "multitake"       // "1" -> генерировать 3 дубля каждой фразы и выбирать лучший по таймингу; "0" -> один дубль (быстро)
            | "breath_on"       // "1" -> авто-вставка легких вдохов в паузах между фразами; "0" -> выкл
            | "speech_rate_on"  // "1" -> адаптация темпа генерации TTS под длину текста/слота; "0" -> дефолт темп
            | "emo_ref_on"      // "1" -> эмоциональный референс сцены (перенос эмоций из оригинального вокала); "0" -> выкл
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
        "google_tts_mode" => matches!(value, "standard" | "batch"),
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
/// под CUDA 13 (hw::gpu_report) — на драйвере до 580 или Pascal «авто» идёт на CPU, а не падает на CUDA.
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
        .unwrap_or(if crate::hw::gpu_report().cuda13_ok { "gpu" } else { "cpu" })
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
        tracing::error!("proxy_mode {mode:?} в active.json не распознан — считаю «как в Windows»");
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
            Ok(url) => tracing::info!("прокси: свой, {}", dub_llm::net::masked(url.as_str())),
            Err(e) => tracing::error!("прокси: свой адрес не читается ({e:#}) — запросы идут напрямую, пока его не исправят в настройках"),
        }
    } else {
        tracing::info!("прокси: {}", settings.mode.as_str());
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
}

impl LlmBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            LlmBackend::Local => "local",
            LlmBackend::Server => "server",
            LlmBackend::OpenRouter => "openrouter",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "local" => Some(LlmBackend::Local),
            "server" => Some(LlmBackend::Server),
            "openrouter" => Some(LlmBackend::OpenRouter),
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
            None => tracing::error!("{key}={value:?} в active.json не распознан — беру прежние флаги or_*_on"),
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
            .unwrap_or("")
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

/// Выбор ASR-движка для одной джобы: Parakeet (каталог TDT) либо Whisper (бинарь + модель + квант + девайс).
#[derive(Debug, Clone)]
pub enum AsrChoice {
    Parakeet(PathBuf),
    Whisper { bin: PathBuf, model_dir: PathBuf, model: String, compute: String, device: String,
    },
}

impl AsrChoice {
    /// Строка для лога `[models]` — видно, каким движком реально пойдёт транскрипция.
    /// ВАЖНО: участвует в param_hash ASR-стадии (analyze.rs) — только СТАБИЛЬНЫЕ токены (вариант
    /// каталога, не абсолютный путь), иначе кэш/чекпоинты инвалидируются от переноса репо между
    /// машинами/папками (ревью-находка).
    pub fn describe(&self) -> String {
        match self {
            AsrChoice::Parakeet(d) => {
                let variant = d.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "tdt".into());
                format!("Parakeet ({variant})")
            }
            AsrChoice::Whisper { model, compute, device, .. } => {
                format!("Whisper {model} (compute={compute}, device={device})")
            }
        }
    }
}

/// Путь к бинарю Whisper: env DUB_STUDIO_WHISPER_BIN, иначе приоритетом XXL-сборка
/// (<repo>/tools/whisper/Faster-Whisper-XXL/faster-whisper-xxl.exe — свежий движок, CUDA-DLL в
/// комплекте (_xxl_data), умеет --batched), фолбэк — старый onefile whisper-faster.exe (CPU).
pub fn whisper_bin(repo_root: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("DUB_STUDIO_WHISPER_BIN") {
        return PathBuf::from(p);
    }
    let wdir = repo_root.join("tools").join("whisper");
    if cfg!(windows) {
        let xxl = wdir.join("Faster-Whisper-XXL").join("faster-whisper-xxl.exe");
        if xxl.is_file() {
            return xxl;
        }
        return wdir.join("whisper-faster.exe");
    }
    wdir.join("whisper-faster")
}

/// Каталог Whisper-моделей: <mroot>/whisper (внутри — faster-whisper-<size>). Есть ли модель на диске.
fn whisper_model_installed(mroot: &Path, size: &str) -> bool {
    mroot.join("whisper").join(format!("faster-whisper-{size}")).join("model.bin").is_file()
}

/// Резолв активного ASR: если выбран движок whisper И бинарь+модель на диске — Whisper (модель = выбор,
/// иначе первый установленный по убыванию качества); иначе — Parakeet (существующий резолв каталога TDT).
/// Так «выбрал Whisper + скачал модель» применяется без рестарта, а недо-настроенный Whisper тихо
/// откатывается на Parakeet (analyze не падает).
pub fn resolve_asr_choice(repo_root: &Path, mroot: &Path, sel: &Value) -> AsrChoice {
    if pick(sel, "asr_engine") == Some("whisper") {
        let bin = whisper_bin(repo_root);
        // выбранная модель, если скачана; иначе — лучшая из установленных.
        let want = pick(sel, "whisper_model").filter(|m| whisper_model_installed(mroot, m));
        let model = want.map(String::from).or_else(|| {
            ["large-v3-turbo", "large-v3", "medium", "small", "base", "tiny",
            ]
                .into_iter()
                .find(|m| whisper_model_installed(mroot, m))
                .map(String::from)
        });
        if let (true, Some(model)) = (bin.is_file(), model) {
            // Девайс Whisper: авто по ФАКТУ наличия CUDA-либ. whisper-faster (CTranslate2, CUDA 11)
            // требует cublas64_11 + cudnn8 РЯДОМ С EXE (официальный Purfview: GPU execution requires
            // cuBLAS and cuDNN libs next to the executable). CUDA-13 DLL Higgs'а ему не подходят —
            // имена версионные (cublas64_13 ≠ cublas64_11), cuDNN в дистрибутиве нет вообще. Поэтому:
            // либы лежат -> cuda (GPU в разы быстрее на длинных), нет -> честный cpu БЕЗ попыток и
            // фолбэков. Явная настройка whisper_device перекрывает авто-детект.
            // Backend стадии ASR перекрывает авто: cpu -> строго cpu; иначе cuda если либы рядом.
            let auto_dev = if stage_backend(mroot, "asr_backend") == "cpu" {
                "cpu"
            } else if whisper_cuda_libs_present(&bin) {
                "cuda"
            } else {
                "cpu"
            };
            let device = pick(sel, "whisper_device").unwrap_or(auto_dev).to_string();
            // Квант: на GPU дефолт float16 (родной для тензорных ядер), на CPU — int8.
            let mut compute = pick(sel, "whisper_compute")
                .unwrap_or(if device == "cuda" { "float16" } else { "int8" })
                .to_string();
            // ГАРД: float16/bfloat16 не поддерживаются на CPU — CTranslate2 роняет процесс с
            // "Requested float16 compute type, but the target device do not support efficient float16".
            // На cpu коэрсим GPU-only кванты в безопасный int8, чтобы транскрипция не падала.
            if device == "cpu"
                && matches!(compute.as_str(), "float16" | "bfloat16" | "int8_float16" | "int8_bfloat16")
            {
                compute = "int8".to_string();
            }
            return AsrChoice::Whisper { bin, model_dir: mroot.join("whisper"), model, compute, device,
            };
        }
    }
    AsrChoice::Parakeet(resolve_asr(mroot, sel))
}

/// Лежат ли рядом с whisper-faster.exe CUDA-библиотеки CTranslate2 (cuBLAS 11/12 + cuDNN).
/// Ровно те имена, что требует движок; без них cuda-запуск гарантированно падает.
/// XXL-сборка — self-contained: cublas64_12 + cudnn64_8 лежат внутри её _xxl_data (проверено по
/// содержимому архива r245.4) -> для неё сразу true.
fn whisper_cuda_libs_present(bin: &std::path::Path) -> bool {
    if bin
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("faster-whisper-xxl.exe"))
    {
        return true;
    }
    let Some(dir) = bin.parent() else { return false;
    };
    let cublas = dir.join("cublas64_11.dll").is_file() || dir.join("cublas64_12.dll").is_file();
    let cudnn = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.starts_with("cudnn") && n.ends_with(".dll")
            })
        })
        .unwrap_or(false);
    cublas && cudnn
}

/// Построить ASR-движок из выбора (boxed trait-object): analyze не знает деталей резолва.
pub fn build_engine(choice: &AsrChoice) -> Box<dyn dub_asr::AsrEngine> {
    match choice {
        AsrChoice::Parakeet(dir) => Box::new(dub_asr::Asr::new(dir)),
        AsrChoice::Whisper { bin, model_dir, model, compute, device,
        } => Box::new(dub_asr::WhisperAsr::new(bin, model_dir, model, compute, device,
        )),
    }
}

/// Higgs TTS: папки higgs-{q8_0,q6_k,q4_k_m}, внутри файл {q}.gguf. Возврат (каталог, квант-строка
/// для audiocpp load_model). Env DUB_STUDIO_HIGGS_MODEL (портатив) имеет приоритет.
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

/// Parakeet ASR: каталоги tdt (int8) / tdt-fp32 / tdt-ultra (Parakeet Ultra, fp32) / tdt-ultra-int8. from_pretrained сам
/// различает имена файлов внутри. Без выбора — int8 (дефолт). Env DUB_STUDIO_TDT имеет приоритет.
pub fn resolve_asr(mroot: &Path, sel: &Value) -> PathBuf {
    if let Ok(env) = std::env::var("DUB_STUDIO_TDT") {
        return PathBuf::from(env);
    }
    resolve_asr_dir(mroot, sel)
}

fn resolve_asr_dir(mroot: &Path, sel: &Value) -> PathBuf {
    let fp32 = mroot.join("tdt-fp32");
    let int8 = mroot.join("tdt");
    let ultra = mroot.join("tdt-ultra");
    let ultra_int8 = mroot.join("tdt-ultra-int8");
    let fp32_ok = fp32.join("encoder-model.onnx").is_file();
    let int8_ok = int8.join("encoder-model.int8.onnx").is_file();
    let ultra_ok = ultra.join("encoder-model.onnx").is_file();
    let ultra_int8_ok = ultra_int8.join("encoder-model.int8.onnx").is_file();
    match pick(sel, "asr") {
        Some("fp32") if fp32_ok => fp32,
        Some("int8") if int8_ok => int8,
        Some("ultra") if ultra_ok => ultra,
        Some("ultra-int8") if ultra_int8_ok => ultra_int8,
        _ if int8_ok => int8,
        _ if fp32_ok => fp32,
        _ if ultra_ok => ultra,
        _ if ultra_int8_ok => ultra_int8,
        _ => int8,
    }
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
        for (id, variant) in [("parakeet", "int8"), ("parakeet-fp32", "fp32"), ("parakeet-ultra", "ultra"),
            ("parakeet-ultra-int8", "ultra-int8"),
        ] {
            let sel = component_selection(id);
            assert_eq!(sel, vec![("asr_engine", "parakeet".to_string()), ("asr", variant.to_string())], "{id}");
        }
    }

    struct TmpModels(PathBuf);
    impl TmpModels {
        fn new(tag: &str, dirs: &[(&str, &str)]) -> Self {
            let root = std::env::temp_dir().join(format!("dub-asr-variant-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (dir, file) in dirs {
                std::fs::create_dir_all(root.join(dir)).unwrap();
                std::fs::write(root.join(dir).join(file), b"x").unwrap();
            }
            TmpModels(root)
        }
    }
    impl Drop for TmpModels {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ALL: &[(&str, &str)] = &[
        ("tdt", "encoder-model.int8.onnx"),
        ("tdt-fp32", "encoder-model.onnx"),
        ("tdt-ultra", "encoder-model.onnx"),
        ("tdt-ultra-int8", "encoder-model.int8.onnx"),
    ];

    fn sel(asr: &str) -> Value {
        serde_json::json!({ "asr": asr })
    }

    fn leaf(p: &Path) -> String {
        p.file_name().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn each_selected_variant_resolves_to_its_folder() {
        let m = TmpModels::new("all", ALL);
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &sel("int8"))), "tdt");
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &sel("fp32"))), "tdt-fp32");
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &sel("ultra"))), "tdt-ultra");
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &sel("ultra-int8"))), "tdt-ultra-int8");
    }

    #[test]
    fn default_stays_int8_when_ultra_installed() {
        let m = TmpModels::new("default", ALL);
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &serde_json::json!({}))), "tdt");
    }

    #[test]
    fn ultra_selected_but_missing_falls_to_installed() {
        let m = TmpModels::new("missing", &ALL[..1]);
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &sel("ultra"))), "tdt");
    }

    #[test]
    fn only_ultra_installed_is_used() {
        let m = TmpModels::new("only", &ALL[2..3]);
        assert_eq!(leaf(&resolve_asr_dir(&m.0, &serde_json::json!({}))), "tdt-ultra");
        assert_eq!(
            AsrChoice::Parakeet(resolve_asr_dir(&m.0, &sel("ultra"))).describe(),
            "Parakeet (tdt-ultra)"
        );
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
        // Ассертим Whisper только когда он РЕАЛЬНО установлен: резолв по контракту тихо откатывается
        // на Parakeet без бинаря/модели (ревью: иначе тест ложно валится на машине без whisper).
        let whisper_ready = whisper_bin(repo).is_file()
            && ["large-v3-turbo", "large-v3", "medium", "small", "base", "tiny",
            ]
                .iter()
                .any(|m| whisper_model_installed(&mroot, m));
        if pick(&sel, "asr_engine") == Some("whisper") && whisper_ready {
            assert!(
                choice.describe().starts_with("Whisper"),
                "active.json просит whisper (и он установлен), но резолв дал: {}", choice.describe()
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
