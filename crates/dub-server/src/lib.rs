//! dub-server — axum-реализация REST/SSE контракта Dub Studio. Цель: SPA (frontend/dist) работает
//! без правок против этого сервера так же, как против backend/app.py.
//!
//! Раунд 1 реализует: раздачу SPA с защитой от traversal, GET /engine/capabilities, POST /projects
//! (multipart-загрузка видео -> workspace/<pid>/), GET /projects/{id}, каркас очереди джоб + SSE
//! GET /jobs/{id}/events. GPU-эндпоинты (analyze/render/preview/patch и т.д.) — каркас на следующие
//! раунды; их карта в docs/PORT-CONTRACT.md.

mod analyze;
mod asr_filter;
mod atomic;
mod bench;
mod casting;
mod casting_library;
mod cloud_asr;
mod cloud_tts;
mod cloud_voices;
mod compose;
mod credentials;
#[cfg(test)]
mod dll_imports;
mod downloads;
mod dub_timing;
mod endpoints;
mod fitplan;
mod guard;
mod llm_provider;
mod openrouter;
mod f0;
mod frame;
mod glossary_api;
mod hw;
mod presets;
mod job_store;
mod jobs;
mod mcp;
mod limiter;
mod media;
mod models;
mod ocr;
mod patch;
mod post_analyze;
mod project_files;
mod record;
mod render;
mod secrets_api;
mod setup;
mod shorten;
mod spa;
mod studio_settings;
mod subalign;
mod subimport;
mod subs_text;
mod subtracks;
mod takes;
mod translate;
mod tts_text;
mod tts_trim;
mod url_import;
mod voice_slots;
mod wavio;
mod ytdlp;
pub mod process_group;
pub mod service;

use axum::extract::{Multipart, Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use dub_core::{EngineOpts, Project};
use futures_util::stream::Stream;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use jobs::JobQueue;
/// Маршрут прокси приложения — для клиентов вне сервера (апдейтер десктопа).
pub use dub_llm::net;

/// E2E-верификация caption-композита БЕЗ ASR/Gemma/TTS: берём УЖЕ проанализированный project.json
/// (кэш transcript+raw_ctx), заново гоним OCR-стадию + compose (реальная детекция + матчинг титров),
/// затем вжигаем блюр+сабы в captioned.mp4. Проверка на живых данных, не
/// перезапуская тяжёлый Gemma-vision. Возвращает путь к captioned.mp4.
pub fn verify_captions_e2e(
    repo_root: &Path,
    project_json: &Path,
    input_video: &Path,
    work_dir: &Path,
) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(project_json).map_err(|e| e.to_string())?;
    let mut proj = Project::from_json(&text).map_err(|e| e.to_string())?;

    // OCR-стадия перезаписывает captions.blur_boxes/titles/sub_style/sub_px через compose. Чистим
    // предыдущий вывод, чтобы не смешивать со старым blur из project.json.
    proj.captions.blur_boxes.clear();
    proj.captions.titles.clear();

    let vw = proj.meta.width;
    let vh = proj.meta.height;
    let total = proj.meta.duration;
    let src_codec = proj.meta.src_codec.clone();

    // Пути резолвим напрямую из repo_root (без AppState — тот тянет Tokio-JobQueue). ocr::stage нужны
    // лишь models_root/caption_fps/input/work_dir; llama_bin/mt_model — только для таглайн-MT (fail-safe).
    let opts = EngineOpts::default();
    let mroot = models_root(repo_root);
    let fonts_dir = repo_root.join("fonts");
    let unused = repo_root.join("nonexistent");
    let args = analyze::AnalyzeArgs {
        speaker_count: 0,
        tgt_lang: proj.tgt_lang.clone(),
        mode: proj.mode.clone(),
        src_lang: "auto".into(),
        subs: proj.subs.mode.clone(),
        rewrite: String::new(),
        translate_style: proj.audio.translate_style.clone(),
        burn: proj.subs.burn,
        detect_text: true,
        casting: false, // verify-путь: только капшены/OCR, кастинг не нужен
        casting_ref: String::new(),
        content_type: String::new(),
        import_translated: false,
        align_subs: false,
    };
    let sel = models::load_selection(&mroot);
    let (mt_model, mmproj) = models::resolve_mt(&mroot, &sel);
    let paths = analyze::AnalyzePaths {
        input: input_video.to_path_buf(),
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        asr: models::AsrChoice::Parakeet(unused.clone()), // ocr::stage не трогает ASR (verify-путь)
        sortformer_onnx: unused.clone(),
        llama_bin: dub_llm::resolve_llama_bin(&repo_root.join("tools").join("llama")),
        mt_model,
        mmproj,
        models_root: mroot,
        caption_fps: opts.caption_fps,
        import_subs: None,
        // ocr::stage сепарацию не использует — заглушки (verify-путь без BSRoformer).
        bsroformer_cli: unused.clone(),
        bsroformer_model: unused.clone(),
    };
    let cb = |ev: Value| {
        if let Some(m) = ev.get("msg").and_then(|v| v.as_str()) {
            eprintln!("[verify] {m}");
        }
    };
    ocr::stage(&args, &paths, &mut proj, vw, vh, total, &cb);

    let out_ass = work_dir.join("verify_caps.ass");
    let captioned = work_dir.join("verify_captioned.mp4");
    render::build_and_burn_captions(
        &proj, input_video, &out_ass, &captioned, &fonts_dir, vw, vh, total, &src_codec,
    )?;
    // Записать обновлённый project (с bbox титров) рядом — для инспекции.
    let dbg = work_dir.join("verify_project.json");
    let _ = std::fs::write(&dbg, proj.to_json_pretty().unwrap_or_default());
    Ok(captioned)
}

#[derive(Clone)]
pub struct AppState {
    pub repo_root: PathBuf,
    pub workspace: PathBuf,
    pub web_root: Option<PathBuf>,
    pub opts: Arc<EngineOpts>,
    pub jobs: JobQueue,
    /// Каталог TDT-модели ASR (analyze). Env DUB_STUDIO_TDT, иначе <models_root>/tdt.
    pub tdt_dir: PathBuf,
    /// Путь к модели диаризации (Nemotron 3 Diarization .onnx). Env DUB_STUDIO_SORTFORMER, иначе
    /// <models_root>/nemotron-diar/nemotron3_diar_v3.onnx.
    pub sortformer_onnx: PathBuf,
    /// llama-server(.exe) — сайдкар перевода/vision. Env DUB_STUDIO_LLAMA_BIN, иначе <repo>/tools/llama/llama-server(.exe).
    pub llama_bin: PathBuf,
    /// BSRoformer CLI (сепарация). Env DUB_STUDIO_BSROFORMER_DIR/<cli>, иначе <repo>/tools/bsroformer/bs_roformer-cli.exe.
    pub bsroformer_cli: PathBuf,
    /// GGUF модель сепарации. Env DUB_STUDIO_BSROFORMER_MODEL, иначе <repo>/models/bsroformer/voc_fv6-Q8_0.gguf.
    pub bsroformer_model: PathBuf,
    /// Higgs audiocpp_engine.dll (TTS). Env DUB_STUDIO_HIGGS_DLL, иначе <models>/higgs-engine/audiocpp_engine.dll.
    pub higgs_dll: PathBuf,
    /// Каталог весов Higgs (q8_0). Env DUB_STUDIO_HIGGS_MODEL, иначе <models>/higgs-q8_0.
    pub higgs_model_root: PathBuf,
    /// Каталог bundled-шрифтов субтитров. Env DUB_STUDIO_FONTS_DIR, иначе <repo>/fonts.
    pub fonts_dir: PathBuf,
    /// Корень моделей (для OCR: <root>/ocr/…). Env DUBENGINE_MODELS_ROOT, иначе <repo>/models.
    pub models_root: PathBuf,
    /// Каталог голосов-паков + записей с микрофона. Env DUBENGINE_VOICES, иначе <repo>/voices.
    pub voices_dir: PathBuf,
    /// Фоновая закачка компонентов (мимо GPU-очереди джоб; состояние — в /setup/status).
    pub downloads: downloads::Downloads,
    /// Загрузки видео по ссылке (yt-dlp), тоже мимо GPU-очереди; состояние — в /url/fetches.
    pub fetches: url_import::Fetches,
}

/// Корень моделей: env DUBENGINE_MODELS_ROOT, иначе <repo_root>/models.
pub(crate) fn models_root(repo_root: &Path) -> PathBuf {
    std::env::var("DUBENGINE_MODELS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("models"))
}

/// Прописать в PATH процесса каталоги скачанных бинарей (ffmpeg, llama, движок Higgs с CUDA/VC-DLL),
/// чтобы `Command::new("ffmpeg")` и подобные находили их без перезапуска после автозакачки. Вызывается
/// один раз при старте сервера (main.rs / Tauri-shell). Идемпотентно: не дублирует уже присутствующие.
pub fn augment_path_for_tools(repo_root: &Path) {
    let dirs = [
        repo_root.join("tools").join("ffmpeg"),
        repo_root.join("tools").join("llama"),
        repo_root.join("models").join("higgs-engine"),
    ];
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cur = std::env::var("PATH").unwrap_or_default();
    let mut prefix = String::new();
    for d in dirs {
        let s = d.to_string_lossy().to_string();
        if !s.is_empty() && !cur.split(sep).any(|p| p == s) && !prefix.split(sep).any(|p| p == s) {
            if !prefix.is_empty() {
                prefix.push_str(sep);
            }
            prefix.push_str(&s);
        }
    }
    if !prefix.is_empty() {
        std::env::set_var("PATH", format!("{prefix}{sep}{cur}"));
    }
}

/// Прописать ORT_DYLIB_PATH (onnxruntime 1.28) в окружение процесса до старта сервера: ort грузит dylib лениво,
/// а по голому имени Windows нашла бы раньше PATH onnxruntime.dll другой версии из System32 (Windows AI).
/// GPU-сборка (cuda13) приоритетнее — суперсет CPU+CUDA; `beside_exe` — копия рядом с exe (портатив).
pub fn set_ort_dylib_env(repo_root: &Path, beside_exe: &Path) {
    if std::env::var_os("ORT_DYLIB_PATH").is_some() {
        return;
    }
    let rt = repo_root.join("models").join("runtime");
    let found = [
        rt.join("onnxruntime-win-x64-gpu_cuda13-1.28.2").join("lib").join("onnxruntime.dll"),
        rt.join("onnxruntime-win-x64-1.28.2").join("lib").join("onnxruntime.dll"),
        beside_exe.join("onnxruntime.dll"),
        rt.join("onnxruntime-1.28.dll"),
        rt.join("onnxruntime.dll"),
    ]
    .into_iter()
    .find(|cand| cand.is_file());
    match found {
        Some(cand) => std::env::set_var("ORT_DYLIB_PATH", cand),
        None => eprintln!("[ERROR] onnxruntime 1.28 не найден в {}: распознавание и диаризация не запустятся, пока не скачан компонент ONNX Runtime", rt.display()),
    }
}

/// Маршрут прокси из models/active.json для всех HTTP-клиентов приложения (dub_llm::net): вызывать на старте.
/// Сохранение формы прокси перестраивает его сразу, без рестарта.
pub fn init_proxy_route(repo_root: &Path) {
    models::apply_proxy_route(&repo_root.join("models"));
}

/// Поднять axum-сервер БЛОКИРУЮЩЕ на собственном tokio-рантайме. Для встраивания в десктоп-оболочку ОДНИМ
/// процессом (вместо запуска dub-server.exe отдельным subprocess) — вызывать из фонового std::thread.
/// Слушатель уже занят вызывающим (service::claim_port); augment PATH под инструменты делается здесь же.
pub fn serve_blocking(repo_root: impl AsRef<Path>, listener: std::net::TcpListener) -> anyhow::Result<()> {
    let root = repo_root.as_ref().to_path_buf();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        augment_path_for_tools(&root);
        init_proxy_route(&root);
        serve(AppState::new(&root), listener).await
    })
}

/// Обслуживать API и SPA на занятом слушателе до остановки рантайма.
pub async fn serve(state: AppState, listener: std::net::TcpListener) -> anyhow::Result<()> {
    listener.set_nonblocking(true)?;
    service::record_port(listener.local_addr()?.port());
    let listener = tokio::net::TcpListener::from_std(listener)?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

impl AppState {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        let repo_root = repo_root.as_ref().to_path_buf();
        let workspace = repo_root.join("workspace");
        let _ = std::fs::create_dir_all(&workspace);
        let interrupted = job_store::recover(&workspace);
        if interrupted > 0 {
            eprintln!("[jobs] прерванных джоб проектов: {interrupted} (можно продолжить)");
        }
        let web_root = spa::find_web_root(&repo_root);
        let mroot = models_root(&repo_root);
        match credentials::migrate_legacy_selection(&mroot) {
            Ok(moved) if moved.openrouter_key || moved.proxy_password => {
                tracing::info!("секреты перенесены из active.json в хранилище секретов: {moved:?}")
            }
            Ok(_) => {}
            Err(e) => tracing::error!("секреты из active.json не перенесены в хранилище: {e:#}"),
        }
        let tdt_dir = std::env::var("DUB_STUDIO_TDT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| mroot.join("tdt"));
        let sortformer_onnx = std::env::var("DUB_STUDIO_SORTFORMER")
            .map(PathBuf::from)
            .unwrap_or_else(|_| mroot.join(dub_asr::DIAR_MODEL_DIR).join(dub_asr::DIAR_MODEL_FILE));
        // llama-server: env-override, иначе <repo>/tools/llama/llama-server(.exe) (негитуемый каталог,
        // кладёт установщик/раунд 3). Существование проверяет сама стадия перевода (fail-safe).
        let llama_bin = dub_llm::resolve_llama_bin(&repo_root.join("tools").join("llama"));
        let bsroformer_cli = std::env::var("DUB_STUDIO_BSROFORMER_DIR")
            .map(|d| PathBuf::from(d).join(dub_sep::ENGINE_CLI_FILE))
            .unwrap_or_else(|_| dub_sep::engine_dir(&repo_root).join(dub_sep::ENGINE_CLI_FILE));
        let bsroformer_model = dub_sep::model_path(&repo_root);
        let higgs_dll = std::env::var("DUB_STUDIO_HIGGS_DLL")
            .map(PathBuf::from)
            .unwrap_or_else(|_| mroot.join("higgs-engine").join("audiocpp_engine.dll"));
        let higgs_model_root = std::env::var("DUB_STUDIO_HIGGS_MODEL")
            .map(PathBuf::from)
            .unwrap_or_else(|_| mroot.join("higgs-q8_0"));
        let fonts_dir = std::env::var("DUB_STUDIO_FONTS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| repo_root.join("fonts"));
        let voices_dir = std::env::var("DUBENGINE_VOICES")
            .map(PathBuf::from)
            .unwrap_or_else(|_| repo_root.join("voices"));
        let downloads = downloads::Downloads::open(&repo_root);
        let fetches = url_import::Fetches::open(&repo_root, &workspace);
        AppState {
            repo_root,
            workspace,
            web_root,
            opts: Arc::new(EngineOpts::default()),
            jobs: JobQueue::new(),
            tdt_dir,
            sortformer_onnx,
            llama_bin,
            bsroformer_cli,
            bsroformer_model,
            higgs_dll,
            higgs_model_root,
            fonts_dir,
            models_root: mroot,
            voices_dir,
            downloads,
            fetches,
        }
    }

    fn proj_dir(&self, pid: &str) -> Result<PathBuf, Response> {
        // pid — hex uuid; отсекаем любые сепараторы/`..` до касания ФС (защита от traversal в pid).
        if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err((StatusCode::NOT_FOUND, "project not found").into_response());
        }
        let d = self.workspace.join(pid);
        if !d.exists() {
            return Err((StatusCode::NOT_FOUND, "project not found").into_response());
        }
        Ok(d)
    }

    fn load_project(&self, pid: &str) -> Result<Project, Response> {
        let d = self.proj_dir(pid)?;
        let f = d.join("project.json");
        if !f.is_file() {
            return Err((StatusCode::CONFLICT, "project not analyzed yet").into_response());
        }
        let text = std::fs::read_to_string(&f)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())?;
        Project::from_json(&text)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())
    }
}

/// Атомарная запись project.json (tmp+rename): частичного файла при падении не будет.
fn save_project_atomic(dir: &Path, proj: &Project) -> Result<(), String> {
    let _held = project_writes();
    write_project(dir, proj)
}

