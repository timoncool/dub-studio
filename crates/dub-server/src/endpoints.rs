//! Остаток REST-контракта (раунд 5): каталоги (/fonts /voices /presets), PATCH /engine/opts,
//! POST /remix (Gemma переписывает весь транскрипт), PUT /projects/{pid} (undo/redo снапшот),
//! GET /waveform (пики аудио), GET /preview?t&rev + GET /original?t (ОДИН PNG-кадр через GPU-воркер).
//! Порт соответствующих ручек backend/app.py 1:1.

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use dub_core::Project;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{jobs, AppState};

// ─── GET /fonts ─────────────────────────────────────────────────────────────
pub async fn fonts() -> Json<Value> {
    Json(json!({ "fonts": dub_captions::fonts_catalog() }))
}


// ─── GET /presets ───────────────────────────────────────────────────────────
pub async fn presets() -> Json<Value> {
    let (presets, reveals) = dub_captions::presets_catalog();
    Json(json!({ "presets": presets, "reveals": reveals }))
}

// ─── GET /engine/openrouter/models?kind=llm|vision|tts|asr ──────────────────
// Модели каталога OpenRouter, подходящие стадии (llm = текст -> текст; vision = картинка -> текст;
// tts = синтез речи; asr = транскрипция): id, имя, цены (строки USD за токен, как у OpenRouter), контекст,
// голоса. Каталог — из кэша (память -> диск -> сеть), ключ не нужен.
pub async fn openrouter_models(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let kind = q.get("kind").map(String::as_str).unwrap_or("llm");
    let Some(capability) = dub_llm::openrouter::Capability::parse(kind) else {
        return (StatusCode::BAD_REQUEST, format!("unknown model kind: {kind:?}")).into_response();
    };
    let models_root = st.models_root.clone();
    match tokio::task::spawn_blocking(move || crate::openrouter::catalog(&models_root)).await.unwrap_or_else(|e| Err(e.to_string())) {
        Ok(cached) => {
            let models: Vec<Value> = cached.catalog().models_for(capability).map(model_view).collect();
            Json(json!({ "models": models, "refreshed_at": cached.refreshed_at })).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, t!("openrouter-catalog-failed", error = e)).into_response(),
    }
}

/// Модель каталога для окна: без служебных полей чат-клиента.
fn model_view(model: &dub_llm::openrouter::CatalogModel) -> Value {
    json!({
        "id": model.id,
        "name": model.name,
        "context_length": model.context_length,
        "pricing": model.pricing,
        "input_modalities": model.input_modalities,
        "voices": model.voices,
    })
}

// ─── GET /engine/openrouter/catalog, POST /engine/openrouter/catalog/refresh ─
// Сводка каталога: сколько моделей по стадиям и когда он обновлялся; refresh — скачать заново.
fn catalog_summary(cached: &crate::openrouter::CachedCatalog) -> Value {
    use dub_llm::openrouter::Capability;
    let catalog = cached.catalog();
    let count = |capability| catalog.models_for(capability).count();
    json!({
        "refreshed_at": cached.refreshed_at,
        "total": cached.models.len(),
        "counts": {
            "llm": count(Capability::Text),
            "vision": count(Capability::Vision),
            "tts": count(Capability::Speech),
            "asr": count(Capability::Transcription),
        },
    })
}

pub async fn openrouter_catalog(State(st): State<AppState>) -> Response {
    let models_root = st.models_root.clone();
    match tokio::task::spawn_blocking(move || crate::openrouter::catalog(&models_root)).await.unwrap_or_else(|e| Err(e.to_string())) {
        Ok(cached) => Json(catalog_summary(&cached)).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, t!("openrouter-catalog-failed", error = e)).into_response(),
    }
}

pub async fn openrouter_catalog_refresh(State(st): State<AppState>) -> Response {
    let models_root = st.models_root.clone();
    match tokio::task::spawn_blocking(move || crate::openrouter::refresh(&models_root)).await.unwrap_or_else(|e| Err(e.to_string())) {
        Ok(cached) => Json(catalog_summary(&cached)).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, t!("openrouter-catalog-failed", error = e)).into_response(),
    }
}

