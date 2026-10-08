//! Projects an agent makes of a file where it lies, and the single-stage jobs its one-call tools run
//! on them: the voice split from the background, and the text burned into the picture.
//!
//! A project made from a path reads the file where it is (source.txt names it, nothing is copied)
//! and carries agent.json: made by an agent, of which file (its path, size and change time), for
//! which tool and arguments (key). The same file with the same key finds the same project again,
//! so a tool called twice answers from the work already done.

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{jobs, media, models, AppState};

/// The record that marks a project as an agent's.
pub(crate) const AGENT_FILE: &str = "agent.json";
/// The text detect-text read off the picture.
pub(crate) const REGIONS_FILE: &str = "text_regions.json";

/// The files the studio takes, as the window's file picker lists them.
const VIDEO_EXT: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v", "flv", "wmv", "ts", "mpg", "mpeg", "3gp", "ogv", "mts", "m2ts", "vob", "f4v"];
const AUDIO_EXT: &[&str] = &["wav", "mp3", "flac", "m4a", "aac", "ogg", "opus", "wma", "aiff", "aif", "alac"];

/// Looking a project up and making it happen under one lock: two calls for the same file at once
/// would otherwise both miss and make two projects.
static MAKING: Mutex<()> = Mutex::new(());

/// Who made a project: an agent's tool, or the window.
pub(crate) fn source_of(dir: &Path) -> &'static str {
    if record(dir).is_some() {
        "agent"
    } else {
        "window"
    }
}

/// The project's agent.json when it is its own: a copy made for another language carries its
/// parent's, whose project_id is not the copy's.
fn record(dir: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(dir.join(AGENT_FILE)).ok()?;
    let rec: Value = serde_json::from_str(&text).ok()?;
    let pid = dir.file_name()?.to_str()?;
    (rec["project_id"] == pid).then_some(rec)
}

/// A file named by its path: as given, and what identifies it - the path resolved (case folded
/// on Windows), its size and when it last changed.
struct Source {
    path: String,
    identity: String,
    size: u64,
    modified: u64,
}

impl Source {
    fn of(path: &str) -> Result<Source, String> {
        let given = Path::new(path);
        if !given.is_absolute() {
            return Err(format!("{path} is not a full path: name the file with its drive (C:\\...) or its share (\\\\server\\...)"));
        }
        let meta = std::fs::metadata(given).map_err(|e| format!("{path} is not a file on this computer ({e})"))?;
        if !meta.is_file() {
            return Err(format!("{path} is a folder, not a file"));
        }
        let ext = given.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !VIDEO_EXT.contains(&ext.as_str()) && !AUDIO_EXT.contains(&ext.as_str()) {
            return Err(format!("{path} is not a video or audio file: the studio takes {} and {}", VIDEO_EXT.join(", "), AUDIO_EXT.join(", ")));
        }
        let resolved = std::fs::canonicalize(given).map_err(|e| format!("{path}: {e}"))?.to_string_lossy().into_owned();
        let identity = if cfg!(windows) { resolved.to_lowercase() } else { resolved };
        let modified = meta
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs());
        Ok(Source { path: path.to_string(), identity, size: meta.len(), modified })
    }

    fn describe(&self) -> Value {
        json!({ "path": self.path, "identity": self.identity, "size": self.size, "modified": self.modified })
    }

    fn is(&self, file: &Value) -> bool {
        file["identity"] == self.identity.as_str() && file["size"] == self.size && file["modified"] == self.modified
    }

    fn name(&self) -> String {
        Path::new(&self.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.clone())
    }
}

/// The newest analysable project an agent made of this file for this key.
fn find(workspace: &Path, file: &Source, key: &str) -> Option<String> {
    let entries = std::fs::read_dir(workspace).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.join("project.json").is_file())
        .filter_map(|dir| record(&dir).map(|rec| (dir, rec)))
        .filter(|(_, rec)| rec["key"] == key && file.is(&rec["file"]))
        .max_by_key(|(_, rec)| rec["created_at"].as_u64().unwrap_or_default())
        .and_then(|(dir, _)| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// JSON written whole or not at all.
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    dub_core::atomic::write(path, format!("{value:#}").as_bytes())
}