/// Держится от чтения project.json до записи обратно: правка одного писателя не теряется между
/// чтением и записью другого. Не реентерабелен: под ним пишут через write_project, не save_project_atomic.
pub(crate) fn project_writes() -> std::sync::MutexGuard<'static, ()> {
    static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Запись project.json под уже взятым project_writes.
pub(crate) fn write_project(dir: &Path, proj: &Project) -> Result<(), String> {
    let json = proj
        .to_json_pretty()
        .map_err(|e| format!("сериализация project.json: {e}"))?;
    mcp::save_with_revision(dir, || dub_core::atomic::write(&dir.join("project.json"), json.as_bytes()))
}

/// Ответ на неудачную постановку джобы: 409 с id уже идущей джобы того же класса или 500.
fn enqueue_error(e: jobs::EnqueueError) -> Response {
    match e {
        jobs::EnqueueError::Conflict { job_id, kind } => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "job_conflict", "job_id": job_id, "kind": kind.as_str() })),
        )
            .into_response(),
        jobs::EnqueueError::Store(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("job.json: {e}")).into_response()
        }
    }
}

/// Начало тела джобы проекта: убрать недописанные временные файлы прошлого (упавшего) процесса.
fn clean_partials(dir: &Path) {
    let mut n = dub_core::atomic::cleanup_stale(dir) + dub_core::atomic::cleanup_stale(&dir.join("stems"));
    // Джобы идут по одной, поэтому недописанная сепарация на старте джобы — остаток оборванного прогона.
    let stems_part = dub_sep::part_dir(&dir.join("stems"));
    if stems_part.is_dir() {
        match std::fs::remove_dir_all(&stems_part) {
            Ok(()) => n += 1,
            Err(e) => eprintln!("[jobs] {}: {e}", stems_part.display()),
        }
    }
    if n > 0 {
        eprintln!("[jobs] {}: удалено недописанных файлов: {n}", dir.display());
    }
}

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(service::health))
        .route("/engine/capabilities", get(capabilities))
        .route("/engine/opts", axum::routing::patch(endpoints::set_opts))
        .route("/engine/select", post(endpoints::select_model))
        .route("/engine/openrouter/models", get(endpoints::openrouter_models))
        .route("/engine/openrouter/voices", get(endpoints::openrouter_voices))
        .route("/engine/openrouter/verify", post(endpoints::openrouter_verify))
        .route("/engine/openrouter/catalog", get(endpoints::openrouter_catalog))
        .route("/engine/openrouter/catalog/refresh", post(endpoints::openrouter_catalog_refresh))
        .route("/engine/server/models", get(endpoints::server_models))
        .route(
            "/engine/server/key",
            get(secrets_api::server_key_settings).put(secrets_api::update_server_key).delete(secrets_api::delete_server_key),
        )
        .route(
            "/engine/openrouter/settings",
            get(secrets_api::openrouter_settings)
                .put(secrets_api::update_openrouter_settings)
                .delete(secrets_api::delete_openrouter_settings),
        )
        .route("/engine/proxy/test", post(endpoints::proxy_test))
        .route("/engine/proxy/settings", get(secrets_api::proxy_settings).put(secrets_api::update_proxy_settings))
        .route("/engine/presets", get(endpoints::presets_list))
        .route("/engine/preset", post(endpoints::preset_apply))
        // «Первый запуск»: статус компонентов + автозакачка недостающего (SSE через ту же job-машину).
        .route("/setup/status", get(setup_status))
        .route("/setup/download", post(setup_download))
        .route("/setup/cancel", post(setup_cancel))
        .route("/setup/remove", post(setup_remove))
        .route("/setup/open-models", post(setup_open_models))
        .route("/setup/browse", post(setup_browse))
        .route("/pick-folder", post(pick_folder))
        .route("/setup/import", post(setup_import))
        .route("/hw/snapshot", get(hw_snapshot))
        .route("/record/devices", get(record_devices))
        .route("/record/level", get(record_level))
        .route("/record/start", post(record_start))
        .route("/record/stop", post(record_stop))
        .route("/voices/download-pack", post(voices_download_pack))
        .route("/voices/catalog", get(voices_catalog))
        .route("/voices/sample", get(voice_sample))
        .route("/voices/get", post(voices_get))
        .route("/voices/rename", post(voices_rename))
        .route("/voices/delete", post(voices_delete))
        .route("/projects/{pid}/speaker-voice", post(speaker_voice))
        .route("/projects/{pid}/voice-slots", post(voice_slots_assign))
        .route("/projects/{pid}/casting", get(casting_get).post(casting_save))
        .route("/projects/{pid}/casting/avatar", get(casting_avatar))
        .route("/projects/{pid}/casting/voice", get(casting_voice))
        .route("/casting/library", get(casting_library_list))
        .route("/projects/{pid}/casting/library", post(casting_library_save))
        .route("/casting/library/{slug}", delete(casting_library_delete))
        .route("/casting/library/{slug}/avatar", get(casting_library_avatar))
        .route("/casting/library/{slug}/glossary", get(glossary_api::series_get).put(glossary_api::series_put))
        .route("/settings/launch", get(studio_settings::launch_get).patch(studio_settings::launch_patch))
        .route("/app/paths", get(studio_settings::app_paths))
        .route("/fonts", get(endpoints::fonts))
        .route("/voices", get(voices_list))
        .route("/presets", get(endpoints::presets))
        .route("/projects", post(create_project).get(list_projects))
        .route("/projects/from-path", post(atomic::from_path))
        // Видео по ссылке (yt-dlp): проба, загрузка в новый проект мимо GPU-очереди, сам инструмент и его обновление.
        .route("/projects/from_url", post(url_import::create_from_url))
        .route("/url/probe", get(url_import::probe_get).post(url_import::probe_post))
        .route("/url/fetches", get(url_import::fetches_list))
        .route("/url/fetches/{id}", get(url_import::fetch_get).delete(url_import::fetch_forget))
        .route("/url/fetches/{id}/cancel", post(url_import::fetch_cancel))
        .route("/url/fetches/{id}/resume", post(url_import::fetch_resume))
        .route("/url/tool", get(url_import::tool_status))
        .route("/url/tool/update", post(url_import::tool_update))
        .route(
            "/projects/{pid}",
            get(get_project).patch(patch_project).put(endpoints::put_project).delete(delete_project),
        )
        .route("/projects/{pid}/analyze", post(analyze_project))
        .route("/projects/{pid}/resume", post(resume_project))
        .route("/projects/{pid}/align", post(align_project))
        .route("/projects/{pid}/remix", post(endpoints::remix_project))
        .route("/projects/{pid}/render", post(render_project))
        .route("/projects/{pid}/export-lang", post(export_lang))   // клон+ре-перевод+рендер на другом языке (экспорт-уровень мультиязыка)
        .route("/projects/{pid}/retranslate", post(retranslate_project))   // #122: смена режима из транскрипта — перевод готовых сегментов БЕЗ ASR
        .route("/projects/{pid}/glossary", get(glossary_api::project_get).put(glossary_api::project_put))
        .route("/projects/{pid}/glossary/extract", post(glossary_api::extract))
        .route("/projects/{pid}/waveform", get(endpoints::waveform))
        .route("/projects/{pid}/preview", get(endpoints::preview))
        .route("/projects/{pid}/output", get(output))
        .route("/projects/{pid}/open", post(open_output))
        .route("/projects/{pid}/reveal", post(reveal_file))
        .route("/projects/{pid}/save-text", post(save_text))
        .route("/projects/{pid}/save-output", post(save_output))
        .route("/projects/{pid}/files", get(project_files::files))
        .route("/projects/{pid}/export-text", post(project_files::export_text))
        .route("/projects/{pid}/dub-audio", post(dub_audio_project))
        .route("/projects/{pid}/separate", post(atomic::separate))
        .route("/projects/{pid}/detect-text", post(atomic::detect_text))
        .route("/projects/{pid}/shorten", post(shorten::shorten_project))
        .route("/projects/{pid}/segments/{id}/takes", get(takes::list))
        .route("/projects/{pid}/segments/{id}/takes/{n}/audio", get(takes::audio))
        // /original?t= отдаёт ОДИН PNG-кадр оригинала (порт app.py.original -> source_frame),
        // фронт (ComparePane) вставляет его как <img src>. Range-раздача сырого видео — /dub.
        .route("/projects/{pid}/original", get(endpoints::original_frame))
        .route("/projects/{pid}/dub", get(dub_video))
        .route("/jobs", get(jobs_list))
        .route("/jobs/{job_id}", get(job_get))
        .route("/jobs/{job_id}/cancel", post(job_cancel))
        .route("/jobs/{job_id}/events", get(job_events))
        // SPA fallback — монтируется последним, чтобы не затенять API.
        .fallback(spa_fallback)
        // Видео-аплоад — большие тела. axum по дефолту режет на 2МБ (multipart ломается на
        // реальном ролике). Питон (Starlette) лимита не ставит -> снимаем и мы.
        .layer(axum::extract::DefaultBodyLimit::disable())
        // Автор запроса (окно, агент, API), ревизия проекта в ответе и оповещение окон о переменах.
        .layer(axum::middleware::from_fn(mcp::track))
        .with_state(state);
    // MCP-инструменты зовут те же маршруты внутри процесса. Гард Origin/Host вешается ниже этой точки,
    // снаружи /mcp и /mcp/status, а не внутри `api`.
    mcp::install(api.clone());
    api.route("/mcp", post(mcp::handle))
        .route("/mcp/status", get(mcp::status))
        .route("/mcp/window", get(mcp::window_events))
        .route("/mcp/window/result", post(mcp::window_result).layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)))
        .route("/mcp/window/focus", post(mcp::window_focus))
        .layer(guard::cors())
        // Снаружи всех слоёв: чужой Origin/Host получает 403 раньше CORS, SPA и любой ручки.
        .layer(axum::middleware::from_fn(guard::origin_guard))
}

// ─── /engine/capabilities ───────────────────────────────────────────────────

async fn capabilities(State(st): State<AppState>) -> Json<Value> {
    let o = &st.opts;
    let ffmpeg = which_ffmpeg();
    // Текущий выбор вариантов (active.json) — фронт рисует по нему селекторы движка/модели/кванта ASR.
    // Без секретов: вместо ключа OpenRouter и пароля прокси — флаги or_key_set / proxy_password_set.
    let sel = models::public_selection(&st.models_root);
    // Языки контента = полный набор Whisper (99). Единый источник — dub_translate::WHISPER_LANGS.
    let langs: Vec<&str> = dub_translate::WHISPER_LANGS.iter().map(|(c, _)| *c).collect();
    // Тот же JSON-контракт, что в app.py.capabilities(), плюс поля выбора ASR-движка.
    Json(json!({
        "device": o.device,
        "tts_quant": o.tts_quant,
        "asr_model": o.asr_model,
        "models": {
            "asr": o.asr_model,
            "llm": o.mt_model_path.to_string_lossy(),
            "vision": o.mmproj_path.to_string_lossy(),
            "tts": o.tts_model,
        },
        "ffmpeg": ffmpeg,
        "languages": langs,
        "voice_modes": ["clone","autocast","auto","voice"],
        // Выбор ASR: движок (parakeet|whisper), модель Whisper, квант Whisper (compute_type).
        "selection": sel,
        "asr_engines": ["parakeet","whisper"],
        "whisper_models": ["tiny","base","small","medium","large-v3","large-v3-turbo"],
        // Кванты Whisper (compute_type): float16 / int8_float16 задействуют Tensor Cores на CUDA GPU.
        "whisper_computes": ["int8","int8_float16","float16","int8_float32","float32"],
        // Видимые лимиты RAM (настройки, не авто-магия): против OOM на слабой памяти. "0" = авто (дефолт).
        "llama_ubatches": ["0","512","256","128"],      // prefill-батч Gemma (меньше = меньше RAM)
        "higgs_ref_secs_opts": ["12","8","6","4"],       // длина реф-клипа клона (сек; <12 спасает 32ГБ)
    }))
}

fn which_ffmpeg() -> bool {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
        })
        .unwrap_or(false)
}

// ─── /setup/status ; /setup/download ; /setup/cancel ; /setup/remove ; /setup/open-models ───────────

/// Ответ на отказ закачки/удаления: HTTP-статус по коду + {code, detail}.
fn dl_error_response(e: setup::DlError) -> Response {
    let status = match e.code {
        "busy" => StatusCode::CONFLICT,
        "disk_space" => StatusCode::INSUFFICIENT_STORAGE,
        "nothing_to_download" | "unknown_component" | "not_removable" | "no_ids" => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "code": e.code, "detail": e.detail }))).into_response()
}

fn join_failure(e: tokio::task::JoinError) -> Response {
    internal_error(e.to_string())
}

fn body_ids(body: &Value) -> Vec<String> {
    body.get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

/// Полный статус «Первого запуска» с фоновой закачкой (ФС-обход и первая проба драйвера — в блокирующем пуле).
/// Ошибка — текст для ответа 500 (internal_error).
async fn full_setup_status(st: &AppState) -> Result<Value, String> {
    let root = st.repo_root.clone();
    let mut status = tokio::task::spawn_blocking(move || setup::setup_status(&root))
        .await
        .map_err(|e| e.to_string())?;
    status.active = st.downloads.active();
    serde_json::to_value(status).map_err(|e| e.to_string())
}

fn internal_error(detail: String) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "code": "internal", "detail": detail }))).into_response()
}

/// GET /setup/status — статус каждого компонента (installed/missing/размер/место), папка моделей и свободное
/// место, видеокарта против CUDA 13 и фоновая закачка (active). Читает только диск — безопасно до закачки.
async fn setup_status(State(st): State<AppState>) -> Response {
    match full_setup_status(&st).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => internal_error(e),
    }
}

/// POST /setup/download {ids:[...]} — запустить фоновую закачку (мимо GPU-очереди): {download}. Идущая
/// закачка — 409 busy, нехватка места — 507 disk_space. Прогресс — в GET /setup/status (active).
async fn setup_download(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let ids = body_ids(&body);
    if ids.is_empty() {
        return dl_error_response(setup::DlError::new("no_ids", "ids пуст"));
    }
    let dl = st.downloads.clone();
    match tokio::task::spawn_blocking(move || dl.start(ids)).await {
        Ok(Ok(job)) => Json(json!({ "download": job })).into_response(),
        Ok(Err(e)) => dl_error_response(e),
        Err(e) => join_failure(e),
    }
}

/// POST /setup/cancel — пауза фоновой закачки: скачанное остаётся и докачивается следующим /setup/download.
async fn setup_cancel(State(st): State<AppState>) -> Json<Value> {
    Json(json!({ "paused": st.downloads.pause() }))
}

/// POST /setup/remove {ids:[...]} — удалить скачанные компоненты: {removed, freedBytes, errors, status}.
async fn setup_remove(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let ids = body_ids(&body);
    if ids.is_empty() {
        return dl_error_response(setup::DlError::new("no_ids", "ids пуст"));
    }
    let root = st.repo_root.clone();
    let dl = st.downloads.clone();
    let res = tokio::task::spawn_blocking(move || {
        let mut report = setup::remove_components(&root, &ids)?;
        dl.forget_covering(&ids);
        if ids.iter().any(|id| id == ytdlp::COMPONENT) {
            report.errors.extend(ytdlp::remove_updates(&root));
        }
        Ok::<_, setup::DlError>(report)
    })
    .await;
    let report = match res {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return dl_error_response(e),
        Err(e) => return join_failure(e),
    };
    let status = match full_setup_status(&st).await {
        Ok(v) => v,
        Err(e) => return internal_error(e),
    };
    Json(json!({ "removed": report.removed, "freedBytes": report.freed_bytes, "errors": report.errors, "status": status }))
        .into_response()
}

/// Открыть каталог в проводнике (без выделения файла).
fn open_folder(dir: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let cmd = "explorer.exe";
    #[cfg(not(windows))]
    let cmd = "xdg-open";
    std::process::Command::new(cmd).arg(dir).spawn().map(|_| ())
}

/// POST /setup/open-models — открыть папку моделей в проводнике: {path}.
async fn setup_open_models(State(st): State<AppState>) -> Response {
    let dir = st.repo_root.join("models");
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| open_folder(&dir)) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "code": "open_failed", "detail": format!("{}: {e}", dir.display()) })))
            .into_response();
    }
    Json(json!({ "path": dir.to_string_lossy() })).into_response()
}