// ─── POST /engine/openrouter/verify {key} ───────────────────────────────────
// Проверка ключа OpenRouter (GET /key) без сохранения. Ключ из формы, НЕ логируется. Ответ:
// {ok:true, data:{label, limit, usage, …}} | {ok:false, error}.
pub async fn openrouter_verify(Json(body): Json<Value>) -> Response {
    let key = body.get("key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if key.is_empty() {
        return (StatusCode::BAD_REQUEST, t!("openrouter-empty-key")).into_response();
    }
    let res = tokio::task::spawn_blocking(move || {
        dub_llm::openrouter::OpenRouter::new(Some(key)).and_then(|client| client.check_key()).map_err(|e| format!("{e:#}"))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(dub_llm::openrouter::KeyCheck::Accepted(data)) => Json(json!({ "ok": true, "data": data })).into_response(),
        Ok(dub_llm::openrouter::KeyCheck::Rejected(detail)) => Json(json!({ "ok": false, "error": detail })).into_response(),
        Err(e) => Json(json!({ "ok": false, "error": e })).into_response(),
    }
}

// ─── GET /engine/server/models?url= — модели локального OpenAI-совместимого сервера ──────────────────────
// Сервер пользователя (Ollama, LM Studio, vLLM, llama-server) отвечает на GET <адрес>/v1/models списком
// {data:[{id}]}. Спрашиваем через сервер студии, чтобы окно не ходило на чужой адрес само. Без url — адрес из
// настроек. Ключ из хранилища уходит только на адрес, для которого его сохранили.
pub async fn server_models(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let url = q
        .get("url")
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| crate::models::server_url(&st.models_root));
    let res = tokio::task::spawn_blocking(move || list_server_models(&url, crate::credentials::local_server_key_for(&url)))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(models) => Json(json!({ "models": models })).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": "server_unreachable", "detail": e }))).into_response(),
    }
}

/// id моделей сервера по адресу `url` (с `/v1` или без).
pub(crate) fn list_server_models(url: &str, key: Option<String>) -> Result<Vec<String>, String> {
    let base = dub_llm::server_base(url);
    if base.is_empty() {
        return Err(t!("llm-server-no-address"));
    }
    let endpoint = format!("{base}/v1/models");
    let client = dub_llm::net::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("http: {e}"))?;
    let mut request = client.get(&endpoint);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request.send().map_err(|e| t!("llm-server-no-answer", base = base.clone(), reason = dub_llm::net::why(&e)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(t!("llm-server-status", endpoint = endpoint.clone(), status = status.to_string()));
    }
    let body: Value = response.json().map_err(|e| t!("llm-server-not-json", endpoint = endpoint.clone(), error = e.to_string()))?;
    let models = body
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| t!("llm-server-not-model-list", endpoint = endpoint.clone()))?;
    Ok(models.iter().filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_string)).collect())
}

// ─── POST /engine/proxy/test {mode?, kind?, url, password?} — проверить связность ──────────────────────────
// Отвечают ли HF (закачка моделей) и OpenRouter (облако) при таком прокси — до сохранения. mode по умолчанию
// custom (прежняя форма {url}); прямой доступ — mode=off, прокси Windows — mode=system. Ответ с любым
// HTTP-статусом считается: меряем транспорт. 15 с на каждый, оба параллельно, причины ошибок цепочкой.
pub async fn proxy_test(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let mode = match body.get("mode").and_then(Value::as_str) {
        None => dub_llm::net::ProxyMode::Custom,
        Some(mode) => match dub_llm::net::ProxyMode::parse(mode) {
            Some(mode) => mode,
            None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_proxy_mode", "detail": mode }))).into_response(),
        },
    };
    let kind = match body.get("kind").and_then(Value::as_str) {
        None => crate::models::proxy_kind(&crate::models::load_selection(&st.models_root)),
        Some(kind) => match dub_llm::net::ProxyKind::parse(kind) {
            Some(kind) => kind,
            None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_proxy_kind", "detail": kind }))).into_response(),
        },
    };
    let address = crate::secrets_api::proxy_test_address(
        &st.models_root,
        crate::credentials::secrets_dir().as_deref(),
        body.get("url").and_then(Value::as_str).unwrap_or(""),
        body.get("password").and_then(Value::as_str),
    );
    let settings = dub_llm::net::ProxySettings { mode, kind, address: Some(address).filter(|a| !a.trim().is_empty()) };
    let res = tokio::task::spawn_blocking(move || dub_llm::net::test(settings).map_err(|e| format!("{e:#}")))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(v) => {
            let hf = v["huggingface"].as_bool() == Some(true);
            let or = v["openrouter"].as_bool() == Some(true);
            Json(json!({
                "ok": hf && or,
                "hf": hf,
                "openrouter": or,
                "hf_error": v["huggingface_error"],
                "openrouter_error": v["openrouter_error"],
            }))
            .into_response()
        }
        Err(e) => Json(json!({ "ok": false, "error": e })).into_response(),
    }
}