/// POST /projects/from-path {path, key?, tool?} — a project of a video or audio file on this
/// computer, read where it lies. With a key, the project an agent made earlier of the same file
/// (same path, size and change time) for the same key is answered instead of a new one.
pub async fn from_path(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    match tokio::task::spawn_blocking(crate::mcp::carry(move || make(&st, &body))).await {
        Ok(Ok(answer)) => Json(answer).into_response(),
        Ok(Err((status, why))) => (status, why).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("from-path: {e}")).into_response(),
    }
}

fn make(st: &AppState, body: &Value) -> Result<Value, (StatusCode, String)> {
    let refused = |why: String| (StatusCode::BAD_REQUEST, why);
    let path = body.get("path").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()).ok_or_else(|| refused("path is required".into()))?;
    let file = Source::of(path).map_err(refused)?;
    let key = body.get("key").and_then(Value::as_str).unwrap_or_default();
    let tool = body.get("tool").and_then(Value::as_str).unwrap_or_default();
    let _one_at_a_time = MAKING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !key.is_empty() {
        if let Some(pid) = find(&st.workspace, &file, key) {
            return Ok(json!({ "project_id": pid, "reused": true, "filename": file.name() }));
        }
    }
    let meta = media::probe(Path::new(&file.path)).map_err(|e| refused(format!("{} is not a video or audio file the studio can read: {e}", file.path)))?;
    let mut pid = uuid::Uuid::new_v4().simple().to_string();
    pid.truncate(12);
    let dir = st.workspace.join(&pid);
    let made = (|| -> Result<(), String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        std::fs::write(dir.join("source.txt"), file.path.as_bytes()).map_err(|e| format!("write source.txt: {e}"))?;
        std::fs::write(dir.join("name.txt"), file.name().as_bytes()).map_err(|e| format!("write name.txt: {e}"))?;
        let meta = dub_core::Meta {
            video: file.path.clone(),
            duration: meta.duration,
            width: meta.width,
            height: meta.height,
            fps: meta.fps,
            src_codec: meta.src_codec,
            extra: serde_json::Map::new(),
        };
        crate::save_project_atomic(&dir, &crate::initial_project(&dir, meta))?;
        let created_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
        write_json(
            &dir.join(AGENT_FILE),
            &json!({ "source": "agent", "project_id": pid, "tool": tool, "key": key, "file": file.describe(), "created_at": created_at }),
        )
    })();
    if let Err(why) = made {
        let _ = std::fs::remove_dir_all(&dir);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, why));
    }
    Ok(json!({ "project_id": pid, "reused": false, "filename": file.name() }))
}

/// The file a project was made from, or why the project cannot be worked on.
fn source_file(dir: &Path) -> Result<PathBuf, (StatusCode, String)> {
    let text = std::fs::read_to_string(dir.join("source.txt")).map_err(|_| (StatusCode::CONFLICT, "no source uploaded".to_string()))?;
    let input = PathBuf::from(text.trim());
    if !input.is_file() {
        return Err((StatusCode::CONFLICT, format!("the file the project was made from is not there any more: {}", input.display())));
    }
    Ok(input)
}

/// Queues a job of the project and answers {job_id}; the same kind already queued or at work on
/// the project is 409 job_conflict with its job_id. The one-call tools (mcp/atomic.rs) look for a
/// separation or a reading at work by its kind and the project in GET /jobs?pid=.
async fn enqueue(st: &AppState, kind: jobs::JobKind, pid: &str, job: jobs::JobFn) -> Response {
    match st.jobs.enqueue(jobs::JobMeta::new(kind, Some(pid)), job).await {
        Ok(id) => Json(json!({ "job_id": id })).into_response(),
        Err(e) => crate::enqueue_error(e),
    }
}

/// A result found done, marked as such.
fn cached(mut found: Value) -> Value {
    found["cached"] = true.into();
    found
}