/// ON-DEMAND: перед джобой догружаем компоненты, нужные ИМЕННО ДЛЯ ЭТОЙ функции/конфига, если их нет
/// на диске. Юзер запросил кастинг, но модели лиц не скачаны -> тянем их тут же (прогресс в тот же SSE)
/// и продолжаем — вместо «0 лиц/все SPK0». Аналогично GPU-стадия без CUDA-стека -> догружаем cuFFT/onnx-GPU/
/// cuDNN. Пустой список = ничего не качаем, идём дальше мгновенно.
fn ensure_job_components(
    repo_root: &Path,
    models_root: &Path,
    casting: bool,
    diarization: bool,
    progress: &setup::ProgressCb,
) -> Result<(), String> {
    let manifest = setup::manifest();
    let missing = |id: &str| -> bool {
        manifest
            .iter()
            .find(|c| c.id == id)
            .map(|c| !setup::component_status(repo_root, c).installed)
            .unwrap_or(false)
    };
    let mut need: Vec<String> = Vec::new();
    // Кастинг персонажей (#115) → его модели (детект лиц + эмбеддинг лица/голоса + аниме/окклюдер).
    if casting && missing("casting") {
        need.push("casting".to_string());
    }
    // Любая локальная стадия на GPU → полный CUDA-стек: диаризация/ASR грузят onnxruntime-GPU + cuDNN,
    // ggml-движки (сепарация/Higgs) — cuda-runtime (cudart/cublas/cuFFT). Без cuFFT CUDA-EP не поднимется.
    let gpu = ["sep_backend", "diar_backend", "asr_backend"]
        .iter()
        .any(|k| models::stage_backend(models_root, k) == "gpu");
    if gpu {
        for id in ["cuda-runtime", "onnxruntime-gpu", "cudnn", "bsroformer-engine"] {
            if missing(id) {
                need.push(id.to_string());
            }
        }
    }
    // Whisper на GPU: CTranslate2 требует cuBLAS12 + cuDNN8 РЯДОМ с whisper-faster.exe (наш ONNX-стек
    // выше — CUDA 13, whisper'у не подходит). Без них whisper молча идёт на CPU. Тянем их только когда
    // движок ASR = Whisper И стадия ASR на GPU.
    let asr_whisper = models::load_selection(models_root)
        .get("asr_engine")
        .and_then(|v| v.as_str())
        == Some("whisper");
    if asr_whisper && models::stage_backend(models_root, "asr_backend") == "gpu" && missing("whisper-cuda") {
        need.push("whisper-cuda".to_string());
    }
    // Диаризация (дубляж/транскрипт) → модель Nemotron 3 Diarization. Её сбой закачки не валит джобу:
    // анализ штатно деградирует в single-speaker и пишет об этом в журнал (см. analyze::run).
    let need_diar = diarization && missing("sortformer");
    if need.is_empty() && !need_diar {
        return Ok(());
    }
    progress(json!({ "stage": "download", "msg": "Догружаю недостающие модели для этой функции…" }));
    let ctl = jobs::current();
    let cancel = move || ctl.as_ref().is_some_and(|c| c.is_cancelled());
    if !need.is_empty() {
        setup::download_components(repo_root, &need, &cancel, progress).map_err(|e| e.detail)?;
    }
    if need_diar {
        if let Err(e) = setup::download_components(repo_root, &["sortformer".to_string()], &cancel, progress) {
            progress(json!({
                "stage": "download",
                "msg": format!("Модель диаризации не скачалась ({}) — анализ пойдёт без разделения спикеров", e.detail),
            }));
        }
    }
    Ok(())
}

/// GET /hw/snapshot — снимок GPU/VRAM/темп/мощность + RAM для монитора ресурсов.
async fn hw_snapshot() -> Json<hw::HardwareSnapshot> {
    Json(tokio::task::spawn_blocking(hw::snapshot).await.unwrap_or_default())
}

/// Безопасное имя голоса: только буквы/цифры/пробел/дефис/подчёркивание (без разделителей путей).
fn sanitize_voice_name(s: &str) -> String {
    let n: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .take(48)
        .collect();
    let n = n.trim().to_string();
    if n.is_empty() { "Мой голос".to_string() } else { n }
}

/// Имена голосов в каталоге: стемы .wav/.mp3 (запись с микрофона = .wav; пак = .mp3).
fn list_voice_names(dir: &Path) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if ext == "wav" || ext == "mp3" {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    names.insert(stem.to_string());
                }
            }
        }
    }
    names.into_iter().collect()
}

/// GET /voices — список голосов из каталога (пак + записи с микрофона).
async fn voices_list(State(st): State<AppState>) -> Json<Value> {
    Json(json!({ "voices": list_voice_names(&st.voices_dir) }))
}

/// GET /voices/sample?name=<name> — отдать аудио-сэмпл голоса (voices/<name>.wav|.mp3) с Range для <audio>
/// (прослушка выбранного голоса в UI). sanitize защищает от path-traversal.
async fn voice_sample(
    State(st): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let name = sanitize_voice_name(q.get("name").map(|s| s.as_str()).unwrap_or(""));
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "no name").into_response();
    }
    for ext in ["wav", "mp3"] {
        let p = st.voices_dir.join(format!("{name}.{ext}"));
        if p.is_file() {
            return serve_file_range(&p, req, None).await;
        }
    }
    (StatusCode::NOT_FOUND, "voice not found").into_response()
}

/// GET /record/devices — список микрофонов.
async fn record_devices() -> Json<Value> {
    Json(json!({ "devices": tokio::task::spawn_blocking(record::input_devices).await.unwrap_or_default() }))
}

/// GET /record/level — текущий пик 0..1 (метр уровня).
async fn record_level() -> Json<Value> {
    Json(json!({ "level": record::level() }))
}

/// POST /record/start {name, device?} — начать запись голоса в voices/<name>.wav.
async fn record_start(State(st): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let name = sanitize_voice_name(body.get("name").and_then(|v| v.as_str()).unwrap_or("Мой голос"));
    let device = body.get("device").and_then(|v| v.as_str()).map(|s| s.to_string());
    let path = st.voices_dir.join(format!("{name}.wav"));
    match tokio::task::spawn_blocking(move || record::start(&path, device)).await {
        Ok(Ok(())) => Json(json!({ "ok": true, "name": name })),
        Ok(Err(e)) => Json(json!({ "ok": false, "error": e })),
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

/// POST /record/stop — остановить запись; вернуть имя записанного голоса + обновлённый список.
async fn record_stop(State(st): State<AppState>) -> Json<Value> {
    let path = tokio::task::spawn_blocking(record::stop).await;
    let name = match path {
        Ok(Ok(p)) => p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()),
        _ => None,
    };
    Json(json!({ "name": name, "voices": list_voice_names(&st.voices_dir) }))
}

/// POST /projects/{pid}/speaker-voice {speaker, name} — сделать голос из спикера: его длиннейшая
/// реплика (<=12с) чистым вокалом в полной полосе → voices/<name>.wav (render::speaker_voice_clip).
/// Порт «Сделать голос» Higgs. Отказ — JSON {error, detail}: 400 no_speaker_lines, 409 no_separation (нет ни
/// вокала проекта, ни движка сепарации той сборки, что выбрана для стадии), 500 separation_failed и
/// speaker_voice_failed.
async fn speaker_voice(
    State(st): State<AppState>,
    axum::extract::Path(pid): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let want = body.get("speaker").and_then(|v| v.as_str()).unwrap_or("0").to_string();
    let name = sanitize_voice_name(body.get("name").and_then(|v| v.as_str()).unwrap_or(""));
    // длиннейшая реплика спикера.
    let cand = proj
        .segments
        .iter()
        .filter(|s| s.speaker.as_deref().unwrap_or("0") == want)
        .max_by(|a, b| (a.end - a.start).partial_cmp(&(b.end - b.start)).unwrap_or(std::cmp::Ordering::Equal));
    let refused = |status: StatusCode, code: &str, detail: String| {
        (status, Json(json!({ "error": code, "detail": detail }))).into_response()
    };
    let Some(cand) = cand else {
        return refused(StatusCode::BAD_REQUEST, "no_speaker_lines", want);
    };
    let cand = cand.clone();
    let input = std::fs::read_to_string(dir.join("source.txt")).unwrap_or_default();
    let input = if !input.trim().is_empty() { PathBuf::from(input.trim()) } else { dir.join("source.mp4") };
    let out = st.voices_dir.join(format!("{name}.wav"));
    let txt = st.voices_dir.join(format!("{name}.txt"));
    let voices_dir = st.voices_dir.clone();
    let repo_root = st.repo_root.clone();
    let models_root = st.models_root.clone();
    let model = st.bsroformer_model.clone();
    let tmp = dir.join("_voicecut");
    let res = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&voices_dir).map_err(|e| format!("{}: {e}", voices_dir.display()))?;
        let cli = dub_sep::engine_cli(&repo_root, models::stage_backend(&models_root, "sep_backend"));
        let made = render::speaker_voice_clip(&cand, 12.0, &dir, &input, (&cli, &model), &tmp, &out);
        let cleaned =
            if tmp.exists() { std::fs::remove_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display())) } else { Ok(()) };
        let text = made?;
        cleaned?;
        // Голос несёт в библиотеке текст, который в нём звучит; без него старый текст того же имени убирается.
        match text {
            Some(t) => std::fs::write(&txt, t).map_err(|e| format!("{}: {e}", txt.display()))?,
            None if txt.exists() => std::fs::remove_file(&txt).map_err(|e| format!("{}: {e}", txt.display()))?,
            None => {}
        }
        Ok::<(), render::VoiceClipError>(())
    })
    .await;
    match res {
        Ok(Ok(())) => Json(json!({ "ok": true, "name": name, "voices": list_voice_names(&st.voices_dir) })).into_response(),
        Ok(Err(render::VoiceClipError::NoSeparator(missing))) => refused(StatusCode::CONFLICT, "no_separation", missing),
        Ok(Err(render::VoiceClipError::Separation(e))) => refused(StatusCode::INTERNAL_SERVER_ERROR, "separation_failed", e),
        Ok(Err(render::VoiceClipError::Io(e))) => refused(StatusCode::INTERNAL_SERVER_ERROR, "speaker_voice_failed", e),
        Err(e) => refused(StatusCode::INTERNAL_SERVER_ERROR, "speaker_voice_failed", e.to_string()),
    }
}