// ─── Пресеты железа: GET /engine/presets (список + детект GPU + рекомендация), POST /engine/preset {id} ──
pub async fn presets_list(State(_st): State<AppState>) -> Response {
    let rec = tokio::task::spawn_blocking(crate::presets::recommend)
        .await
        .unwrap_or_else(|_| crate::presets::recommend());
    let list: Vec<Value> = crate::presets::PRESETS
        .iter()
        .map(|p| json!({ "id": p.id, "title": (p.title)(), "subtitle": (p.subtitle)() }))
        .collect();
    Json(json!({ "presets": list, "hardware": rec })).into_response()
}

pub async fn preset_apply(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    match crate::presets::apply(&st.models_root, &id) {
        Ok(applied) => Json(json!({
            "ok": true,
            "id": id,
            "applied": applied.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

// GET /engine/openrouter/voices?model=<id> — голоса TTS-модели с полом/возрастом/русским из встроенного
// справочника (voice_db, собран роем). Для дропдауна выбора голоса + автокастинга. Нет в справочнике ->
// пустой список (юзер введёт голос вручную).
pub async fn openrouter_voices(Query(q): Query<HashMap<String, String>>) -> Response {
    let model = q.get("model").cloned().unwrap_or_default();
    let voices = crate::cloud_voices::list(&model).cloned().unwrap_or_default();
    let ru = crate::cloud_voices::model_supports_russian(&model);
    Json(json!({ "voices": voices, "supportsRussian": ru })).into_response()
}

// ─── PATCH /engine/opts ─────────────────────────────────────────────────────
// Свап слота модели (asr/tts/llm/vision) в рантайме. У порта OPTS иммутабелен внутри AppState
// (Arc<EngineOpts>), а модели резолвятся путями из окружения/дефолтов — рантайм-свап без пересоздания
// стейта не предусмотрен архитектурой (в отличие от питоновского живого OPTS). Валидируем вход как
// питон (непустые строки -> 400 иначе) и возвращаем ТЕКУЩИЙ стек — контракт формы {models:{...}}
// соблюдён; фактический свап слота — вне scope порта (модель-стек задаётся при старте сервера).
pub async fn set_opts(State(st): State<AppState>, Json(edit): Json<Value>) -> Response {
    for key in ["asr", "tts", "llm", "vision"] {
        if let Some(v) = edit.get(key) {
            let ok = v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false);
            if !ok {
                return (StatusCode::BAD_REQUEST, format!("{key:?} must be a non-empty path"))
                    .into_response();
            }
        }
    }
    let o = &st.opts;
    Json(json!({ "models": {
        "asr": o.asr_model,
        "llm": o.mt_model_path.to_string_lossy(),
        "vision": o.mmproj_path.to_string_lossy(),
        "tts": o.tts_model,
    }}))
    .into_response()
}

// ─── POST /engine/select ────────────────────────────────────────────────────
// Выбрать активный вариант модели (квант) для движка. Тело: {"id":"higgs-q6_k"} — id компонента из
// манифеста настроек. Пишем models/active.json; резолв при следующей генерации подхватит выбор без
// рестарта. Это ФАКТИЧЕСКИЙ свап-слот (в отличие от set_opts, который остался эхо-совместимостью).
pub async fn select_model(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    // Форма 1: {"key":"asr_engine","value":"whisper"} — прямая установка слота (движок/модель/квант ASR
    // и т.п.) из настроек, без скачивания. Ключ валидируем по белому списку.
    if let (Some(key), Some(val)) =
        (body.get("key").and_then(Value::as_str), body.get("value").and_then(Value::as_str))
    {
        if !crate::models::is_selection_key(key) {
            return (StatusCode::BAD_REQUEST, format!("unknown selection key: {key:?}")).into_response();
        }
        if val.trim().is_empty() {
            return (StatusCode::BAD_REQUEST, "empty value").into_response();
        }
        if !crate::models::is_selection_value(key, val) {
            return (StatusCode::BAD_REQUEST, format!("invalid value for {key}: {val:?}")).into_response();
        }
        if let Err(e) = crate::models::set_selection(&st.models_root, key, val) {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("write selection: {e}")).into_response();
        }
        return Json(crate::models::public_selection(&st.models_root)).into_response();
    }
    // Форма 2: {"id":"whisper-small"} — id компонента манифеста -> набор слотов (активация при скачивании
    // и переключение варианта). Пустой набор -> неизвестный компонент.
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let pairs = crate::models::component_selection(id);
    if pairs.is_empty() {
        return (StatusCode::BAD_REQUEST, format!("unknown model component: {id:?}")).into_response();
    }
    for (engine, variant) in pairs {
        if let Err(e) = crate::models::set_selection(&st.models_root, engine, &variant) {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("write selection: {e}")).into_response();
        }
    }
    Json(crate::models::public_selection(&st.models_root)).into_response()
}

// ─── PUT /projects/{pid} ────────────────────────────────────────────────────
// Полная замена Project (undo/redo снапшот). Порт app.py.put_project: валидировать тело как Project,
// сохранить атомарно, вернуть Project.
pub async fn put_project(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let mut proj: Project = match serde_json::from_value(body) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad project: {e}")).into_response(),
    };
    crate::fitplan::strip_computed(&mut proj);
    {
        let _held = crate::project_writes();
        // Глоссарий меняет только его ручка: откат правок окна (undo) присылает снимок со старым глоссарием.
        match st.load_project(&pid) {
            Ok(stored) => proj.glossary = stored.glossary,
            Err(resp) => return resp,
        }
        if let Err(e) = crate::write_project(&dir, &proj) {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
    }
    crate::project_response(&st, &dir, &proj)
}

// ─── GET /projects/{pid}/waveform?n=600 ─────────────────────────────────────
// Даунсэмпл-пики аудио (ffmpeg s16le 8kHz), кэш waveform.json. CPU, вне GPU-воркера. Порт app.py.
pub async fn waveform(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let cache = dir.join("waveform.json");
    if let Ok(txt) = std::fs::read_to_string(&cache) {
        return ([("content-type", "application/json")], txt).into_response();
    }
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let n: usize = q.get("n").and_then(|v| v.parse().ok()).unwrap_or(600);
    let video = PathBuf::from(&proj.meta.video);
    let peaks =
        tokio::task::spawn_blocking(move || crate::wavio::waveform_peaks(&video, n))
            .await
            .unwrap_or_default();
    let empty = peaks.is_empty();
    let out = json!({ "peaks": peaks });
    if !empty {
        let _ = std::fs::write(&cache, out.to_string());
    }
    Json(out).into_response()
}

// ─── POST /projects/{pid}/remix?instruction= ────────────────────────────────
// Творческий ремикс: Gemma переписывает ВЕСЬ транскрипт по теме/инструкции, помечает всё dirty.
// Порт app.py.remix_project (использует translate.rewrite = flat_rewrite dub-translate).
pub async fn remix_project(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let args = json!({ "instruction": q.get("instruction").cloned().unwrap_or_default() });
    match remix_enqueue(&st, &pid, &args).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(resp) => *resp,
    }
}

pub(crate) async fn remix_enqueue(st: &AppState, pid: &str, args: &Value) -> Result<String, Box<Response>> {
    let instruction = args.get("instruction").and_then(Value::as_str).unwrap_or("").to_string();
    if instruction.trim().is_empty() {
        return Err(Box::new((StatusCode::BAD_REQUEST, "empty remix instruction").into_response()));
    }
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    let proj_path = dir.join("project.json");
    if !proj_path.is_file() {
        return Err(Box::new((StatusCode::CONFLICT, "project not analyzed yet").into_response()));
    }
    let llama_bin = st.llama_bin.clone();
    // Квант Gemma — выбранный сейчас (models::resolve_mt), а не найденный при старте сервера.
    let (mt_model, _) = crate::models::resolve_mt(&st.models_root, &crate::models::load_selection(&st.models_root));
    let models_root = st.models_root.clone();
    let instr = instruction.trim().to_string();
    let dir_for_job = dir.clone();

    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        use dub_translate::{flat_rewrite, Seg};

        let read = |path: &std::path::Path| -> Result<Project, String> {
            let text = std::fs::read_to_string(path).map_err(|e| t!("common-read", path = path.display().to_string(), error = e.to_string()))?;
            Project::from_json(&text).map_err(|e| t!("common-parse", path = path.display().to_string(), error = e.to_string()))
        };
        let p = read(&proj_path)?;
        if p.segments.is_empty() {
            return Err("no transcript to remix — analyze first".into());
        }
        progress(json!({ "type": "progress",
            "msg": t!("remix-start", count = p.segments.len(),
                           instruction = instr[..instr.len().min(60)].to_string()) }));

        // LLM-провайдер перевода: своя Gemma, локальный сервер или OpenRouter (плоский rewrite).
        // Text-режим: mmproj не нужен (передаём пустой путь — фабрика его в Text-режиме игнорирует).
        let prov = crate::llm_provider::open(
            &crate::llm_provider::LlmOpen {
                llama_bin: &llama_bin,
                mt_model: &mt_model,
                mmproj: std::path::Path::new(""),
                models_root: &models_root,
            },
            crate::llm_provider::LlmMode::Text,
        )
        .map_err(|e| t!("remix-no-llm", error = e.to_string()))?;
        let client = prov.client();

        let inputs: Vec<(String, String)> = p.segments.iter().map(|s| (s.id.clone(), s.src_text.clone())).collect();
        let mut segs: Vec<Seg> = p
            .segments
            .iter()
            .map(|s| {
                let spk = crate::analyze::speaker_to_i64(s.speaker.as_deref());
                Seg::new(s.src_text.clone(), spk)
            })
            .collect();
        let r = flat_rewrite(client, &mut segs, &instr, "auto", &p.tgt_lang, false, &p.audio.translate_style);
        drop(prov);
        r.map_err(|e| t!("remix-failed", error = crate::localize::Localize::localize(&e)))?;

        // Ремикс шёл минутами: правки, сделанные за это время, остаются, а новый текст ложится, только пока
        // реплики те же, что переписывались.
        let _held = crate::project_writes();
        let mut fresh = read(&proj_path)?;
        if fresh.segments.iter().map(|s| (s.id.clone(), s.src_text.clone())).collect::<Vec<_>>() != inputs {
            return Err(t!("remix-lines-changed"));
        }
        for (s, sg) in fresh.segments.iter_mut().zip(segs) {
            if !sg.tgt.trim().is_empty() {
                s.tgt_text = sg.tgt;
            }
            s.dirty = true;
        }
        fresh.audio.rewrite = Some(instr.clone());
        crate::write_project(&dir_for_job, &fresh)?;
        serde_json::to_value(&fresh).map_err(|e| e.to_string())
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::Remix, pid, dir, args.clone()), job)
        .await
        .map_err(|e| Box::new(crate::enqueue_error(e)))
}