/// The voice and the background, when both are separated from the audio as it is extracted now.
fn stems_of(dir: &Path) -> Result<Option<Value>, String> {
    let stems = dir.join("stems");
    let (vocals, background) = (stems.join("vocals.wav"), stems.join("instrumental.wav"));
    if !(vocals.is_file() && background.is_file() && media::stems_current(dir)?) {
        return Ok(None);
    }
    Ok(Some(json!({ "vocals": vocals.to_string_lossy(), "background": background.to_string_lossy() })))
}

/// POST /projects/{pid}/separate — the voice and the background (music, effects) of the
/// project's audio, as the render separates them (stems/, shared with analyze and render):
/// {cached: true, vocals, background} when they are there, else a job whose result is
/// {vocals, background}.
pub async fn separate(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let input = match source_file(&dir) {
        Ok(input) => input,
        Err(refused) => return refused.into_response(),
    };
    match stems_of(&dir) {
        Ok(Some(found)) => return Json(cached(found)).into_response(),
        Ok(None) => {}
        Err(why) => return (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
    }
    let model = models::resolve_sep(&st.models_root, &models::load_selection(&st.models_root));
    if !model.is_file() {
        return (StatusCode::CONFLICT, format!("the voice separator's model is not installed ({}): models_status names it, models_download fetches it", model.display())).into_response();
    }
    let cli = dub_sep::engine_cli(&st.repo_root, models::stage_backend(&st.models_root, "sep_backend"));
    let job = separation(dir, input, cli, model, st.repo_root.clone(), st.models_root.clone());
    enqueue(&st, jobs::JobKind::Separate, &pid, job).await
}

/// The separation as a job. One that finds the stems made by the time it runs answers them
/// without separating again.
fn separation(dir: PathBuf, input: PathBuf, cli: PathBuf, model: PathBuf, repo_root: PathBuf, models_root: PathBuf) -> jobs::JobFn {
    Box::new(move |progress: jobs::ProgressFn| {
        crate::clean_partials(&dir);
        jobs::check_cancelled()?;
        let stems = dir.join("stems");
        if let Some(found) = stems_of(&dir)? {
            return Ok(found);
        }
        media::drop_stale_separation(&dir)?;
        let cb = |ev: Value| progress(ev);
        crate::ensure_job_components(&repo_root, &models_root, false, false, &cb)?;
        jobs::check_cancelled()?;
        if !cli.is_file() {
            return Err(format!("the voice separator's engine is not installed ({}): models_status names it, models_download fetches it", cli.display()));
        }
        let audio_hq = dir.join("audio_hq.wav");
        if !audio_hq.is_file() {
            cb(json!({ "stage": "separate", "msg": t!("atomic-extract-separation-audio") }));
            media::extract_audio(&input, &audio_hq, 44100, 2)?;
            jobs::check_cancelled()?;
        }
        cb(json!({ "stage": "separate", "msg": t!("atomic-separating", model = "Mel-Band Roformer voc_fv6-Q8_0") }));
        let split = dub_sep::separate(&audio_hq, &stems, &cli, &model).map_err(|e| t!("atomic-separation-failed", error = e.to_string()))?;
        media::mark_separation(&stems)?;
        Ok(json!({ "vocals": split.vocals.to_string_lossy(), "background": split.instrumental.to_string_lossy() }))
    })
}

/// The analysis's own detection settings (ocr.rs), so that what the agent reads is what the
/// studio blurs and translates: the least seconds a text stays, the overlap a track keeps, the
/// box padding, the jitter a track allows, the least recognition score.
const OCR_MIN_DUR: f32 = 0.3;
const OCR_IOU: f32 = 0.3;
const OCR_PAD: i64 = 8;
const OCR_JITTER: f32 = 20.0;
const OCR_SCORE: f32 = 0.4;

/// POST /projects/{pid}/detect-text — the text burned into the picture, read at the studio's
/// caption rate: {cached: true, ...} when it was read before, else a job. Either way the answer
/// is {file, width, height, fps, count, regions: [{text, x, y, w, h, t0, t1}]}.
pub async fn detect_text(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let input = match source_file(&dir) {
        Ok(input) => input,
        Err(refused) => return refused.into_response(),
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let (width, height) = (proj.meta.width, proj.meta.height);
    if width <= 0 || height <= 0 {
        return (StatusCode::CONFLICT, "the project is audio: there is no picture to read text from").into_response();
    }
    let file = dir.join(REGIONS_FILE);
    if file.is_file() {
        return match read_regions(&file) {
            Ok(found) => Json(cached(found)).into_response(),
            Err(why) => (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
        };
    }
    let ocr = dub_ocr::OcrPaths::under(&st.models_root);
    if !ocr.all_exist() {
        return (StatusCode::CONFLICT, format!("the on-screen text reader is not installed ({}): models_status names it, models_download fetches it", ocr.det.display())).into_response();
    }
    let job = reading(input, dir, ocr, st.opts.caption_fps.max(1), (width, height));
    enqueue(&st, jobs::JobKind::DetectText, &pid, job).await
}

/// The text read before, as text_regions.json holds it.
fn read_regions(file: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("read {}: {e}", file.display()))
}

/// The reading as a job. One that finds the text read by the time it runs answers it without
/// reading again.
fn reading(input: PathBuf, dir: PathBuf, ocr: dub_ocr::OcrPaths, fps: i32, (width, height): (i64, i64)) -> jobs::JobFn {
    Box::new(move |progress: jobs::ProgressFn| {
        crate::clean_partials(&dir);
        jobs::check_cancelled()?;
        let file = dir.join(REGIONS_FILE);
        if file.is_file() {
            return read_regions(&file);
        }
        progress(json!({ "stage": "ocr_detect", "msg": t!("ocr-detecting-burned-text", model = "PP-OCR DBNet+CRNN") }));
        let (regions, _) = dub_ocr::detect_regions(&input, &dir, &ocr, fps, OCR_MIN_DUR, OCR_IOU, OCR_PAD, OCR_JITTER, OCR_SCORE)?;
        jobs::check_cancelled()?;
        let found = json!({
            "file": file.to_string_lossy(),
            "width": width,
            "height": height,
            "fps": fps,
            "count": regions.len(),
            "regions": regions
                .iter()
                .map(|r| json!({ "text": r.text, "x": r.x, "y": r.y, "w": r.w, "h": r.h, "t0": r.t0, "t1": r.t1 }))
                .collect::<Vec<_>>(),
        });
        write_json(&file, &found)?;
        Ok(found)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second of a tone as a WAV, the least media file ffprobe reads.
    fn tone(path: &Path) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut out = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..16_000 {
            out.write_sample(((i as f32 * 0.07).sin() * 8_000.0) as i16).unwrap();
        }
        out.finalize().unwrap();
    }

    async fn call(st: &AppState, body: Value) -> (StatusCode, Value) {
        let response = from_path(State(st.clone()), Json(body)).await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, body)
    }

    #[tokio::test]
    async fn a_path_that_is_no_media_file_makes_no_project() {
        let root = tempfile::tempdir().unwrap();
        let st = AppState::new(root.path());
        let media = tempfile::tempdir().unwrap();
        let missing = media.path().join("gone.mp4");
        let (status, why) = call(&st, json!({ "path": missing.to_string_lossy() })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(why.as_str().unwrap().contains("is not a file on this computer"), "{why}");

        let (status, why) = call(&st, json!({ "path": "clip.mp4" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "a relative path is refused");
        assert!(why.as_str().unwrap().contains("not a full path"), "{why}");

        let notes = media.path().join("notes.txt");
        std::fs::write(&notes, "hello").unwrap();
        let (status, why) = call(&st, json!({ "path": notes.to_string_lossy() })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(why.as_str().unwrap().contains("not a video or audio file"), "{why}");

        let fake = media.path().join("fake.mp4");
        std::fs::write(&fake, b"not a video at all").unwrap();
        let (status, why) = call(&st, json!({ "path": fake.to_string_lossy() })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "a file that only looks like a video is refused");
        assert!(why.as_str().unwrap().contains("can read"), "{why}");

        let (status, _) = call(&st, json!({ "path": media.path().to_string_lossy() })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "a folder is refused");
        assert_eq!(std::fs::read_dir(&st.workspace).unwrap().count(), 0, "nothing was made");
    }

    #[tokio::test]
    async fn a_file_is_read_where_it_lies_and_found_again_by_its_key() {
        let root = tempfile::tempdir().unwrap();
        let st = AppState::new(root.path());
        let media = tempfile::tempdir().unwrap();
        let wav = media.path().join("Voice note.wav");
        tone(&wav);
        let path = wav.to_string_lossy().into_owned();

        let (status, made) = call(&st, json!({ "path": path, "key": "transcribe_file?mode=transcribe", "tool": "transcribe_file" })).await;
        assert_eq!(status, StatusCode::OK, "{made}");
        assert_eq!(made["reused"], false);
        let pid = made["project_id"].as_str().unwrap().to_string();
        let dir = st.workspace.join(&pid);
        assert_eq!(std::fs::read_to_string(dir.join("source.txt")).unwrap(), path, "the project reads the file where it lies");
        let copies: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("source.") && n != "source.txt").collect();
        assert!(copies.is_empty(), "nothing is copied: {copies:?}");
        assert_eq!(std::fs::read_to_string(dir.join("name.txt")).unwrap(), "Voice note.wav");
        let proj = st.load_project(&pid).unwrap();
        assert!((proj.meta.duration - 1.0).abs() < 0.05, "probed: {}", proj.meta.duration);
        assert_eq!((proj.meta.width, proj.meta.video.as_str()), (0, path.as_str()));
        assert_eq!(source_of(&dir), "agent");

        let (_, again) = call(&st, json!({ "path": path, "key": "transcribe_file?mode=transcribe" })).await;
        assert_eq!((again["project_id"].as_str(), again["reused"].clone()), (Some(pid.as_str()), json!(true)));
        let (_, other) = call(&st, json!({ "path": path, "key": "translate_file?tgt_lang=es" })).await;
        assert_ne!(other["project_id"].as_str(), Some(pid.as_str()), "another key is another project");
        let (_, plain) = call(&st, json!({ "path": path })).await;
        assert_eq!(plain["reused"], false, "without a key a new project is made");

        tone(&wav);
        std::fs::OpenOptions::new().write(true).open(&wav).unwrap().set_len(32_100).unwrap();
        let (_, changed) = call(&st, json!({ "path": path, "key": "transcribe_file?mode=transcribe" })).await;
        assert_ne!(changed["project_id"].as_str(), Some(pid.as_str()), "a changed file is another project");
    }

    #[test]
    fn a_copy_for_another_language_is_not_the_agents() {
        let root = tempfile::tempdir().unwrap();
        let (own, copy) = (root.path().join("aaa111"), root.path().join("bbb222"));
        for dir in [&own, &copy] {
            std::fs::create_dir_all(dir).unwrap();
            write_json(&dir.join(AGENT_FILE), &json!({ "source": "agent", "project_id": "aaa111" })).unwrap();
        }
        assert_eq!((source_of(&own), source_of(&copy)), ("agent", "window"));
        assert_eq!(source_of(root.path()), "window");
    }

    #[test]
    fn a_job_queued_again_answers_the_first_ones_work() {
        let dir = tempfile::tempdir().unwrap();
        let nowhere = dir.path().join("not-installed");
        let quiet: jobs::ProgressFn = std::sync::Arc::new(|_| {});
        let stems = dir.path().join("stems");
        std::fs::create_dir_all(&stems).unwrap();
        for name in ["vocals.wav", "instrumental.wav"] {
            std::fs::write(stems.join(name), b"w").unwrap();
        }
        media::mark_separation(&stems).unwrap();
        let job = separation(dir.path().into(), nowhere.clone(), nowhere.clone(), nowhere.clone(), nowhere.clone(), nowhere.clone());
        let split = job(quiet.clone()).expect("the stems are there: no engine is needed");
        assert!(split["vocals"].as_str().unwrap().ends_with("vocals.wav") && split["background"].as_str().unwrap().ends_with("instrumental.wav"), "{split}");

        let read = json!({ "count": 1, "regions": [{ "text": "EXIT", "x": 10, "y": 20, "w": 120, "h": 40, "t0": 1.0, "t1": 3.5 }] });
        write_json(&dir.path().join(REGIONS_FILE), &read).unwrap();
        let job = reading(nowhere.clone(), dir.path().into(), dub_ocr::OcrPaths::under(&nowhere), 4, (1280, 720));
        assert_eq!(job(quiet).expect("the text was read: no reader is needed"), read);
    }

    #[tokio::test]
    async fn the_jobs_are_listed_by_kind_and_project_and_not_queued_twice() {
        let root = tempfile::tempdir().unwrap();
        let st = AppState::new(root.path());
        let (release, held) = std::sync::mpsc::channel::<()>();
        let hold: jobs::JobFn = Box::new(move |_| {
            let _ = held.recv();
            Ok(json!({}))
        });
        let answer = |response: Response| async move {
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        };
        let (status, first) = answer(enqueue(&st, jobs::JobKind::Separate, "p1", hold).await).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        let (status, again) = answer(enqueue(&st, jobs::JobKind::Separate, "p1", Box::new(|_| Ok(json!({})))).await).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(again, json!({ "error": "job_conflict", "job_id": first["job_id"], "kind": "separate" }));
        let (status, reading) = answer(enqueue(&st, jobs::JobKind::DetectText, "p1", Box::new(|_| Ok(json!({})))).await).await;
        assert_eq!(status, StatusCode::OK, "a reading is not a separation: {reading}");
        let (status, _) = answer(enqueue(&st, jobs::JobKind::Separate, "p2", Box::new(|_| Ok(json!({})))).await).await;
        assert_eq!(status, StatusCode::OK, "another project separates on its own");
        let listed: Vec<(Value, Value)> = st.jobs.list(Some("p1")).await.iter().map(|job| (job["kind"].clone(), job["pid"].clone())).collect();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(listed.contains(&(json!("separate"), json!("p1"))) && listed.contains(&(json!("detect_text"), json!("p1"))), "{listed:?}");
        release.send(()).unwrap();
    }

    #[tokio::test]
    async fn separated_stems_are_answered_without_a_job_and_audio_has_no_text_to_read() {
        let root = tempfile::tempdir().unwrap();
        let st = AppState::new(root.path());
        let media = tempfile::tempdir().unwrap();
        let wav = media.path().join("song.wav");
        tone(&wav);
        let (_, made) = call(&st, json!({ "path": wav.to_string_lossy() })).await;
        let pid = made["project_id"].as_str().unwrap().to_string();
        let read = |response: Response| async move {
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
            (status, String::from_utf8_lossy(&bytes).into_owned())
        };

        let listed = || async {
            let (_, listed) = read(crate::project_files::files(State(st.clone()), AxPath(pid.clone())).await).await;
            serde_json::from_str::<Value>(&listed).unwrap()
        };
        let stems = st.workspace.join(&pid).join("stems");
        std::fs::create_dir_all(&stems).unwrap();
        std::fs::write(stems.join("vocals.wav"), b"v").unwrap();
        let (status, why) = read(separate(State(st.clone()), AxPath(pid.clone())).await).await;
        assert_eq!(status, StatusCode::CONFLICT, "half the stems are not the answer, and no separator is installed here: {why}");
        assert!(why.contains("not installed"), "{why}");
        let half = listed().await;
        assert!(half["vocals"].as_str().unwrap().ends_with("vocals.wav") && half["background"].is_null(), "{half}");
        std::fs::write(stems.join("instrumental.wav"), b"i").unwrap();
        crate::media::mark_separation(&stems).unwrap();
        let (status, body) = read(separate(State(st.clone()), AxPath(pid.clone())).await).await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["cached"], true);
        assert!(body["vocals"].as_str().unwrap().ends_with("vocals.wav") && body["background"].as_str().unwrap().ends_with("instrumental.wav"));
        assert_eq!((listed().await["vocals"].clone(), listed().await["background"].clone()), (body["vocals"].clone(), body["background"].clone()), "the project's files name the stems");

        let (status, why) = read(detect_text(State(st.clone()), AxPath(pid.clone())).await).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(why.contains("no picture"), "{why}");

        std::fs::remove_file(&wav).unwrap();
        let (status, why) = read(separate(State(st.clone()), AxPath(pid)).await).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(why.contains("not there any more"), "{why}");
    }
}