/// POST /projects/{pid}/voice-slots {male:[имена], female:[имена]} — авто-распределение голосов-слотов
/// по спикерам (#114). Валидирует, что имена есть в voices/; определяет пол каждого спикера по медиане F0
/// его реплик на чистом вокале, ранжирует по длительности речи, раскладывает по слотам (по кругу сверх
/// числа слотов), записывает per-speaker голоса (voice.mode/name) и метит сегменты dirty (ре-синтез).
/// Возвращает итоговый маппинг {speaker:{voice,gender,f0}}. Пустой список пола -> клон; оба пусты -> клон.
async fn voice_slots_assign(
    State(st): State<AppState>,
    axum::extract::Path(pid): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if let Err(r) = st.load_project(&pid) {
        return r;
    }
    // Списки имён из тела; проверяем существование каждого в voices/ (.wav|.mp3).
    let slots = voice_slots::Slots::from_json(&body);
    if let Some(n) = slots.missing_in(&list_voice_names(&st.voices_dir)).first() {
        return (StatusCode::BAD_REQUEST, format!("голос {n:?} не найден в voices/")).into_response();
    }
    let Some(vocals) = voice_slots::vocals_for(&dir) else {
        return (StatusCode::CONFLICT, "нет вокала для замера F0 — сначала analyze").into_response();
    };

    let dir_job = dir.clone();
    let res = tokio::task::spawn_blocking(mcp::carry(move || {
        let path = dir_job.join("project.json");
        let read = || -> Result<Project, String> {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("чтение {}: {e}", path.display()))?;
            Project::from_json(&text).map_err(|e| format!("разбор {}: {e}", path.display()))
        };
        let measured = read()?;
        let assigns = voice_slots::plan(&measured, &vocals, &dir_job, &slots);
        // Замер F0 идёт секунды: правки, пришедшие за это время, остаются, а голоса ложатся, только пока
        // спикеры те же, что мерились.
        let _held = project_writes();
        let mut proj = read()?;
        if voice_slots::speakers(&proj) != voice_slots::speakers(&measured) {
            return Err("голоса по слотам: спикеры изменились, пока мерился голос — проект не менялся, запустите ещё раз".to_string());
        }
        voice_slots::apply(&mut proj, &assigns);
        write_project(&dir_job, &proj)?;
        Ok::<_, String>((assigns, proj))
    }))
    .await;
    match res {
        Ok(Ok((assigns, proj))) => Json(json!({
            "ok": true,
            "voice_mode": proj.audio.voice.mode,
            "voice_name": proj.audio.voice.name,
            "speakers": voice_slots::mapping(&assigns),
        }))
        .into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ─── Кастинг персонажей (#115) ──────────────────────────────────────────────

/// Единый шейп персонажа для фронта (тип Character в api.ts): id/name/gender/voice/speaker_ids/
/// line_count/sample_frame_url/voice_sample_url. Возвращают ОБА эндпоинта (get + save) — фронт кладёт
/// результат в один стейт Character[]. `voice` = dub_voice бэка; line_count = число сегментов проекта у
/// speaker_ids персонажа; sample_frame_url nullable (закадровый персонаж без кадра -> null); voice_sample_url
/// nullable (нет образца голоса -> null). `proj_dir` — workspace/<pid> для проверки наличия wav-образца.
fn character_json(c: &dub_faces::Character, pid: &str, proj: &Project, proj_dir: &Path) -> Value {
    let ids: std::collections::HashSet<&str> = c.speaker_ids.iter().map(|s| s.as_str()).collect();
    let line_count = proj
        .segments
        .iter()
        .filter(|s| ids.contains(s.speaker.as_deref().unwrap_or("0")))
        .count();
    // Образец голоса: путь из voice_sample (устойчив к смене id при кросс-матче); фолбэк на id-путь для
    // старых casting.json без поля. Есть файл -> URL, иначе null.
    let voice_rel = if c.voice_sample.is_empty() { char_voice_rel(&c.id) } else { Some(c.voice_sample.clone()) };
    let voice_sample_url = if voice_rel.as_ref().map(|r| proj_dir.join(r).is_file()).unwrap_or(false) {
        Value::from(format!("/projects/{pid}/casting/voice?id={}", c.id))
    } else {
        Value::Null
    };
    json!({
        "id": c.id,
        "name": c.name,
        "gender": c.gender,
        "voice": c.dub_voice,
        "speech_note": c.speech_note,
        "speaker_ids": c.speaker_ids,
        "line_count": line_count,
        // URL аватарки эндпоинта (без раскрытия файловых путей). Нет кадра -> null (фронт рисует инициал).
        "sample_frame_url": if c.sample_frame.is_empty() {
            Value::Null
        } else {
            Value::from(format!("/projects/{pid}/casting/avatar?id={}", c.id))
        },
        // URL проигрываемого образца голоса. Нет голоса -> null.
        "voice_sample_url": voice_sample_url,
    })
}

/// Относительный путь к wav-образцу голоса персонажа внутри work_dir по его id (char_<i> -> casting/
/// char_<i>_voice.wav). Нестандартный id -> None (voice_sample_url = null).
fn char_voice_rel(char_id: &str) -> Option<String> {
    // id формата char_<i>; допускаем только безопасные символы (без traversal).
    if char_id.is_empty() || !char_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(format!("casting/{char_id}_voice.wav"))
}

/// GET /projects/{pid}/casting — карточки персонажей из casting.json в шейпе фронта (см. character_json).
/// 404 если casting.json нет (кастинг не запускался).
async fn casting_get(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = dir.join("casting.json");
    let Some(casting) = dub_faces::load_casting(&path) else {
        return (StatusCode::NOT_FOUND, "casting not run").into_response();
    };
    let chars: Vec<Value> = casting
        .characters
        .iter()
        .map(|c| character_json(c, &pid, &proj, &dir))
        .collect();
    Json(json!({ "version": casting.version, "characters": chars })).into_response()
}

/// GET /projects/{pid}/casting/avatar?id=<char_id> — кадр-аватарка персонажа (JPG, с Range).
async fn casting_avatar(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let id = q.get("id").map(|s| s.as_str()).unwrap_or("");
    let Some(casting) = dub_faces::load_casting(&dir.join("casting.json")) else {
        return (StatusCode::NOT_FOUND, "casting not run").into_response();
    };
    let Some(ch) = casting.characters.iter().find(|c| c.id == id) else {
        return (StatusCode::NOT_FOUND, "character not found").into_response();
    };
    if ch.sample_frame.is_empty() {
        return (StatusCode::NOT_FOUND, "no avatar").into_response();
    }
    // sample_frame — относительный путь внутри work_dir (casting/char_N.jpg); отсекаем traversal.
    if ch.sample_frame.contains("..") {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    let p = dir.join(&ch.sample_frame);
    if !p.is_file() {
        return (StatusCode::NOT_FOUND, "avatar file missing").into_response();
    }
    serve_file_range(&p, req, None).await
}

/// GET /projects/{pid}/casting/voice?id=<char_id> — проигрываемый образец голоса персонажа (WAV, с Range).
/// Из casting/char_<i>_voice.wav (кладёт casting.rs). Нет файла -> 404 (voice_sample_url и так был null).
async fn casting_voice(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let id = q.get("id").map(|s| s.as_str()).unwrap_or("");
    // Существует ли такой персонаж (id из casting.json) — иначе 404, не раскрываем ФС.
    let Some(casting) = dub_faces::load_casting(&dir.join("casting.json")) else {
        return (StatusCode::NOT_FOUND, "casting not run").into_response();
    };
    let Some(ch) = casting.characters.iter().find(|c| c.id == id) else {
        return (StatusCode::NOT_FOUND, "character not found").into_response();
    };
    // Резолвим по voice_sample (id мог смениться при кросс-матче — файл назван по индексу эпизода); фолбэк
    // на id-путь для старых casting.json. Гард от traversal: только casting/*.wav без "..".
    let rel = if ch.voice_sample.is_empty() {
        match char_voice_rel(id) {
            Some(r) => r,
            None => return (StatusCode::BAD_REQUEST, "bad id").into_response(),
        }
    } else if ch.voice_sample.starts_with("casting/") && !ch.voice_sample.contains("..") {
        ch.voice_sample.clone()
    } else {
        return (StatusCode::BAD_REQUEST, "bad voice path").into_response();
    };
    let p = dir.join(&rel);
    if !p.is_file() {
        return (StatusCode::NOT_FOUND, "no voice sample").into_response();
    }
    serve_file_range(&p, req, None).await
}

// ─── App-библиотека кастингов (#115): профили, переносимые между роликами ─────

/// GET /casting/library -> {"casts":[{"slug","name","char_count"}]} — список сохранённых профилей.
async fn casting_library_list(State(st): State<AppState>) -> Response {
    let repo_root = st.repo_root.clone();
    let profiles = tokio::task::spawn_blocking(move || casting_library::list_profiles(&repo_root))
        .await
        .unwrap_or_default();
    let casts: Vec<Value> = profiles
        .into_iter()
        .map(|(slug, m)| json!({ "slug": slug, "name": m.name, "char_count": m.char_count }))
        .collect();
    Json(json!({ "casts": casts })).into_response()
}

/// POST /projects/{pid}/casting/library {name, created?} -> {"slug"} — сохранить текущий casting проекта
/// в библиотеку (casting.json + аватарки + образцы голоса). slug из name (транслит/kebab, уникализируется).
async fn casting_library_save(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "name required").into_response();
    }
    // created — из тела запроса (Date::now в Rust не используем); пусто допустимо.
    let created = body.get("created").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let repo_root = st.repo_root.clone();
    let res = tokio::task::spawn_blocking(move || {
        let base = casting_library::slugify(&name);
        let slug = casting_library::unique_slug(&repo_root, &base);
        casting_library::save_profile(&repo_root, &dir, &slug, &name, &created).map(|_| slug)
    })
    .await;
    match res {
        Ok(Ok(slug)) => Json(json!({ "slug": slug })).into_response(),
        Ok(Err(e)) => (StatusCode::CONFLICT, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// DELETE /casting/library/{slug} -> {"ok":true} — удалить профиль библиотеки.
async fn casting_library_delete(
    State(st): State<AppState>,
    AxPath(slug): AxPath<String>,
) -> Response {
    if !casting_library::is_safe_slug(&slug) {
        return (StatusCode::BAD_REQUEST, "bad slug").into_response();
    }
    let repo_root = st.repo_root.clone();
    let res = tokio::task::spawn_blocking(move || casting_library::delete_profile(&repo_root, &slug)).await;
    match res {
        Ok(Ok(())) => Json(json!({ "ok": true })).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /casting/library/{slug}/avatar?id=<char_id> — аватарка персонажа профиля (JPG/PNG, с Range).
async fn casting_library_avatar(
    State(st): State<AppState>,
    AxPath(slug): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    if !casting_library::is_safe_slug(&slug) {
        return (StatusCode::BAD_REQUEST, "bad slug").into_response();
    }
    let id = q.get("id").map(|s| s.as_str()).unwrap_or("");
    // id формата char_<i>; берём аватарку avatars/char_<i>.jpg (нативный JPG), фолбэк на .png для старых
    // профилей. Эндпоинт резолвит по id персонажа.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return (StatusCode::BAD_REQUEST, "bad id").into_response();
    }
    let dir = casting_library::profile_dir(&st.repo_root, &slug);
    // Проверяем, что такой персонаж есть в профиле (не раскрываем ФС при мусорном id).
    let Some(casting) = casting_library::load_profile_casting(&st.repo_root, &slug) else {
        return (StatusCode::NOT_FOUND, "profile not found").into_response();
    };
    if !casting.characters.iter().any(|c| c.id == id) {
        return (StatusCode::NOT_FOUND, "character not found").into_response();
    }
    let avdir = dir.join("avatars");
    let p = {
        let jpg = avdir.join(format!("{id}.jpg"));
        if jpg.is_file() { jpg } else { avdir.join(format!("{id}.png")) }
    };
    if !p.is_file() {
        return (StatusCode::NOT_FOUND, "avatar file missing").into_response();
    }
    serve_file_range(&p, req, None).await
}

/// POST /projects/{pid}/casting {characters:[{id,name,speech_note,dub_voice,gender?}]} — сохранить правки
/// юзера в casting.json И применить: (1) per-speaker голоса тем же механизмом, что voice_slots/ручное
/// назначение (voice.mode="voice" + позиционный CSV по отсортированным id спикеров); (2) speech_note ->
/// единый translate_style (пер-спикерный style в ctx_run НЕ поддержан — глобальный; см. casting.rs +
/// translate.rs). Метит сегменты dirty при смене голоса (ре-синтез). Возвращает обновлённый кастинг.
async fn casting_save(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let _held = project_writes();
    let mut proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = dir.join("casting.json");
    let Some(mut casting) = dub_faces::load_casting(&path) else {
        return (StatusCode::CONFLICT, "casting not run").into_response();
    };
    // Применить правки по id: name/speech_note/dub_voice/gender.
    if let Some(edits) = body.get("characters").and_then(|v| v.as_array()) {
        for e in edits {
            let Some(id) = e.get("id").and_then(|v| v.as_str()) else { continue };
            let Some(ch) = casting.characters.iter_mut().find(|c| c.id == id) else { continue };
            if let Some(v) = e.get("name").and_then(|v| v.as_str()) {
                ch.name = v.to_string();
            }
            if let Some(v) = e.get("speech_note").and_then(|v| v.as_str()) {
                ch.speech_note = v.to_string();
            }
            // dub_voice: строка -> назначить; JSON null (фронт шлёт при очистке поля) -> СБРОС на клон ("").
            // Ключ отсутствует -> не трогаем. Иначе вернуть голос на клонирование из UI было невозможно.
            match e.get("dub_voice") {
                Some(v) if v.is_string() => ch.dub_voice = v.as_str().unwrap_or("").to_string(),
                Some(v) if v.is_null() => ch.dub_voice = String::new(),
                _ => {}
            }
            if let Some(v) = e.get("gender").and_then(|v| v.as_str()) {
                ch.gender = v.to_string();
            }
        }
    }
    // Валидация голосов. В ОБЛАЧНОМ TTS-режиме (OpenRouter) имена — облачные голоса, локального файла в
    // voices/ у них нет: проверку против voices/ пропускаем (cloud TTS сам примет имя голоса при синтезе).
    // Локальный режим — как раньше: непустой не-"clone" должен существовать в voices/.
    if !crate::models::openrouter_stage_on(&st.models_root, "tts") {
        let available: std::collections::BTreeSet<String> =
            list_voice_names(&st.voices_dir).into_iter().collect();
        for c in &casting.characters {
            let v = c.dub_voice.trim();
            if !v.is_empty() && !v.eq_ignore_ascii_case("clone") && !available.contains(v) {
                return (StatusCode::BAD_REQUEST, format!("голос {v:?} не найден в voices/")).into_response();
            }
        }
    }

    // (1) применить голоса per-speaker: позиционный CSV по лексикографически отсортированным id спикеров
    // (как читает render.rs / пишет voice_slots). Слияние ПОВЕРХ старого CSV — см. merge_voice_csv:
    // спикера на 'clone' в кастинге -> "-", спикера НЕ из кастинга -> сохраняем его позицию из #114.
    let vmap = casting::casting_voice_map(&casting);
    let mut spk_ids: Vec<String> = proj
        .segments
        .iter()
        .map(|s| s.speaker.clone().unwrap_or_else(|| "0".to_string()))
        .collect();
    spk_ids.sort();
    spk_ids.dedup();
    let any_voice = vmap.values().any(|v| v.is_some());
    // Голоса из кастинга применяем ТОЛЬКО когда юзер реально назначил хоть один (any_voice). Иначе
    // (все на "clone") НЕ трогаем proj.audio.voice — иначе перезатёрли бы библиотечные голоса из #114
    // (voice_slots) и пометили бы ВСЕ сегменты dirty на пустой ре-синтез.
    if any_voice {
        let old_name = proj.audio.voice.name.clone();
        // Новый CSV строим ПОВЕРХ текущего (см. casting::merge_voice_csv): спикеров, НЕ настроенных в
        // кастинге, НЕ трогаем — сохраняем их позицию из voice_slots (#114). "-" ставим только тем, кого
        // реально выставили в кастинге на 'clone'. Формат позиционный по отсортированным id (как render.rs).
        let old_is_voice = proj.audio.voice.mode == "voice";
        let csv = casting::merge_voice_csv(&spk_ids, &vmap, old_name.as_deref(), old_is_voice);
        proj.audio.voice.mode = "voice".to_string();
        proj.audio.voice.name = Some(csv);
        // Смена голосов -> ре-синтез: метим сегменты dirty (как voice_slots).
        if proj.audio.voice.name != old_name {
            for seg in &mut proj.segments {
                seg.dirty = true;
            }
        }
    }

    // (2) speech_note -> глобальный translate_style (документированное ограничение: ctx_run не берёт
    // пер-спикерный стиль). Дополняем существующий стиль заметками персонажей.
    let notes = casting::speech_notes_to_style(&casting);
    proj.audio.translate_style = casting::merge_speech_notes_style(&proj.audio.translate_style, &notes);

    // Сохранить casting.json + project.json.
    if let Err(e) = dub_faces::save_casting(&path, &casting) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    if let Err(e) = write_project(&dir, &proj) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    // Возвращаем ПОЛНЫЙ список персонажей в шейпе фронта (тот же, что casting_get) — фронт кладёт его
    // прямо в стейт Character[]. line_count считаем по обновлённому proj (спикеры не менялись, но шейп единый).
    let chars: Vec<Value> = casting
        .characters
        .iter()
        .map(|c| character_json(c, &pid, &proj, &dir))
        .collect();
    Json(json!({ "ok": true, "characters": chars })).into_response()
}

/// GET /voices/catalog — каталог доп-голосов (HF датасет Slait/russia_voices): имя, пол, URL-превью. Hugging Face
/// не ответил — 502 {error, detail}.
async fn voices_catalog() -> Response {
    match tokio::task::spawn_blocking(record::catalog).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": "catalog_unavailable", "detail": e }))).into_response(),
        Err(e) => join_failure(e),
    }
}

/// POST /voices/get {name} — скачать один голос (mp3+txt) из датасета в каталог голосов.
async fn voices_get(State(st): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let dir = st.voices_dir.clone();
    let ok = tokio::task::spawn_blocking(move || record::fetch_voice(&dir, &name)).await;
    match ok {
        Ok(Ok(())) => Json(json!({ "ok": true, "voices": list_voice_names(&st.voices_dir) })),
        Ok(Err(e)) => Json(json!({ "ok": false, "error": e })),
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

/// POST /voices/rename {from, to} — переименовать голос (mp3/wav + txt).
async fn voices_rename(State(st): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let from = sanitize_voice_name(body.get("from").and_then(|v| v.as_str()).unwrap_or(""));
    let to = sanitize_voice_name(body.get("to").and_then(|v| v.as_str()).unwrap_or(""));
    for ext in ["wav", "mp3", "txt"] {
        let src = st.voices_dir.join(format!("{from}.{ext}"));
        if src.is_file() {
            let _ = std::fs::rename(&src, st.voices_dir.join(format!("{to}.{ext}")));
        }
    }
    Json(json!({ "voices": list_voice_names(&st.voices_dir) }))
}

/// POST /voices/delete {name} — удалить голос.
async fn voices_delete(State(st): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let name = sanitize_voice_name(body.get("name").and_then(|v| v.as_str()).unwrap_or(""));
    for ext in ["wav", "mp3", "txt"] {
        let _ = std::fs::remove_file(st.voices_dir.join(format!("{name}.{ext}")));
    }
    Json(json!({ "voices": list_voice_names(&st.voices_dir) }))
}

/// POST /voices/download-pack — скачать пак голосов (VibeVoice) и распаковать в каталог голосов.
async fn voices_download_pack(State(st): State<AppState>) -> Response {
    let dir = st.voices_dir.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        let cb = |ev: Value| progress(ev);
        record::download_pack(&dir, &cb)
    });
    match st.jobs.enqueue(jobs::JobMeta::new(jobs::JobKind::VoicesPack, None), job).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(e) => enqueue_error(e),
    }
}

/// POST /setup/browse — открыть нативный диалог выбора папки и импортировать оттуда готовые веса (без
/// докачки): {picked, imported, files, errors, status}.
async fn setup_browse(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let root = st.repo_root.clone();
    let only = body.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let res = tokio::task::spawn_blocking(move || {
        let title = if only.is_some() { "Файл(ы) модели" } else { "Папка с готовыми моделями" };
        rfd::FileDialog::new()
            .set_title(title)
            .pick_folder()
            .map(|d| setup::import_from_dir(&root, &d, only.as_deref()))
    })
    .await;
    let report = match res {
        Ok(r) => r,
        Err(e) => return join_failure(e),
    };
    let status = match full_setup_status(&st).await {
        Ok(v) => v,
        Err(e) => return internal_error(e),
    };
    let picked = report.is_some();
    let report = report.unwrap_or_default();
    Json(json!({ "picked": picked, "imported": report.imported, "files": report.files, "errors": report.errors, "status": status }))
        .into_response()
}

/// POST /setup/import {path, id?} — импортировать готовые веса из заданной папки (без диалога):
/// {imported, files, errors, status}.
async fn setup_import(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let path = body.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if path.is_empty() || !Path::new(&path).is_dir() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "code": "bad_path", "detail": format!("нет папки {path}") }))).into_response();
    }
    let only = body.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let root = st.repo_root.clone();
    let report = match tokio::task::spawn_blocking(move || setup::import_from_dir(&root, Path::new(&path), only.as_deref())).await {
        Ok(r) => r,
        Err(e) => return join_failure(e),
    };
    let status = match full_setup_status(&st).await {
        Ok(v) => v,
        Err(e) => return internal_error(e),
    };
    Json(json!({ "imported": report.imported, "files": report.files, "errors": report.errors, "status": status })).into_response()
}

// ─── POST /projects (multipart video upload) ────────────────────────────────