// ─── GET /projects/{pid}/preview?t=&rev= ────────────────────────────────────
// ОДИН превью-кадр (PNG) через GPU-воркер (сериализовано). Порт app.py.preview (таймаут 300с).
pub async fn preview(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(resp) => return resp, // 409 если не проанализирован
    };
    let input = match std::fs::read_to_string(dir.join("source.txt")) {
        Ok(s) => PathBuf::from(s.trim()),
        Err(_) => return (StatusCode::CONFLICT, "no source uploaded").into_response(),
    };
    let t: f64 = q.get("t").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    // lr=1 (плей) -> низкое разрешение кадра на больших видео (решает preview_frame). JPEG-кадр.
    let lowres = q.get("lr").map(|v| v == "1" || v == "true").unwrap_or(false);
    let fonts_dir = st.fonts_dir.clone();
    let work_dir = dir.clone();
    frame_job(&st, &pid, 300, "image/jpeg", move |_p| {
        crate::frame::preview_frame(&proj, &input, &work_dir, &fonts_dir, t, lowres)
    })
    .await
}

// ─── GET /projects/{pid}/original?t= ────────────────────────────────────────
// Сырой кадр ОРИГИНАЛА (PNG) на t — для before/after. Порт app.py.original (source_frame, таймаут 60с).
// ВНИМАНИЕ: возвращает ОДИН PNG-кадр (как источник истины app.py), а не всё видео — фронт (ComparePane)
// использует это как <img src>.
pub async fn original_frame(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let input = match std::fs::read_to_string(dir.join("source.txt")) {
        Ok(s) => PathBuf::from(s.trim()),
        Err(_) => return (StatusCode::NOT_FOUND, "no source").into_response(),
    };
    if !input.is_file() {
        return (StatusCode::NOT_FOUND, "source missing").into_response();
    }
    let t: f64 = q.get("t").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let work_dir = dir.clone();
    frame_job(&st, &pid, 60, "image/png", move |_p| crate::frame::source_frame(&input, &work_dir, t)).await
}