async fn create_project(
    State(st): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let mut pid = uuid::Uuid::new_v4().simple().to_string();
    pid.truncate(12);
    let d = st.workspace.join(&pid);
    if let Err(e) = tokio::fs::create_dir_all(&d).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    // Поля мультипарта: видео + (опц.) файл субтитров. Роутим по расширению — порядок неважен.
    // Субтитры (srt/ass/ssa) -> import_subs.<ext> (analyze возьмёт текст+тайминг оттуда, ASR skip).
    let mut filename: Option<String> = None;
    let mut imported_subs = false;
    while let Ok(Some(field)) = multipart.next_field().await {
        let fname = field.file_name().map(|s| s.to_string());
        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
        let ext = fname
            .as_deref()
            .and_then(|f| Path::new(f).extension())
            .and_then(|e| e.to_str())
            .unwrap_or("mp4")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "srt" | "ass" | "ssa") {
            let dst = d.join(format!("import_subs.{ext}"));
            if let Err(e) = tokio::fs::write(&dst, &data).await {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
            imported_subs = true;
            continue;
        }
        let dst = d.join(format!("source.{ext}"));
        if let Err(e) = tokio::fs::write(&dst, &data).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
        // source.txt хранит абсолютный путь к видео (как в app.py).
        let _ = tokio::fs::write(d.join("source.txt"), dst.to_string_lossy().as_bytes()).await;
        // name.txt — исходное имя файла (для списка «недавние проекты»; meta.video хранит внутреннее source.*).
        if let Some(n) = fname.as_deref() {
            let _ = tokio::fs::write(d.join("name.txt"), n.as_bytes()).await;
        }
        filename = fname;
    }

    // Создаём начальный project.json, чтобы проект сразу мог быть открыт в редакторе без вызова analyze
    if d.join("source.txt").is_file() {
        let dir = d.clone();
        match tokio::task::spawn_blocking(mcp::carry(move || write_initial_project(&dir))).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
            Err(e) => return join_failure(e),
        }
    }

    Json(json!({ "project_id": pid, "filename": filename, "imported_subs": imported_subs })).into_response()
}

/// Начальный project.json проекта из его source.txt (и import_subs.*, если субтитры пришли вместе с видео): проект
/// открывается в редакторе без analyze. Общий для загрузки файла и загрузки по ссылке.
pub(crate) fn write_initial_project(d: &Path) -> Result<(), String> {
    let src_path_str = std::fs::read_to_string(d.join("source.txt")).map_err(|e| format!("{}: {e}", d.join("source.txt").display()))?;
    let src_path = Path::new(src_path_str.trim());
    let meta = match media::probe(src_path) {
        Ok(m) => dub_core::Meta {
            video: src_path.to_string_lossy().to_string(),
            duration: m.duration,
            width: m.width,
            height: m.height,
            fps: m.fps,
            src_codec: m.src_codec,
            extra: serde_json::Map::new(),
        },
        Err(_) => dub_core::Meta {
            video: src_path.to_string_lossy().to_string(),
            duration: 0.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            src_codec: String::new(),
            extra: serde_json::Map::new(),
        },
    };
    save_project_atomic(d, &initial_project(d, meta))
}

/// Начальный project.json нового проекта, чтобы редактор открыл его без analyze: медиа и реплики
/// из субтитров, загруженных вместе с видео (import_subs.*).
fn initial_project(d: &Path, meta: dub_core::Meta) -> Project {
    let mut segments = Vec::new();
    for ext in ["srt", "ass", "ssa"] {
        let sub_file = d.join(format!("import_subs.{ext}"));
        if sub_file.is_file() {
            if let Ok(txt) = std::fs::read_to_string(&sub_file) {
                let cues = subimport::parse(&txt, ext);
                for (i, c) in cues.into_iter().enumerate() {
                    segments.push(dub_core::Segment {
                        id: format!("seg_{}", i + 1),
                        start: c.start,
                        end: c.end,
                        speaker: Some("0".to_string()),
                        src_text: c.text.clone(),
                        tgt_text: c.text,
                        voice: None,
                        dirty: true,
                        ckpt: None,
                        extra: serde_json::Map::new(),
                    });
                }
            }
            break;
        }
    }
    dub_core::Project {
        meta,
        mode: "nodub".to_string(),
        tgt_lang: "ru".to_string(),
        segments,
        audio: dub_core::Audio::default(),
        subs: dub_core::Subs::default(),
        captions: dub_core::Captions::default(),
        render: dub_core::Render::default(),
        ..Default::default()
    }
}

// ─── GET /projects ──────────────────────────────────────────────────────────
// Список сохранённых проектов (у которых есть project.json) для экрана «Открыть/недавние».
// Всё уже персистится в workspace/<pid>/ (каждый PATCH пишет project.json — автосейв),
// здесь просто отдаём сводку, сортированную по времени последней правки (новые сверху).
async fn list_projects(State(st): State<AppState>) -> Response {
    let mut items: Vec<Value> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&st.workspace) {
        for e in rd.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            let pj = dir.join("project.json");
            if !pj.is_file() {
                continue; // только проанализированные (редактируемые) проекты
            }
            let pid = match dir.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let proj = match std::fs::read_to_string(&pj)
                .ok()
                .and_then(|t| Project::from_json(&t).ok())
            {
                Some(p) => p,
                None => continue,
            };
            let mtime = std::fs::metadata(&pj)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // Время создания = рождение каталога проекта (mtime project.json меняет каждая правка).
            // ФС без времени рождения -> null, окно сортирует такой проект последним.
            let created = std::fs::metadata(&dir)
                .and_then(|m| m.created())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            // Имя: исходный файл из name.txt (новые проекты), иначе basename meta.video (старые).
            let video = std::fs::read_to_string(dir.join("name.txt"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    Path::new(&proj.meta.video)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(proj.meta.video.as_str())
                        .to_string()
                });
            let audio_only = proj.meta.width <= 0 || proj.meta.height <= 0;
            let done = dir.join("output.mp4").is_file()
                || dir.join("output.mkv").is_file()
                || dir.join("output.wav").is_file();
            // Последняя джоба проекта (job.json): статус/стадия/ошибка для бейджа и «Продолжить».
            let job = match job_store::read(&dir) {
                Ok(Some(r)) => json!({
                    "job_kind": r.kind,
                    "job_state": r.state,
                    "job_stage": r.stage,
                    "job_error": r.error,
                }),
                Ok(None) => json!({ "job_kind": null, "job_state": null, "job_stage": null, "job_error": null }),
                Err(e) => json!({
                    "job_kind": null,
                    "job_state": "failed",
                    "job_stage": null,
                    "job_error": { "code": "job_record_broken", "text": e },
                }),
            };
            let mut item = json!({
                "pid": pid,
                "video": video,
                "tgt_lang": proj.tgt_lang,
                "mode": proj.mode,
                "width": proj.meta.width,
                "height": proj.meta.height,
                "duration": proj.meta.duration,
                "segments": proj.segments.len(),
                "created": created,
                "audio_only": audio_only,
                "mtime": mtime,
                "done": done,
                "source": atomic::source_of(&dir),
            });
            if let (Some(obj), Value::Object(extra)) = (item.as_object_mut(), job) {
                obj.extend(extra);
            }
            items.push(item);
        }
    }
    items.sort_by(|a, b| {
        b["mtime"].as_u64().unwrap_or(0).cmp(&a["mtime"].as_u64().unwrap_or(0))
    });
    Json(json!({ "projects": items })).into_response()
}

// ─── GET /projects/{pid} ────────────────────────────────────────────────────

async fn get_project(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    match st.load_project(&pid) {
        Ok(p) => project_response(&st, &dir, &p),
        Err(resp) => resp,
    }
}

/// Проект в ответе ручки: с вычисленными у реплик прогнозом укладки (`fit`), сводкой дублей (`takes`) и
/// текстом для синтеза (`tts_text`, `tts_skip`), где он не тот, что показан.
pub(crate) fn project_response(st: &AppState, dir: &Path, proj: &Project) -> Response {
    let rules = fitplan::rules(&st.models_root, st.opts.max_stretch as f64);
    let mut shown = proj.clone();
    tts_text::annotate(&mut shown);
    match fitplan::decorate(dir, &shown, &rules) {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

// ─── DELETE /projects/{pid} — удалить проект (весь каталог workspace/<pid>) ───
// Убирает проект из «последних» на главной И реально стирает его данные с диска.
async fn delete_project(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp, // невалидный/несуществующий pid -> та же ошибка, что у прочих ручек
    };
    if let Some((job_id, kind)) = st.jobs.active_for(&pid).await {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "project_busy", "job_id": job_id, "kind": kind.as_str() })),
        )
            .into_response();
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => ([("content-type", "application/json")], "{\"ok\":true}").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("удаление проекта {}: {e}", dir.display()),
        )
            .into_response(),
    }
}

// ─── POST /projects/{pid}/analyze ───────────────────────────────────────────

async fn analyze_project(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let args: serde_json::Map<String, Value> = q.into_iter().map(|(k, v)| (k, Value::String(v))).collect();
    match analyze_enqueue(&st, &pid, Value::Object(args)).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(resp) => *resp,
    }
}

/// Поставить analyze. `args_json` — параметры query как есть; они же пишутся в job.json, чтобы
/// «Продолжить» повторило анализ с теми же настройками на том же проекте (кэши стадий переиспользуются).
async fn analyze_enqueue(st: &AppState, pid: &str, args_json: Value) -> Result<String, Box<Response>> {
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    let Some(obj) = args_json.as_object() else {
        return Err(Box::new((StatusCode::BAD_REQUEST, "analyze args: ожидался объект").into_response()));
    };
    let q: HashMap<String, String> = obj
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect();
    // Исходный путь видео из source.txt (как в app.py). Fallback — source.* в каталоге.
    let src_txt = dir.join("source.txt");
    let input = match std::fs::read_to_string(&src_txt) {
        Ok(s) => PathBuf::from(s.trim()),
        Err(_) => {
            return Err(Box::new((StatusCode::CONFLICT, "no source uploaded").into_response()));
        }
    };
    if !input.is_file() {
        return Err(Box::new((StatusCode::CONFLICT, "source video missing").into_response()));
    }

    // Параметры analyze из query (дефолты как в app.py.analyze_project).
    let qget = |k: &str, d: &str| q.get(k).cloned().unwrap_or_else(|| d.to_string());
    // Импортированные субтитры (если юзер загрузил их при создании) — берём текст+тайминг оттуда, ASR skip.
    let import_subs = ["srt", "ass", "ssa"]
        .iter()
        .map(|e| dir.join(format!("import_subs.{e}")))
        .find(|p| p.is_file());
    if let Some(p) = &import_subs {
        eprintln!("[analyze] импорт субтитров: {}", p.display());
    }
    let args = analyze::AnalyzeArgs {
        speaker_count: qget("speaker_count", "0").parse::<usize>()
            .ok().filter(|n| *n <= dub_asr::MAX_SPEAKERS)
            .ok_or_else(|| Box::new((StatusCode::BAD_REQUEST, "speaker_count: ожидается целое число от 0 до 8 (0 — автоматически)").into_response()))?,
        tgt_lang: qget("tgt_lang", "en"),
        mode: qget("mode", "auto"),
        src_lang: qget("src_lang", "auto"),
        subs: qget("subs", "auto"),
        rewrite: qget("rewrite", ""),
        translate_style: qget("translate_style", ""),
        burn: qget("burn", "1") != "0",
        detect_text: qget("detect", "1") != "0",
        // кастинг персонажей (#115): ВЫКЛ по умолчанию (дорогой скан кадров SCRFD/LVFace).
        casting: qget("casting", "0") == "1",
        // slug профиля app-библиотеки кастингов (#115): применить к этому ролику. Пусто = без применения.
        casting_ref: qget("casting_ref", ""),
        // тип контента кастинга (#115): "auto" (Gemma-детект) | "real" (SCRFD+LVFace) | "anime" (рисованные
        // лица + CCIP). Дефолт "auto" в паритет с UI (App.tsx) — при casting=off поле инертно.
        content_type: qget("content_type", "auto"),
        // «сабы уже на языке перевода» — эффективно только если сабы реально импортированы.
        import_translated: import_subs.is_some() && qget("import_translated", "0") == "1",
        // выровнять тайминги импортированных субтитров по речи (полный прогон ASR ради слов).
        align_subs: import_subs.is_some() && qget("align_subs", "0") == "1",
    };
    let post = post_analyze::PostAnalyze::from_query(&q)
        .map_err(|e| Box::new((StatusCode::BAD_REQUEST, e).into_response()))?;
    let voices_dir = st.voices_dir.clone();
    // Активный вариант модели резолвится ПРИ КАЖДОЙ джобе (не морозится на старте): скачал/выбрал
    // квант -> применяется без рестарта. См. models::resolve_*.
    let sel = models::load_selection(&st.models_root);
    // Backend локальных ONNX-стадий (диаризация Sortformer / Parakeet): exec_config читает DUB_ASR_BACKEND
    // при создании сессии — gpu регистрирует CUDA-EP с error_on_failure (недоступность = ошибка, не тихий
    // CPU-фоллбек), иначе CPU-провайдер. Задаём из local_backend на КАЖДУЮ джобу (переключение без рестарта).
    std::env::set_var("DUB_ASR_BACKEND", models::local_backend(&st.models_root));
    let (mt_model, mmproj) = models::resolve_mt(&st.models_root, &sel);
    let asr = models::resolve_asr_choice(&st.repo_root, &st.models_root, &sel);
    eprintln!("[models] analyze: MT={} · ASR={}", mt_model.display(), asr.describe());
    let paths = analyze::AnalyzePaths {
        input,
        work_dir: dir.clone(),
        repo_root: st.repo_root.clone(),
        asr,
        sortformer_onnx: st.sortformer_onnx.clone(),
        llama_bin: st.llama_bin.clone(),
        mt_model,
        mmproj,
        models_root: st.models_root.clone(),
        caption_fps: st.opts.caption_fps,
        import_subs,
        // Сепарация ДО диаризации/ASR (best practices): чистый вокал; stems-кэш общий с рендером.
        bsroformer_cli: dub_sep::engine_cli(&st.repo_root, models::stage_backend(&st.models_root, "sep_backend")),
        bsroformer_model: st.bsroformer_model.clone(),
    };

    // Тело джобы: analyze -> project.json (атомарно). Прогресс -> SSE.
    let dir_for_save = dir.clone();
    let pid_for_result = pid.to_string();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        clean_partials(&paths.work_dir);
        let cb = |ev: Value| progress(ev);
        // On-demand: если для этой функции (кастинг) или backend (GPU) не хватает моделей — тянем их СЕЙЧАС,
        // до анализа. Иначе кастинг «не видел» бы лиц, а GPU-стадии падали бы с ошибкой CUDA.
        ensure_job_components(&paths.repo_root, &paths.models_root, args.casting, analyze::wants_diarization(&args), &cb)?;
        jobs::check_cancelled()?;
        // Трекинг затрат OpenRouter в ДОЛЛАРАХ (перевод/vision через облако): total_usage до/после.
        let cost_before = openrouter::total_usage_usd(&paths.models_root);
        let mut proj = analyze::run(&args, &paths, &cb)?;
        if let (Some(b), Some(a)) = (cost_before, openrouter::total_usage_usd(&paths.models_root)) {
            let spent = (a - b).max(0.0);
            if spent > 0.0 {
                cb(json!({ "stage": "cost", "msg": format!("OpenRouter: потрачено ${spent:.4} за анализ (всего использовано ${a:.2})") }));
            }
        }
        let post_result = post.apply(&mut proj, &dir_for_save, &list_voice_names(&voices_dir))?;
        proj.glossary = glossary_api::after_analyze(&dir_for_save, &proj.glossary)?;
        save_project_atomic(&dir_for_save, &proj)?;
        Ok(json!({
            "project_id": pid_for_result,
            "output": dir_for_save.join("project.json").to_string_lossy(),
            "post": post_result,
        }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::Analyze, pid, dir, args_json), job)
        .await
        .map_err(|e| Box::new(enqueue_error(e)))
}

// ─── PATCH /projects/{pid} ──────────────────────────────────────────────────

async fn patch_project(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Json(edit): Json<Value>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let _held = project_writes();
    let mut proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let refused = |(code, msg): (u16, String)| (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), msg).into_response();
    // Правки дублей знают историю фразы на диске: take_select берёт из неё текст и ключ дубля.
    let take_op = edit.get("op").and_then(Value::as_str).is_some_and(takes::is_take_op);
    let edit = if take_op {
        match takes::resolve(&dir, &proj, &edit) {
            Ok(e) => e,
            Err(r) => return refused(r),
        }
    } else {
        edit
    };
    if let Err(r) = patch::apply(&mut proj, &edit) {
        return refused(r);
    }
    if let Err(e) = write_project(&dir, &proj) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    if take_op {
        if let Err(e) = takes::commit(&dir, &proj, &edit) {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
    }
    // Новый текст реплики снимает закрепление дубля с прежним текстом.
    if let (Some(id), Some(_)) = (edit.get("id").and_then(Value::as_str), edit.get("tgt_text").or(edit.get("take_text"))) {
        let seg = proj.segments.iter().find(|s| s.id == id);
        if let (Some(seg), Some(sid)) = (seg, render::seg_file_id(id)) {
            if let Err(e) = takes::unpin_if_stale(&dir, &sid, &seg.tgt_text) {
                return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
            }
        }
    }
    project_response(&st, &dir, &proj)
}

// ─── POST /projects/{pid}/render ────────────────────────────────────────────

/// Хвост рендера/озвучки: сбросить dirty у сегментов, чьи правки запечены, и перенести ключи синтеза из
/// seg_ckpt.json в Segment.ckpt. Запечены все правки при regen и реплики, переписанные циклом сокращения
/// (`shortened` — проект после него): второй проход озвучивает их и без regen. project.json перечитывается
/// с диска (а не пишется захваченный proj), чтобы не затереть правки, пришедшие во время джобы.
fn bake_render_state(
    start: &Project,
    shortened: Option<&Project>,
    proj_path: &Path,
    dir_for_job: &Path,
    regen: bool,
) -> Result<(), String> {
    fn texts(p: &Project) -> HashMap<&str, &str> {
        p.segments.iter().map(|s| (s.id.as_str(), s.tgt_text.as_str())).collect()
    }
    let before = texts(start);
    let after = shortened.map(texts);
    let baked = after.as_ref().unwrap_or(&before);
    let ckpts = render::SegCkpts::load(dir_for_job)?;
    let _held = project_writes();
    let t2 = std::fs::read_to_string(proj_path).map_err(|e| format!("чтение {}: {e}", proj_path.display()))?;
    let mut cur = Project::from_json(&t2).map_err(|e| format!("разбор {}: {e}", proj_path.display()))?;
    for s in &mut cur.segments {
        let id = s.id.as_str();
        let voiced = regen || baked.get(id) != before.get(id);
        if voiced && baked.get(id).copied() == Some(s.tgt_text.as_str()) {
            s.dirty = false;
        }
        if let Some(sid) = render::seg_file_id(&s.id) {
            match ckpts.get(&sid) {
                Some(k) if render::is_synth_key(k) => s.ckpt = Some(k.to_string()),
                Some(_) => s.ckpt = None,
                None => {}
            }
        }
    }
    write_project(dir_for_job, &cur)
}

#[cfg(test)]
mod bake_tests {
    use super::*;

    fn seg(id: &str, tgt: &str) -> dub_core::Segment {
        let mut s: dub_core::Segment = serde_json::from_value(json!({ "id": id, "start": 0.0, "end": 1.0, "src_text": "x", "tgt_text": tgt })).unwrap();
        s.dirty = false;
        s
    }

    fn load(d: &Path) -> Project {
        Project::from_json(&std::fs::read_to_string(d.join("project.json")).unwrap()).unwrap()
    }

    #[test]
    fn lines_the_render_loop_shortened_are_baked_without_regen() {
        let d = std::env::temp_dir().join(format!("dub_bake_{}_{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        let start = Project {
            segments: vec![seg("s1", "Нам прямо сейчас уже пора идти"), seg("s2", "Да"), seg("s3", "Эта фраза тоже длинная")],
            ..Default::default()
        };
        let mut after = start.clone();
        for (i, to) in [(0, "Нам пора"), (2, "Фраза длинная")] {
            let c = shorten::Change { id: after.segments[i].id.clone(), from: after.segments[i].tgt_text.clone(), to: to.into() };
            shorten::mark(&mut after.segments[i], &c);
        }
        let mut disk = after.clone();
        disk.segments[1].dirty = true;
        disk.segments[2].tgt_text = "Правка во время рендера".into();
        save_project_atomic(&d, &disk).unwrap();

        bake_render_state(&start, Some(&after), &d.join("project.json"), &d, false).unwrap();
        let back = load(&d);
        assert!(!back.segments[0].dirty, "the second pass voiced the shortened line");
        assert_eq!(back.segments[0].tgt_text, "Нам пора");
        assert!(back.segments[1].dirty, "an edit made during the job is not in this render");
        assert!(back.segments[2].dirty, "the line was edited after it was shortened");

        bake_render_state(&start, None, &d.join("project.json"), &d, false).unwrap();
        assert!(load(&d).segments[1].dirty, "without regen and without shortening nothing is baked");

        bake_render_state(&after, None, &d.join("project.json"), &d, true).unwrap();
        let back = load(&d);
        assert!(!back.segments[1].dirty && back.segments[2].dirty);
        std::fs::remove_dir_all(&d).unwrap();
    }
}

async fn render_project(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    match render_enqueue(&st, &pid).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(resp) => *resp,
    }
}

/// RenderPaths для проекта: активный вариант модели резолвится ПРИ КАЖДОЙ джобе (см. models::resolve_*),
/// скачал/выбрал квант в настройках -> применяется без рестарта сервера.
fn render_paths(st: &AppState, dir: &Path, input: PathBuf) -> render::RenderPaths {
    let sel = models::load_selection(&st.models_root);
    let (higgs_model_root, higgs_quant) = models::resolve_tts(&st.models_root, &sel);
    let sep_model = models::resolve_sep(&st.models_root, &sel);
    eprintln!("[models] render: TTS={} (q={}) · SEP={}", higgs_model_root.display(), higgs_quant, sep_model.display());
    render::RenderPaths {
        input,
        bench: models::bench_enabled(&st.models_root),
        work_dir: dir.to_path_buf(),
        output: dir.join("output.mp4"),
        bsroformer_cli: dub_sep::engine_cli(&st.repo_root, models::stage_backend(&st.models_root, "sep_backend")),
        bsroformer_model: sep_model,
        higgs_dll: st.higgs_dll.clone(),
        higgs_model_root,
        higgs_quant,
        fonts_dir: st.fonts_dir.clone(),
        higgs_backend: "cuda".to_string(),
        higgs_device: 0,
        higgs_threads: st.opts.num_threads,
        max_stretch: st.opts.max_stretch as f64,
        voices_dir: st.voices_dir.clone(),
        asr: models::resolve_asr_choice(&st.repo_root, &st.models_root, &sel),
        ref_secs: models::higgs_ref_secs(&st.models_root),
        models_root: st.models_root.clone(),
        llama_bin: st.llama_bin.clone(),
        mt_model: models::resolve_mt(&st.models_root, &sel).0,
    }
}

/// Исходное видео проекта из source.txt; нет файла -> 409.
fn project_input(dir: &Path) -> Result<PathBuf, Box<Response>> {
    match std::fs::read_to_string(dir.join("source.txt")) {
        Ok(s) => Ok(PathBuf::from(s.trim())),
        Err(_) => Err(Box::new((StatusCode::CONFLICT, "no source uploaded").into_response())),
    }
}

async fn render_enqueue(st: &AppState, pid: &str) -> Result<String, Box<Response>> {
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    let proj_path = dir.join("project.json");
    if !proj_path.is_file() {
        return Err(Box::new((StatusCode::CONFLICT, "project not analyzed yet").into_response()));
    }
    let input = project_input(&dir)?;
    let paths = render_paths(st, &dir, input);
    let dir_for_job = dir.clone();
    let out_for_result = paths.output.clone();
    let repo_root_for_job = st.repo_root.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        clean_partials(&dir_for_job);
        let cb = |ev: Value| progress(ev);
        // On-demand: GPU-стадии рендера (Higgs TTS / сепарация на CUDA) без CUDA-стека -> догрузить сейчас.
        ensure_job_components(&repo_root_for_job, &paths.models_root, false, false, &cb)?;
        jobs::check_cancelled()?;
        // Загрузить свежий Project (правки могли прийти после enqueue).
        let text = std::fs::read_to_string(&proj_path).map_err(|e| e.to_string())?;
        let proj = Project::from_json(&text).map_err(|e| e.to_string())?;
        // regen_dub если есть dirty-сегменты (voice/text/rewrite правились).
        let regen = proj.segments.iter().any(|s| s.dirty);
        // Трекинг затрат OpenRouter в ДОЛЛАРАХ: total_usage до/после (None -> облако не использовалось).
        let cost_before = openrouter::total_usage_usd(&paths.models_root);
        let done = render::run(&proj, &paths, regen, &cb)?;
        if let (Some(b), Some(a)) = (cost_before, openrouter::total_usage_usd(&paths.models_root)) {
            let spent = (a - b).max(0.0);
            if spent > 0.0 {
                cb(json!({ "stage": "cost", "msg": format!("OpenRouter: потрачено ${spent:.4} за прогон (всего использовано ${a:.2})") }));
            }
        }
        bake_render_state(&proj, done.project.as_ref(), &proj_path, &dir_for_job, regen)?;
        Ok(json!({ "output": out_for_result.to_string_lossy() }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::Render, pid, dir, json!({})), job)
        .await
        .map_err(|e| Box::new(enqueue_error(e)))
}

/// Клонировать каталог проекта для ре-перевода на другой язык: копируем ВСЁ, кроме пожадорных/финальных
/// выводов (они перегенерируются рендером). Сохраняем project.json (раскладка/стили/блюр/титры), исходник,
/// референсы клона голоса (ref_*.wav) и разделённые вокал/аудио-кэши — чтобы дубляж взял ТОТ ЖЕ голос,
/// а раскладка субтитров осталась как у пользователя. НЕ гоняем заново ASR/OCR/vision.
fn clone_project_for_relang(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let rd = std::fs::read_dir(src).map_err(|e| e.to_string())?;
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue; // подкаталоги (frames/) не нужны — vision/OCR не перезапускаем
        }
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let skip = name.starts_with("seg_")
            || name == job_store::FILE
            || dub_core::atomic::is_tmp(&p)
            || name.starts_with("output.")
            || name == "captioned.mp4"
            || name.starts_with("_preview")
            || name == "_original.png"
            || name == "dub_audio.m4a"
            || name == "dub_fit.wav"
            || name == "dub_timing.json"
            || name == "dub_words.json"
            || ["new_audio", "final_audio", "gained_audio", "orig_ducked"]
                .iter()
                .any(|stem| name == format!("{stem}.wav") || name == format!("{stem}.m4a"));
        if !skip {
            std::fs::copy(&p, dst.join(&name)).map_err(|e| format!("copy {name}: {e}"))?;
        }
    }
    Ok(())
}