/// Общий помощник: поставить синхронную кадр-джобу в GPU-воркер, ждать с таймаутом, вернуть
/// байты с заданным content-type (`mime`) или 504. Порт паттерна app.py.preview/original.
async fn frame_job<F>(st: &AppState, pid: &str, timeout_s: u64, mime: &'static str, f: F) -> Response
where
    F: FnOnce(jobs::ProgressFn) -> Result<Vec<u8>, String> + Send + 'static,
{
    let job: jobs::JobFn = Box::new(move |progress| {
        let png = f(progress)?;
        // Возвращаем байты как base64 в JSON-результат нельзя эффективно; вместо этого прокидываем через
        // канал: сериализуем как массив байт в Value (voркер отдаёт oneshot Result<Value>). Читаем ниже.
        Ok(Value::Array(png.into_iter().map(|b| Value::from(b as u64)).collect()))
    });
    let meta = jobs::JobMeta::new(jobs::JobKind::Frame, Some(pid));
    let (job_id, rx) = match st.jobs.enqueue_awaitable(meta, job).await {
        Ok(v) => v,
        Err(e) => return crate::enqueue_error(e),
    };
    match tokio::time::timeout(std::time::Duration::from_secs(timeout_s), rx).await {
        Ok(Ok(Ok(v))) => {
            st.jobs.remove(&job_id).await;
            let bytes: Vec<u8> = v
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u8)).collect())
                .unwrap_or_default();
            ([("content-type", mime)], bytes).into_response()
        }
        Ok(Ok(Err(e))) => {
            st.jobs.remove(&job_id).await;
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
        Ok(Err(_)) => {
            st.jobs.remove(&job_id).await;
            (StatusCode::INTERNAL_SERVER_ERROR, "job canceled").into_response()
        }
        Err(_) => {
            // Ожидающий ушёл: джоба в очереди снимается, выполняемая прерывается (её ffmpeg убивается).
            let _ = st.jobs.cancel(&job_id).await;
            (StatusCode::GATEWAY_TIMEOUT, "frame render timed out").into_response()
        }
    }
}