// ─── POST /projects/{pid}/export-lang?lang=Lx ───────────────────────────────
// Экспорт-уровень мультиязыка: КЛОНИРУЕТ уже отредактированный проект (сохраняя раскладку субтитров,
// стили, блюр, титры и клон голоса) в НОВЫЙ проект на языке Lx, переводит ТОЛЬКО текст (сегменты+титры)
// через Gemma (flat_run — без пере-раскладки vision), помечает dirty и рендерит. Возвращает {project_id, job_id}.
async fn export_lang(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let lang = q.get("lang").cloned().unwrap_or_default().trim().to_string();
    if lang.is_empty() {
        return (StatusCode::BAD_REQUEST, "no lang").into_response();
    }
    let src_dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if !src_dir.join("project.json").is_file() {
        return (StatusCode::CONFLICT, "project not analyzed yet").into_response();
    }
    let mut new_pid = uuid::Uuid::new_v4().simple().to_string();
    new_pid.truncate(12);
    let dst_dir = st.workspace.join(&new_pid);
    if let Err(e) = clone_project_for_relang(&src_dir, &dst_dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    let args = json!({ "lang": lang, "src_pid": pid, "translated": false });
    match export_lang_enqueue(&st, &new_pid, &args).await {
        Ok(job_id) => Json(json!({ "job_id": job_id, "project_id": new_pid })).into_response(),
        Err(resp) => {
            if let Err(e) = std::fs::remove_dir_all(&dst_dir) {
                eprintln!("[export-lang] не удалён клон {}: {e}", dst_dir.display());
            }
            *resp
        }
    }
}

/// Перевод+рендер клона `pid` на язык args.lang. args.translated=true (перевод уже сохранён прошлым
/// прогоном) -> только рендер: «Продолжить» после сбоя TTS не платит за перевод повторно.
async fn export_lang_enqueue(st: &AppState, pid: &str, args: &Value) -> Result<String, Box<Response>> {
    let dst_dir = st.proj_dir(pid).map_err(Box::new)?;
    let lang = args.get("lang").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if lang.is_empty() {
        return Err(Box::new((StatusCode::BAD_REQUEST, "no lang").into_response()));
    }
    let translated = args.get("translated").and_then(Value::as_bool).unwrap_or(false);
    let input = match std::fs::read_to_string(dst_dir.join("source.txt")) {
        Ok(s) => PathBuf::from(s.trim()),
        Err(_) => return Err(Box::new((StatusCode::CONFLICT, "no source").into_response())),
    };

    let llama_bin = st.llama_bin.clone();
    // Квант Gemma — выбранный сейчас (models::resolve_mt), а не найденный при старте сервера.
    let (mt_model, _) = models::resolve_mt(&st.models_root, &models::load_selection(&st.models_root));
    let models_root_xl = st.models_root.clone();
    let paths = render_paths(st, &dst_dir, input);
    let dst_for_job = dst_dir.clone();
    let lang_c = lang.clone();
    let new_pid_res = pid.to_string();
    let out_res = paths.output.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        use dub_translate::{flat_run_with, FlatOpts, Seg};
        clean_partials(&dst_for_job);
        let pj = dst_for_job.join("project.json");
        let text = std::fs::read_to_string(&pj).map_err(|e| e.to_string())?;
        let mut p = Project::from_json(&text).map_err(|e| e.to_string())?;
        if !translated {
            p.tgt_lang = lang_c.clone();
            let spoken = matches!(p.mode.as_str(), "dub" | "voiceover");
            progress(json!({ "type": "progress", "stage": "translate",
                "msg": format!("Перевод {} строк → {}", p.segments.len(), lang_c) }));
            // LLM-провайдер перевода: своя Gemma (плоский MT), локальный сервер или OpenRouter.
            let prov = crate::llm_provider::open(
                &crate::llm_provider::LlmOpen {
                    llama_bin: &llama_bin,
                    mt_model: &mt_model,
                    mmproj: std::path::Path::new(""),
                    models_root: &models_root_xl,
                },
                crate::llm_provider::LlmMode::Text,
            )
            .map_err(|e| format!("перевод: LLM недоступен — {e}"))?;
            let client = prov.client();
            // Сегменты: src_text -> Lx (раскладка/стили/тайминг остаются от пользователя).
            let mut segs: Vec<Seg> = p
                .segments
                .iter()
                .map(|s| {
                    let spk = crate::analyze::speaker_to_i64(s.speaker.as_deref());
                    // Вручную добавленные фразы имеют пустой src_text — их перевод берём из текущего tgt_text
                    // (он на СТАРОМ целевом языке), иначе flat_run их пропустит и они останутся на старом языке.
                    let src = if s.src_text.trim().is_empty() { s.tgt_text.clone() } else { s.src_text.clone() };
                    Seg::new(src, spk)
                })
                .collect();
            let contract = dub_translate::Contract::for_client(client);
            let opts = FlatOpts { src: "auto", tgt: &lang_c, spoken, style: &p.audio.translate_style, glossary: &p.glossary, contract: &contract };
            flat_run_with(client, &mut segs, &opts, &mut |m: &str| progress(json!({ "type": "progress", "stage": "translate", "msg": m.trim() })))
                .map_err(|e| format!("translate: {e}"))?;
            let glossary = dub_core::glossary::for_translation(&p.glossary, &lang_c);
            if let Some(note) = crate::translate::untranslated_note(segs.iter().map(|sg| (sg.text.as_str(), sg.tgt.as_str())), &lang_c, &glossary) {
                tracing::warn!("export_lang -> {lang_c}: {note}");
                progress(json!({ "type": "progress", "stage": "translate", "msg": format!("перевод: {note}") }));
            }
            p.glossary_fp = glossary_api::fingerprint(&p.glossary, &lang_c);
            for (s, sg) in p.segments.iter_mut().zip(segs) {
                if !sg.tgt.trim().is_empty() {
                    s.tgt_text = sg.tgt;
                }
                s.dirty = true; // форсим ре-TTS на новом языке
            }
            // Титры: text -> Lx (позиции/стиль остаются). При сбое перевода НЕ оставляем старый целевой язык:
            // очищаем tgt -> рендер покажет исходный text (лучше, чем титр на старом языке рядом с новыми сегментами).
            if !p.captions.titles.is_empty() {
                let mut tsegs: Vec<Seg> = p.captions.titles.iter().map(|ti| Seg::new(ti.text.clone(), 0)).collect();
                let topts = FlatOpts { src: "auto", tgt: &lang_c, spoken: false, style: &p.audio.translate_style, glossary: &[], contract: &contract };
                let done = flat_run_with(client, &mut tsegs, &topts, &mut |m: &str| progress(json!({ "type": "progress", "stage": "translate", "msg": m.trim() })));
                if let Err(e) = &done {
                    tracing::warn!("export_lang {lang_c}: титры не переведены: {e}");
                    progress(json!({ "type": "progress", "stage": "translate",
                        "msg": format!("титры не переведены ({e}) — в видео они останутся на исходном языке") }));
                }
                let ok = done.is_ok();
                for (ti, sg) in p.captions.titles.iter_mut().zip(tsegs) {
                    ti.tgt = if ok && !sg.tgt.trim().is_empty() { sg.tgt } else { String::new() };
                }
            }
            drop(prov); // освободить VRAM перед TTS/рендером (облако — no-op)
            if let Some(now) = glossary_api::on_disk(&dst_for_job)? {
                p.glossary = now;
            }
            save_project_atomic(&dst_for_job, &p)?;
            jobs::update_record(|r| r.args["translated"] = json!(true))?;
        }
        jobs::check_cancelled()?;
        let cb = |ev: Value| progress(ev);
        let regen = p.segments.iter().any(|s| s.dirty);
        let done = render::run(&p, &paths, regen, &cb)?;
        bake_render_state(&p, done.project.as_ref(), &pj, &dst_for_job, regen)?;
        Ok(json!({ "output": out_res.to_string_lossy(), "project_id": new_pid_res }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::ExportLang, pid, dst_dir, args.clone()), job)
        .await
        .map_err(|e| Box::new(enqueue_error(e)))
}

// ─── POST /projects/{pid}/retranslate?lang=Lx&mode=<dub|voiceover|nodub> ─────
/// Смена режима из «Транскрипта» БЕЗ повторного распознавания (реквест Стаса #122). Транскрипт уже дал
/// сегменты (src_text + спикеры + тайминги); переход в дубляж/субтитры/закадр требует лишь ПЕРЕВОДА этих
/// сегментов на tgt (озвучка/сборка — на рендере, как обычно). Раньше switchMode звал полный analyze ->
/// заново гонялись сепарация/диаризация/ASR, хотя текст только что распознан. Здесь — только flat_run-перевод
/// готовых сегментов IN-PLACE (тот же pid, без клона и без рендера) + смена p.mode. Переиспользует ту же
/// машинерию, что export_lang, но не пересобирает видео. Диаризация/ASR НЕ трогаются.
async fn retranslate_project(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let args = json!({
        "lang": q.get("lang").cloned().unwrap_or_default(),
        "mode": q.get("mode").cloned().unwrap_or_default(),
    });
    match retranslate_enqueue(&st, &pid, &args).await {
        Ok(job_id) => Json(json!({ "job_id": job_id, "project_id": pid })).into_response(),
        Err(resp) => *resp,
    }
}

async fn retranslate_enqueue(st: &AppState, pid: &str, args: &Value) -> Result<String, Box<Response>> {
    let lang = args.get("lang").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if lang.is_empty() {
        return Err(Box::new((StatusCode::BAD_REQUEST, "no lang").into_response()));
    }
    // Режим-мишень: dub|voiceover|nodub (субтитры). Прочее -> дубляж (дефолт), как маппинг во фронте.
    let mode = match args.get("mode").and_then(Value::as_str) {
        Some("voiceover") => "voiceover",
        Some("nodub") => "nodub",
        _ => "dub",
    }
    .to_string();
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    if !dir.join("project.json").is_file() {
        return Err(Box::new((StatusCode::CONFLICT, "project not analyzed yet").into_response()));
    }

    let llama_bin = st.llama_bin.clone();
    // Квант Gemma — выбранный сейчас (models::resolve_mt), а не найденный при старте сервера.
    let (mt_model, _) = models::resolve_mt(&st.models_root, &models::load_selection(&st.models_root));
    let models_root_xl = st.models_root.clone();
    let dir_for_job = dir.clone();
    let pid_res = pid.to_string();
    let lang_c = lang.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        use dub_translate::{flat_run_with, FlatOpts, Seg};
        let pj = dir_for_job.join("project.json");
        let read = |path: &Path| -> Result<Project, String> {
            let text = std::fs::read_to_string(path).map_err(|e| format!("чтение {}: {e}", path.display()))?;
            Project::from_json(&text).map_err(|e| format!("разбор {}: {e}", path.display()))
        };
        let p = read(&pj)?;
        let spoken = matches!(mode.as_str(), "dub" | "voiceover");
        progress(json!({ "type": "progress", "stage": "translate",
            "msg": format!("Перевод {} строк → {}", p.segments.len(), lang_c) }));
        let prov = crate::llm_provider::open(
            &crate::llm_provider::LlmOpen {
                llama_bin: &llama_bin,
                mt_model: &mt_model,
                mmproj: std::path::Path::new(""),
                models_root: &models_root_xl,
            },
            crate::llm_provider::LlmMode::Text,
        )
        .map_err(|e| format!("перевод не выполнен: LLM недоступен: {e}"))?;
        let client = prov.client();
        // src_text -> Lx (тайминги/спикеры/раскладка остаются от транскрипта). Вручную добавленные фразы
        // (пустой src_text) переводим из текущего tgt_text (как в export_lang).
        let input = |s: &dub_core::Segment| -> (String, String) {
            let text = if s.src_text.trim().is_empty() { &s.tgt_text } else { &s.src_text };
            (s.id.clone(), text.clone())
        };
        let inputs: Vec<(String, String)> = p.segments.iter().map(input).collect();
        let mut segs: Vec<Seg> = p
            .segments
            .iter()
            .zip(&inputs)
            .map(|(s, (_, text))| Seg::new(text.clone(), crate::analyze::speaker_to_i64(s.speaker.as_deref())))
            .collect();
        let contract = dub_translate::Contract::for_client(client);
        let opts = FlatOpts { src: "auto", tgt: &lang_c, spoken, style: &p.audio.translate_style, glossary: &p.glossary, contract: &contract };
        flat_run_with(client, &mut segs, &opts, &mut |m: &str| progress(json!({ "type": "progress", "stage": "translate", "msg": m.trim() })))
            .map_err(|e| format!("translate: {e}"))?;
        let glossary = dub_core::glossary::for_translation(&p.glossary, &lang_c);
        if let Some(note) = crate::translate::untranslated_note(segs.iter().map(|sg| (sg.text.as_str(), sg.tgt.as_str())), &lang_c, &glossary) {
            tracing::warn!("retranslate {pid_res} -> {lang_c}: {note}");
            progress(json!({ "type": "progress", "stage": "translate", "msg": format!("перевод: {note}") }));
        }
        // Титры: text -> Lx (позиции/стиль остаются).
        let titles: Vec<String> = p.captions.titles.iter().map(|ti| ti.text.clone()).collect();
        let mut title_tgts: Vec<String> = Vec::new();
        if !titles.is_empty() {
            let mut tsegs: Vec<Seg> = titles.iter().map(|text| Seg::new(text.clone(), 0)).collect();
            let topts = FlatOpts { src: "auto", tgt: &lang_c, spoken: false, style: &p.audio.translate_style, glossary: &[], contract: &contract };
            flat_run_with(client, &mut tsegs, &topts, &mut |m: &str| progress(json!({ "type": "progress", "stage": "translate", "msg": m.trim() })))
                .map_err(|e| format!("перевод титров: {e}"))?;
            title_tgts = tsegs.into_iter().map(|sg| sg.tgt).collect();
        }
        drop(prov);
        // Перевод шёл минутами: правки, сделанные за это время, остаются, а перевод ложится, только пока
        // реплики и титры те же, что переводились.
        let _held = project_writes();
        let mut fresh = read(&pj)?;
        let now: Vec<(String, String)> = fresh.segments.iter().map(input).collect();
        let now_titles: Vec<&str> = fresh.captions.titles.iter().map(|ti| ti.text.as_str()).collect();
        if now != inputs || now_titles != titles.iter().map(String::as_str).collect::<Vec<_>>() {
            return Err("перевод: реплики или титры изменились, пока шёл перевод — проект не менялся, запустите перевод ещё раз".into());
        }
        fresh.tgt_lang = lang_c.clone();
        fresh.mode = mode.clone();
        // Закадр/субтитры не переписывают текст «смешно» — сбрасываем rewrite, чтобы derived-режим во фронте
        // не показал «funny» после перехода в dub/nodub/voiceover из транскрипта.
        fresh.audio.rewrite = None;
        fresh.glossary_fp = glossary_api::fingerprint(&p.glossary, &lang_c);
        for (s, sg) in fresh.segments.iter_mut().zip(segs) {
            if !sg.tgt.trim().is_empty() {
                s.tgt_text = sg.tgt;
            }
            s.dirty = true; // новый язык -> ре-TTS при рендере/озвучке
        }
        for (ti, tgt) in fresh.captions.titles.iter_mut().zip(title_tgts) {
            ti.tgt = if tgt.trim().is_empty() { String::new() } else { tgt };
        }
        write_project(&dir_for_job, &fresh)?;
        Ok(json!({ "project_id": pid_res, "ok": true }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::Retranslate, pid, dir, args.clone()), job)
        .await
        .map_err(|e| Box::new(enqueue_error(e)))
}

/// POST /projects/{pid}/dub-audio — сгенерить ТОЛЬКО озвучку (без сборки видео) -> dub_audio.m4a.
/// Фронт вызывает сразу после анализа, чтобы дуб можно было слушать в редакторе (плей играет /dub),
/// не дожидаясь и не собирая финальное видео. Тот же job-паттерн, что и render_project.
async fn dub_audio_project(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    match dub_audio_enqueue(&st, &pid).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(resp) => *resp,
    }
}

async fn dub_audio_enqueue(st: &AppState, pid: &str) -> Result<String, Box<Response>> {
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    let proj_path = dir.join("project.json");
    if !proj_path.is_file() {
        return Err(Box::new((StatusCode::CONFLICT, "project not analyzed yet").into_response()));
    }
    let input = project_input(&dir)?;
    let paths = render_paths(st, &dir, input);
    let dir_for_job = dir.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        clean_partials(&dir_for_job);
        let cb = |ev: Value| progress(ev);
        let text = std::fs::read_to_string(&proj_path).map_err(|e| e.to_string())?;
        let proj = Project::from_json(&text).map_err(|e| e.to_string())?;
        let regen = proj.segments.iter().any(|s| s.dirty);
        let (out, shortened) = render::dub_audio(&proj, &paths, regen, &cb)?;
        // Правки запечены в озвучку (seg_XXX.wav) -> сбросить dirty, как делает render_project. Иначе
        // последующий Экспорт (render видит dirty) РЕ-РОЛЛИТ уже одобренный дубляж — регресс «скидывается».
        bake_render_state(&proj, shortened.as_ref(), &proj_path, &dir_for_job, regen)?;
        Ok(json!({ "audio": out.to_string_lossy() }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::DubAudio, pid, dir, json!({})), job)
        .await
        .map_err(|e| Box::new(enqueue_error(e)))
}

// ─── GET /projects/{pid}/output ; /original ; /dub (Range-раздача файла) ─────

/// Найти готовый выходной файл проекта: output.mp4 (обычный/mp4-мультитрек) -> output.mkv (mkv-мультитрек,
/// #113) -> output.wav (аудио-режим). Единый порядок фолбэков для раздачи/сохранения/открытия/превью.
fn find_output(dir: &std::path::Path) -> std::path::PathBuf {
    for name in ["output.mp4", "output.mkv", "output.wav"] {
        let p = dir.join(name);
        if p.is_file() {
            return p;
        }
    }
    dir.join("output.mp4")
}

/// Выход ДЛЯ СОХРАНЕНИЯ юзеру: mkv приоритетнее mp4 (юзер выбрал mkv ради мультитрека; mp4 — лишь
/// playable-компаньон для встроенного плеера, #116). Порядок mkv->mp4->wav.
fn find_output_save(dir: &std::path::Path) -> std::path::PathBuf {
    for name in ["output.mkv", "output.mp4", "output.wav"] {
        let p = dir.join(name);
        if p.is_file() {
            return p;
        }
    }
    dir.join("output.mp4")
}

async fn output(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    // output.mp4/.mkv (видео; mkv — экспорт с оригинальной дорожкой) или output.wav (аудио-режим).
    let f = find_output(&dir);
    if !f.is_file() {
        return (StatusCode::NOT_FOUND, "not rendered").into_response();
    }
    let dl = q.get("dl").map(|v| v == "1").unwrap_or(false);
    let ext = f.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let filename = if dl { Some(format!("{pid}_dub.{ext}")) } else { None };
    serve_file_range(&f, req, filename).await
}

/// POST /pick-folder — нативный диалог выбора папки (rfd). Возвращает {dir} или {dir:null} при отмене.
/// Для batch-экспорта: юзер выбирает ОДНУ папку назначения, дальше save-output кладёт туда все файлы.
async fn pick_folder() -> Json<Value> {
    let dir = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("Куда сохранить результаты").pick_folder()
    })
    .await
    .ok()
    .flatten();
    Json(json!({ "dir": dir.map(|d| d.to_string_lossy().into_owned()) }))
}

/// POST /projects/{pid}/save-output {dir, name} — скопировать готовый output проекта в папку `dir` под
/// именем `name` (оригинальное имя файла), сохранив расширение реального выхода. Для batch-вывода:
/// все переведённые файлы в ОДНУ папку с исходными именами.
async fn save_output(State(st): State<AppState>, AxPath(pid): AxPath<String>, Json(body): Json<Value>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let src = find_output_save(&dir); // сохраняем богатый файл: mkv приоритетнее playable-mp4 (#116)
    if !src.is_file() {
        return (StatusCode::NOT_FOUND, "not rendered").into_response();
    }
    let Some(dest_dir) = body.get("dir").and_then(|v| v.as_str()) else {
        return (StatusCode::BAD_REQUEST, "no dir").into_response();
    };
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("output");
    // <исходный stem>.<расширение реального выхода> — имя как у оригинала, контейнер как у результата.
    let stem = std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let out_ext = src.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let base = std::path::Path::new(dest_dir);
    let mut dest = base.join(format!("{stem}.{out_ext}"));
    // Коллизия имён: два входа с одинаковым basename (напр. разные папки, оба intro.mp4) при batch
    // «Сохранить все в папку» затёрли бы друг друга → добавляем суффикс (2),(3)… (ревью-находка H).
    let mut n = 2u32;
    while dest.exists() {
        dest = base.join(format!("{stem} ({n}).{out_ext}"));
        n += 1;
    }
    match std::fs::copy(&src, &dest) {
        Ok(_) => (
            [("content-type", "application/json")],
            format!("{{\"ok\":true,\"path\":{:?}}}", dest.to_string_lossy()),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("копирование в {}: {e}", dest.display()),
        )
            .into_response(),
    }
}

/// POST /projects/{pid}/open — открыть готовый output.mp4 в СИСТЕМНОМ плеере (на машине пользователя).
/// Нужно, т.к. в нативном Tauri-webview <a target="_blank"> внешний плеер не открывает.
async fn open_output(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let f = find_output_save(&dir); // системный плеер (VLC и т.п.) тянет богатый mkv, если он есть
    if !f.is_file() {
        return (StatusCode::NOT_FOUND, "not rendered").into_response();
    }
    let path = f.to_string_lossy().to_string();
    let _ = tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let _ = process_group::detach_from_group(&mut std::process::Command::new("cmd"))
                .args(["/C", "start", "", &path])
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
        }
    })
    .await;
    Json(json!({ "ok": true })).into_response()
}

/// Открыть проводник/файловый менеджер с ВЫДЕЛЕННЫМ файлом (не плеер). Windows: explorer /select.
fn reveal_in_explorer(path: String) {
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            // explorer /select,"<path>" — выделяет файл в открытом каталоге. Один аргумент.
            let _ = process_group::detach_from_group(&mut std::process::Command::new("explorer"))
                .arg(format!("/select,{path}"))
                .spawn();
        }
        #[cfg(not(windows))]
        {
            // прочие ОС: открыть родительский каталог (выделение файла непортабельно).
            let parent = std::path::Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    });
}

/// POST /projects/{pid}/reveal — показать файл (по имени в каталоге проекта) в проводнике с выделением.
/// Body {name}. Для кнопок сохранения/экспорта: пользователь видит, КУДА сохранилось.
async fn reveal_file(State(st): State<AppState>, AxPath(pid): AxPath<String>, Json(body): Json<Value>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("output.mp4");
    let safe: String = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-')).collect();
    let mut f = dir.join(&safe);
    // Запрошенного имени нет (фронт просил output.mkv, а mux откатился на mp4, #116) — резолвим фактический.
    if !f.is_file() && safe.starts_with("output.") {
        f = find_output_save(&dir);
    }
    if !f.is_file() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    reveal_in_explorer(f.to_string_lossy().to_string());
    Json(json!({ "ok": true })).into_response()
}

/// POST /projects/{pid}/save-text — записать текстовый файл (SRT/TXT) в каталог проекта и показать его в
/// проводнике. В нативном Tauri-webview браузерный blob-download (<a download>) не работает — сохраняем
/// через бэкенд. Body {name, text}.
async fn save_text(State(st): State<AppState>, AxPath(pid): AxPath<String>, Json(body): Json<Value>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("transcript.txt");
    let safe: String = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-')).collect();
    let safe = if safe.is_empty() { "transcript.txt".to_string() } else { safe };
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let f = dir.join(&safe);
    if let Err(e) = std::fs::write(&f, text) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}")).into_response();
    }
    reveal_in_explorer(f.to_string_lossy().to_string());
    Json(json!({ "ok": true, "path": f.to_string_lossy() })).into_response()
}

async fn dub_video(
    State(st): State<AppState>,
    AxPath(pid): AxPath<String>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    // Что играет <audio>: свежайший дубляж. И output.mp4 (после Экспорта), и dub_audio.m4a (после
    // «перегенерировать озвучку»/regen) несут актуальную дорожку — но regen обновляет ТОЛЬКО dub_audio.m4a,
    // не output.mp4. Прежний жёсткий приоритет output.mp4 играл в превью УСТАРЕВШИЙ дубляж после regen
    // (юзеры: «перегенерировать не работает, новое слышно только после Экспорта») -> берём НОВЕЙШИЙ по mtime.
    let output = find_output(&dir); // output.mp4/.mkv (видео) или output.wav (аудио-режим)
    let dub_audio = dir.join("dub_audio.m4a");
    let mtime = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut f = match (output.is_file(), dub_audio.is_file()) {
        (true, true) => {
            if mtime(&dub_audio) > mtime(&output) { dub_audio } else { output }
        }
        (true, false) => output,
        (false, true) => dub_audio,
        (false, false) => dir.join("analyzed.mp4"),
    };
    if !f.is_file() {
        // nodub / субтитры / транскрипт: озвучки нет — играем ОРИГИНАЛЬНУЮ дорожку видео (source.*),
        // чтобы плей в редакторе работал (переиспользуем аудиодорожку исходника, <audio> берёт её из mp4).
        // Путь — из source.txt (как в analyze/render); fallback — source.* в каталоге.
        if let Ok(p) = std::fs::read_to_string(dir.join("source.txt")) {
            let sp = std::path::PathBuf::from(p.trim());
            if sp.is_file() {
                f = sp;
            }
        }
        if !f.is_file() {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.file_stem().and_then(|s| s.to_str()) == Some("source") && p.is_file() {
                        f = p;
                        break;
                    }
                }
            }
        }
    }
    if !f.is_file() {
        return (StatusCode::NOT_FOUND, "no dubbed audio yet").into_response();
    }
    serve_file_range(&f, req, None).await
}

/// Отдать файл с поддержкой Range (для <video> seek). Используем tower-http ServeFile — он
/// корректно обрабатывает Range/If-Range/Content-Range. dl -> Content-Disposition attachment.
async fn serve_file_range(
    path: &Path,
    req: axum::http::Request<axum::body::Body>,
    download_name: Option<String>,
) -> Response {
    use tower::ServiceExt;
    use tower_http::services::ServeFile;
    let svc = ServeFile::new(path);
    match svc.oneshot(req).await {
        Ok(mut resp) => {
            if let Some(name) = download_name {
                if let Ok(v) = axum::http::HeaderValue::from_str(&format!(
                    "attachment; filename=\"{name}\""
                )) {
                    resp.headers_mut().insert(axum::http::header::CONTENT_DISPOSITION, v);
                }
            }
            resp.map(axum::body::Body::new)
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ─── GET /jobs/{job_id}/events (SSE) ────────────────────────────────────────

async fn job_events(
    State(st): State<AppState>,
    AxPath(job_id): AxPath<String>,
) -> Response {
    let Some(sub) = st.jobs.subscribe(&job_id).await else {
        return (StatusCode::NOT_FOUND, "job not found").into_response();
    };
    Sse::new(sse_stream(sub))
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

// ─── GET /jobs?pid= ; GET /jobs/{id}?wait= ; POST /jobs/{id}/cancel ─────────

/// Активные и недавние джобы (без кадр-джоб). С `pid` — только этого проекта плюс его job.json
/// (последняя джоба проекта, в т.ч. прерванная до рестарта: её можно продолжить).
async fn jobs_list(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let pid = q.get("pid").map(String::as_str).filter(|s| !s.is_empty());
    let project_job = match pid {
        Some(pid) => {
            let dir = match st.proj_dir(pid) {
                Ok(d) => d,
                Err(resp) => return resp,
            };
            match job_store::read(&dir) {
                Ok(rec) => serde_json::to_value(rec).unwrap_or(Value::Null),
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
            }
        }
        None => Value::Null,
    };
    Json(json!({ "jobs": st.jobs.list(pid).await, "project_job": project_job })).into_response()
}

/// Снапшот джобы {id, kind, pid, status, stage, msg, pct, position?, result|error}. `wait=N` (сек,
/// до jobs::WAIT_LONGEST_SECS) — long-poll: ответ приходит при завершении или по истечении N.
async fn job_get(
    State(st): State<AppState>,
    AxPath(job_id): AxPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let wait: u64 = match q.get("wait").map(|v| v.parse::<u64>()) {
        None => 0,
        Some(Ok(v)) => v,
        Some(Err(_)) => return (StatusCode::BAD_REQUEST, "wait: ожидалось число секунд").into_response(),
    };
    let snap = if wait > 0 { st.jobs.wait(&job_id, wait).await } else { st.jobs.snapshot(&job_id).await };
    match snap {
        Some(v) => Json(v).into_response(),
        None => (StatusCode::NOT_FOUND, "job not found").into_response(),
    }
}

async fn job_cancel(State(st): State<AppState>, AxPath(job_id): AxPath<String>) -> Response {
    match st.jobs.cancel(&job_id).await {
        Ok(v) => Json(v).into_response(),
        Err(jobs::CancelError::NotFound) => (StatusCode::NOT_FOUND, "job not found").into_response(),
        Err(jobs::CancelError::Finished) => {
            (StatusCode::CONFLICT, Json(json!({ "error": "job_finished", "job_id": job_id }))).into_response()
        }
    }
}

// ─── POST /projects/{pid}/resume ────────────────────────────────────────────
/// Продолжить последнюю незавершённую джобу проекта (job.json): тот же вид с теми же аргументами на
/// ТОМ ЖЕ проекте — готовые стадии анализа и уже озвученные сегменты берутся из кэша.
async fn resume_project(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    let rec = match job_store::read(&dir) {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "no job to resume").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    if let Some((job_id, kind)) = st.jobs.active_for(&pid).await {
        return enqueue_error(jobs::EnqueueError::Conflict { job_id, kind });
    }
    let unfinished = rec.state == job_store::STATE_QUEUED || rec.state == job_store::STATE_RUNNING;
    if !rec.resumable() && !unfinished {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "nothing_to_resume", "state": rec.state })),
        )
            .into_response();
    }
    let Some(kind) = jobs::JobKind::parse(&rec.kind) else {
        return (StatusCode::CONFLICT, format!("неизвестный вид джобы в job.json: {}", rec.kind)).into_response();
    };
    let started = match kind {
        jobs::JobKind::Analyze => analyze_enqueue(&st, &pid, rec.args.clone()).await,
        jobs::JobKind::Render => render_enqueue(&st, &pid).await,
        jobs::JobKind::DubAudio => dub_audio_enqueue(&st, &pid).await,
        jobs::JobKind::Retranslate => retranslate_enqueue(&st, &pid, &rec.args).await,
        jobs::JobKind::Remix => endpoints::remix_enqueue(&st, &pid, &rec.args).await,
        jobs::JobKind::ExportLang => export_lang_enqueue(&st, &pid, &rec.args).await,
        jobs::JobKind::Align => align_enqueue(&st, &pid).await,
        jobs::JobKind::Shorten => shorten::shorten_enqueue(&st, &pid, &rec.args).await,
        other => {
            return (StatusCode::CONFLICT, format!("джоба {} не продолжается", other.as_str())).into_response();
        }
    };
    match started {
        Ok(job_id) => Json(json!({ "job_id": job_id, "kind": kind.as_str(), "project_id": pid })).into_response(),
        Err(resp) => *resp,
    }
}

/// Поток SSE-событий джобы: сначала терминал (если уже завершена) или последнее известное событие,
/// затем живые события до done/error/cancelled. Джоба из истории не удаляется — её может прочитать
/// следующий подписчик или GET /jobs/{id}.
fn sse_stream(sub: jobs::Subscription) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let jobs::Subscription { first, terminal, mut rx } = sub;
        if let Some(ev) = first {
            yield sse_event(&ev);
            if terminal {
                return;
            }
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let end = jobs::is_terminal_event(&ev);
                    yield sse_event(&ev);
                    if end {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break, // канал закрыт
            }
        }
    }
}

fn sse_event(ev: &Value) -> Result<Event, Infallible> {
    Ok(Event::default().data(serde_json::to_string(ev).unwrap_or_default()))
}

/// POST /projects/{pid}/align — выровнять тайминги реплик по распознанной речи (subalign, как галка
/// «Выровнять тайминги по речи» при импорте субтитров). Джоба: распознавание слов по вокалу проекта,
/// реплики с изменённым таймингом становятся dirty.
pub async fn align_project(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    match align_enqueue(&st, &pid).await {
        Ok(job_id) => Json(json!({ "job_id": job_id, "project_id": pid })).into_response(),
        Err(resp) => *resp,
    }
}

async fn align_enqueue(st: &AppState, pid: &str) -> Result<String, Box<Response>> {
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    if !dir.join("project.json").is_file() {
        return Err(Box::new((StatusCode::CONFLICT, "project not analyzed yet").into_response()));
    }
    let sel = models::load_selection(&st.models_root);
    let asr = models::resolve_asr_choice(&st.repo_root, &st.models_root, &sel);
    let backend = models::stage_backend(&st.models_root, "asr_backend");
    let dir_for_job = dir.clone();
    let pid_res = pid.to_string();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        let proj = Project::from_json(&std::fs::read_to_string(dir_for_job.join("project.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if proj.segments.iter().all(|s| s.src_text.trim().is_empty()) {
            return Err("выравнивание по речи: у реплик нет текста на языке оригинала (субтитры импортированы на языке перевода)".into());
        }
        let wav = ["vocals16_clean.wav", "vocals16.wav"]
            .iter()
            .map(|f| dir_for_job.join(f))
            .find(|p| p.is_file())
            .ok_or("выравнивание по речи: нет дорожки вокала проекта — сначала анализ")?;
        progress(json!({ "stage": "asr", "msg": "выравнивание по речи: распознавание слов" }));
        std::env::set_var("DUB_ASR_BACKEND", &backend);
        let words: Vec<subalign::Heard> = models::build_engine(&asr)
            .transcribe(&wav, "auto")
            .map_err(|e| format!("выравнивание по речи: распознавание: {e}"))?
            .into_iter()
            .flat_map(|s| s.words)
            .map(|w| (w.word, w.start, w.end))
            .collect();
        jobs::check_cancelled()?;
        let refs: Vec<(f64, f64, &str)> = proj.segments.iter().map(|s| (s.start, s.end, s.src_text.as_str())).collect();
        let a = subalign::align(&refs, &words).ok_or("выравнивание по речи: речь не распознана — тайминги не менялись")?;
        if a.share < subalign::MIN_ALIGNED_SHARE {
            return Err(format!(
                "выравнивание по речи: реплики не совпали с речью (сопоставлено {:.0}%) — тайминги не менялись",
                a.share * 100.0
            ));
        }
        // Распознавание шло минутами: правки, сделанные за это время, остаются, а выравнивание ложится,
        // только пока реплики те же, что сопоставлялись со словами.
        let _held = project_writes();
        let mut fresh = Project::from_json(&std::fs::read_to_string(dir_for_job.join("project.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let same = |p: &Project| p.segments.iter().map(|s| (s.id.clone(), s.start, s.end, s.src_text.clone())).collect::<Vec<_>>();
        if same(&fresh) != same(&proj) {
            return Err("выравнивание по речи: реплики изменились, пока шло распознавание — тайминги не менялись, запустите выравнивание ещё раз".into());
        }
        let changed = apply_alignment(&mut fresh, &a);
        write_project(&dir_for_job, &fresh)?;
        progress(json!({ "stage": "asr", "msg": format!(
            "выровнено по речи: {:.0}% реплик по словам, изменён тайминг у {changed}; сдвиг {:+.2} с",
            a.share * 100.0, a.offset
        ) }));
        Ok(json!({ "project_id": pid_res, "changed": changed, "aligned_share": a.share, "offset": a.offset }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::Align, pid, dir, json!({})), job)
        .await
        .map_err(|e| Box::new(enqueue_error(e)))
}

/// Перенести тайминги выравнивания на реплики проекта (как при импорте с выравниванием: extra.timing и
/// исходные cue_start/cue_end). Реплика, чей тайминг сдвинулся больше чем на 10 мс, становится dirty.
fn apply_alignment(proj: &mut Project, a: &subalign::Alignment) -> usize {
    let mut changed = 0;
    for (s, p) in proj.segments.iter_mut().zip(&a.cues) {
        let how = match p.how {
            subalign::How::Aligned => "asr_aligned",
            subalign::How::Shifted => "asr_shifted",
        };
        if (s.start - p.start).abs() > 0.01 || (s.end - p.end).abs() > 0.01 {
            s.extra.entry("cue_start").or_insert(json!(s.start));
            s.extra.entry("cue_end").or_insert(json!(s.end));
            s.start = p.start;
            s.end = p.end;
            s.dirty = true;
            changed += 1;
        }
        s.extra.insert("timing".into(), Value::String(how.into()));
    }
    changed
}

#[cfg(test)]
mod align_tests {
    use super::*;

    fn seg(id: &str, start: f64, end: f64) -> dub_core::Segment {
        let mut s: dub_core::Segment = serde_json::from_value(json!({ "id": id, "start": start, "end": end, "src_text": "x", "tgt_text": "" })).unwrap();
        s.dirty = false;
        s
    }

    #[test]
    fn moved_lines_become_dirty_and_keep_their_file_timing() {
        let mut p: Project = serde_json::from_value(json!({ "segments": [] })).unwrap();
        p.segments = vec![seg("s0", 1.0, 2.0), seg("s1", 3.0, 4.0)];
        let a = subalign::Alignment {
            cues: vec![
                subalign::Placed { start: 1.004, end: 2.0, how: subalign::How::Aligned },
                subalign::Placed { start: 3.5, end: 4.4, how: subalign::How::Shifted },
            ],
            offset: 0.5,
            share: 0.5,
        };
        assert_eq!(apply_alignment(&mut p, &a), 1);
        assert!(!p.segments[0].dirty, "сдвиг меньше 10 мс не трогает реплику");
        assert_eq!(p.segments[0].extra["timing"], "asr_aligned");
        let moved = &p.segments[1];
        assert!(moved.dirty);
        assert_eq!((moved.start, moved.end), (3.5, 4.4));
        assert_eq!((moved.extra["cue_start"].as_f64(), moved.extra["cue_end"].as_f64()), (Some(3.0), Some(4.0)));
        assert_eq!(moved.extra["timing"], "asr_shifted");
    }
}

// ─── SPA fallback ───────────────────────────────────────────────────────────

async fn spa_fallback(State(st): State<AppState>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    spa::serve_spa(st.web_root.as_deref(), path).await
}