#[cfg(test)]
mod provider_endpoint_tests {
    use super::*;
    use dub_llm::test_http::{serve, Reply};

    #[test]
    fn the_saved_server_key_goes_only_to_its_own_address() {
        let saved = serve(vec![Reply::json(200, r#"{"data":[{"id":"m"}]}"#)]);
        let foreign = serve(vec![Reply::json(200, r#"{"data":[{"id":"m"}]}"#)]);
        let dir = std::env::temp_dir().join(format!("dub-server-key-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        crate::credentials::store_local_server_key_in(&dir, &format!("{}/v1", saved.base()), Some("lm-secret")).unwrap();
        let key_for = |url: &str| crate::credentials::local_server_key_in(&dir, url);

        list_server_models(&foreign.base(), key_for(&foreign.base())).unwrap();
        let sent = foreign.request(0).to_ascii_lowercase();
        assert!(!sent.contains("authorization:") && !sent.contains("lm-secret"), "{sent}");

        list_server_models(&saved.base(), key_for(&saved.base())).unwrap();
        assert!(saved.request(0).to_ascii_lowercase().contains("authorization: bearer lm-secret"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_local_server_lists_its_models_in_any_address_form() {
        let server = serve(vec![
            Reply::json(200, r#"{"object":"list","data":[{"id":"gemma3:12b"},{"id":"qwen2.5vl:7b"}]}"#),
            Reply::json(200, r#"{"data":[]}"#),
            Reply::json(200, r#"{"models":[]}"#),
            Reply::json(401, r#"{"error":"no key"}"#),
        ]);
        assert_eq!(list_server_models(&format!("{}/v1/", server.base()), Some("k".into())).unwrap(), ["gemma3:12b", "qwen2.5vl:7b"]);
        let first = server.request(0);
        assert!(first.starts_with("GET /v1/models "), "{first}");
        assert!(first.to_ascii_lowercase().contains("authorization: bearer k"));
        assert!(list_server_models(&server.base(), None).unwrap().is_empty());
        assert!(!server.request(1).to_ascii_lowercase().contains("authorization:"));
        assert!(list_server_models(&server.base(), None).unwrap_err().contains("data"));
        assert!(list_server_models(&server.base(), None).unwrap_err().contains("401"));
        assert!(list_server_models("  ", None).is_err());
    }
}
