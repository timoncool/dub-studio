//! The studio as an MCP server: Streamable HTTP, stateless JSON-RPC at `/mcp`.
//!
//! Every tool is a route of the studio's own API, called inside the process
//! through the same router the page talks to, so an agent does exactly what
//! the page does through the same code: make a project of a video, analyze it,
//! edit the transcript and the translation line by line, cast the voices,
//! render, export more languages, save the result to a folder. A file an agent
//! names by its path is sent to the route as the multipart upload the page
//! would send.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower::ServiceExt;

mod atomic;
mod window;

pub(crate) use window::{carry, carry_job, save_with_revision, tell_windows, track, REV_HEADER};
pub use window::{window_events, window_focus, window_result};

/// The studio's API router, set once the service has built it.
static API: OnceLock<Router> = OnceLock::new();

/// Takes the API router before the Origin/Host guard is layered: the guard
/// wraps this router together with `/mcp` and `/mcp/status` from outside, so
/// the MCP endpoints are guarded and the tools' own calls do not go through it.
/// The tools' requests still carry `Host: 127.0.0.1`, so they pass the guard
/// should it be put inside this router.
pub fn install(api: Router) {
    let _ = API.set(api);
}

/// The studio's name, as clients show it.
const STUDIO: &str = "Dub Studio";
/// The name clients register the server under.
const SERVER_NAME: &str = "dub-studio";
/// The skill an agent reads, served as a resource and a prompt.
const SKILL: &str = include_str!("../../../docs/mcp-skill.md");
const SKILL_URI: &str = "studio://skill";
const LANGUAGES_URI: &str = "studio://languages";
const EDITS_URI: &str = "studio://patch-ops";
/// Answers longer than this are cut: a transcript of hundreds of lines is more
/// than an agent reads in one call.
const LIMIT: usize = 60_000;

/// What a tool sends to its route.
enum Payload {
    None,
    Json(Value),
    /// Multipart form fields and files, as the page uploads them.
    Form { fields: Vec<(String, String)>, files: Vec<(String, PathBuf, String)>,
    },
    /// A command for the studio's window: what is on screen, the controls, the editor. Answered by
    /// the page itself.
    Window { command: &'static str, args: Value, seconds: u64,
    },
}

struct Call {
    method: Method,
    path: String,
    payload: Payload,
}

struct Tool {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    call: fn(&Value) -> Result<Call, String>,
}

fn get(path: String) -> Result<Call, String> {
    Ok(Call { method: Method::GET, path, payload: Payload::None,
    })
}

fn post(path: String, body: Value) -> Result<Call, String> {
    Ok(Call { method: Method::POST, path, payload: Payload::Json(body),
    })
}

fn send(method: Method, path: String, body: Value) -> Result<Call, String> {
    Ok(Call { method, path, payload: Payload::Json(body),
    })
}

fn composite(kind: &'static str) -> Result<Call, String> {
    Ok(Call { method: Method::GET, path: format!("composite:{kind}"), payload: Payload::None,
    })
}

/// A GET inside the process, as JSON; a route that refuses says why.
async fn fetch(path: &str) -> Result<Value, String> {
    let (status, text) = call_route(Call { method: Method::GET, path: path.into(), payload: Payload::None,
    }).await?;
    if !status.is_success() {
        return Err(format!("GET {path}: {status} {text}"));
    }
    serde_json::from_str(&text).map_err(|_| {
        format!("GET {path} did not answer JSON: {}", text.chars().take(200).collect::<String>())
    })
}

/// The app's version: tauri.conf.json is its single source, as for the window.
fn app_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let conf: Value = serde_json::from_str(include_str!("../../../desktop/src-tauri/tauri.conf.json")).expect("tauri.conf.json is JSON");
        conf["version"].as_str().expect("tauri.conf.json names the app's version").to_string()
    })
}

// ---------------------------------------------------------------- jobs

/// A job that ended, one way or the other; any other status is still at work.
const FINISHED: &[&str] = &["done", "error", "failed", "cancelled", "abandoned", "interrupted",
];

fn finished(job: &Value) -> bool {
    job["status"].as_str().is_some_and(|status| FINISHED.contains(&status))
}

/// The rows of GET /jobs: its list, bare or under "jobs".
fn job_rows(listed: &Value) -> Result<Vec<Value>, String> {
    listed
        .as_array()
        .or_else(|| listed.get("jobs").and_then(Value::as_array))
        .cloned()
        .ok_or_else(|| {
            format!("GET /jobs answered no list of jobs: {}", listed.to_string().chars().take(200).collect::<String>())
        })
}

/// A job as an agent follows it: what it is, where it got, what it made. A
/// result that is a whole project is left to project_get.
fn compact_job(job: &Value) -> Value {
    let result = &job["result"];
    let result = if result.get("segments").is_some() {
        json!("the project, updated: project_get reads it")
    } else {
        result.clone()
    };
    let mut row = json!({
        "id": job.get("id").or_else(|| job.get("job_id")).cloned().unwrap_or(Value::Null),
        "kind": job["kind"], "pid": job["pid"], "status": job["status"],
        "stage": job["stage"], "msg": job["msg"], "pct": job["pct"],
    });
    if !job["position"].is_null() {
        row["position"] = job["position"].clone();
    }
    if !result.is_null() {
        row["result"] = result;
    }
    if !job["error"].is_null() {
        row["error"] = job["error"].clone();
    }
    row
}

/// Where a running job is, in one line.
fn describe_job(job: &Value) -> String {
    let text = |key: &str| job[key].as_str().map(str::to_string).unwrap_or_default();
    let pct = job["pct"].as_f64().map(|pct| format!(" {pct:.0}%")).unwrap_or_default();
    format!("{} {} of project {} ({}{pct})", text("status"), job["kind"].as_str().unwrap_or("job"), text("pid"), text("stage")).replace(" ()", "")
}

/// The required models still missing and whether the set is ready.
fn compact_setup(setup: &Value) -> Value {
    let missing: Vec<Value> = setup["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|component| component["installed"] == false && component["requirement"] == "required")
        .map(|component| json!({ "id": component["id"], "name": component["name"], "size": component["size"] }))
        .collect();
    json!({ "ready": setup["ready"], "driver_ok": setup["driverOk"], "download_pending_bytes": setup["downloadPending"], "missing_required": missing })
}

/// Every component in one line: what it is, whether it is there, what it takes.
fn compact_models(setup: &Value) -> Value {
    let components: Vec<Value> = setup["components"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|component| {
            json!({
                "id": component["id"], "name": component["name"], "purpose": component["purpose"], "requirement": component["requirement"],
                "installed": component["installed"], "size": component["size"], "on_disk": component["bytesOnDisk"], "vram": component["vram"],
            })
        })
        .collect();
    json!({ "ready": setup["ready"], "driver_ok": setup["driverOk"], "download_pending_bytes": setup["downloadPending"], "components": components })
}

// ---------------------------------------------------------------- projects

/// A line as an agent reads it: without its word timings.
fn compact_segment(segment: &Value) -> Value {
    let mut row = json!({
        "id": segment["id"], "start": segment["start"], "end": segment["end"], "speaker": segment["speaker"],
        "src_text": segment["src_text"], "tgt_text": segment["tgt_text"], "dirty": segment["dirty"],
    });
    for flag in ["hidden", "keep_original"] {
        if segment[flag] == true {
            row[flag] = true.into();
        }
    }
    for said in ["tts_skip", "tts_text"] {
        if segment[said].is_string() {
            row[said] = segment[said].clone();
        }
    }
    if !segment["voice"].is_null() {
        row["voice"] = segment["voice"].clone();
    }
    for computed in ["fit", "takes", "shortened"] {
        if !segment[computed].is_null() {
            row[computed] = segment[computed].clone();
        }
    }
    row
}

/// Titles and blur boxes with the idx their tools take.
fn indexed(items: &Value) -> Value {
    Value::Array(
        items
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(idx, item)| {
                let mut item = item.clone();
                item["idx"] = idx.into();
                item
            })
            .collect(),
    )
}

fn count_dirty(project: &Value) -> usize {
    project["segments"].as_array().into_iter().flatten().filter(|segment| segment["dirty"] == true).count()
}

/// A project without what only the studio reads (word timings, the vision
/// context, stage checksums), its lines narrowed by time or id when asked.
fn compact_project(project: &Value, args: &Value) -> Value {
    let from = args.get("from").and_then(Value::as_f64);
    let to = args.get("to").and_then(Value::as_f64);
    let ids: Vec<&str> = args.get("ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let lines: Vec<Value> = project["segments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|segment| {
            let start = segment["start"].as_f64().unwrap_or_default();
            let end = segment["end"].as_f64().unwrap_or_default();
            from.is_none_or(|from| end >= from) && to.is_none_or(|to| start <= to) && (ids.is_empty() || ids.contains(&segment["id"].as_str().unwrap_or_default()))
        })
        .map(compact_segment)
        .collect();
    let captions = &project["captions"];
    json!({
        "meta": { "video": project["meta"]["video"], "duration": project["meta"]["duration"], "width": project["meta"]["width"], "height": project["meta"]["height"], "fps": project["meta"]["fps"] },
        "mode": project["mode"], "tgt_lang": project["tgt_lang"], "subs": project["subs"], "audio": project["audio"],
        "blur_on": project["render"]["blur"], "casting_enabled": project["casting_enabled"],
        "captions": {
            "sub_style": captions["sub_style"], "sub_y": captions["sub_y"], "preset": captions["preset"], "overrides": captions["overrides"],
            "titles": indexed(&captions["titles"]), "blur_boxes": indexed(&captions["blur_boxes"]),
        },
        "segments_total": project["segments"].as_array().map_or(0, Vec::len),
        "dirty": count_dirty(project),
        "segments": lines,
    })
}

/// A project's words as a transcript: each line's id, time, speaker and text - the recognised
/// original (text src, the default) or the translation (tgt) - narrowed by from and to seconds,
/// as JSON lines or, with format text, one "[0:14.2 SPK 1] words" line each.
fn transcript(project: &Value, args: &Value) -> Value {
    let from = args.get("from").and_then(Value::as_f64);
    let to = args.get("to").and_then(Value::as_f64);
    let translation = args.get("text").and_then(Value::as_str) == Some("tgt");
    let clock = |seconds: f64| {
        let tenths = (seconds.max(0.0) * 10.0).round() as u64;
        format!("{}:{:02}.{}", tenths / 600, tenths % 600 / 10, tenths % 10)
    };
    // the translation as the subtitles burn it: a line's own subtitle text over its translation,
    // and no line that keeps the original speech
    let own: std::collections::HashMap<&str, &str> = project["captions"]["overrides"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| Some((o["seg_id"].as_str()?, o["text"].as_str()?)))
        .collect();
    let lines: Vec<(&Value, &str)> = project["segments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|segment| {
            segment["hidden"] != true && !(translation && segment["keep_original"] == true)
        })
        .map(|segment| {
            let text = match translation {
                true => segment["id"].as_str().and_then(|id| own.get(id).copied()).unwrap_or_else(|| segment["tgt_text"].as_str().unwrap_or_default()),
                false => segment["src_text"].as_str().unwrap_or_default(),
            };
            (segment, text.trim())
        })
        .filter(|(_, text)| !text.is_empty())
        .filter(|(segment, _)| {
            let start = segment["start"].as_f64().unwrap_or_default();
            let end = segment["end"].as_f64().unwrap_or_default();
            from.is_none_or(|from| end >= from) && to.is_none_or(|to| start <= to)
        })
        .collect();
    let speakers: std::collections::BTreeSet<&str> = lines.iter().filter_map(|(segment, _)| segment["speaker"].as_str()).collect();
    if args.get("format").and_then(Value::as_str) == Some("text") {
        let text: Vec<String> = lines
            .iter()
            .map(|(segment, text)| {
                format!("[{} SPK {}] {text}", clock(segment["start"].as_f64().unwrap_or_default()), segment["speaker"].as_str().unwrap_or("-"))
            })
            .collect();
        return json!({ "text": text.join("\n"), "lines": lines.len(), "speakers": speakers });
    }
    let rows: Vec<Value> = lines.iter().map(|(segment, text)| json!({ "id": segment["id"], "start": segment["start"], "end": segment["end"], "speaker": segment["speaker"], "text": text })).collect();
    json!({ "language": if translation { project["tgt_lang"].as_str().unwrap_or_default() } else { "original" }, "speakers": speakers, "lines": rows })
}

/// What an edit changed: the project's state in brief and the part the edit
/// touched, instead of the whole project every edit answers with.
fn compact_change(name: &str, args: &Value, project: &Value) -> Value {
    // an edit made by its op answers as the op's own tool does
    let name = match name {
        "project_patch" => args.get("op").and_then(Value::as_str).and_then(tool_of_op).unwrap_or(name),
        _ => name,
    };
    let mut summary = json!({
        "saved": true, "mode": project["mode"], "tgt_lang": project["tgt_lang"],
        "segments": project["segments"].as_array().map_or(0, Vec::len), "dirty": count_dirty(project),
    });
    let mut named: Vec<String> = args.get("ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    if let Some(id) = args.get("id").and_then(Value::as_str) {
        named.push(id.to_string());
    }
    let segments = project["segments"].as_array().cloned().unwrap_or_default();
    if name == "segments_reorder" {
        named.clear();
    }
    if name == "segment_split" {
        let after = args.get("id").and_then(Value::as_str).and_then(|id| segments.iter().position(|segment| segment["id"] == id)).and_then(|at| segments.get(at + 1));
        named.extend(after.and_then(|segment| segment["id"].as_str()).map(str::to_string),
        );
    }
    let mut changed: Vec<Value> = segments.iter().filter(|segment| named.iter().any(|id| segment["id"] == id.as_str())).map(compact_segment).collect();
    if name == "segment_add" && args.get("id").is_none() {
        let start = args.get("start").and_then(Value::as_f64).unwrap_or_default().max(0.0);
        changed.extend(segments.iter().filter(|segment| {
                    segment["start"].as_f64().is_some_and(|at| (at - start).abs() < 1e-6) && segment["src_text"] == ""
                }).map(compact_segment),
        );
    }
    if !changed.is_empty() {
        summary["changed"] = Value::Array(changed);
    }
    let captions = &project["captions"];
    if name.starts_with("title") {
        summary["titles"] = indexed(&captions["titles"]);
    }
    if name.starts_with("blur") {
        summary["blur_boxes"] = indexed(&captions["blur_boxes"]);
        summary["blur_on"] = project["render"]["blur"].clone();
    }
    if name.starts_with("caption") {
        summary["sub_style"] = captions["sub_style"].clone();
        summary["preset"] = captions["preset"].clone();
        if let Some(line) = args.get("seg_id").and_then(Value::as_str) {
            summary["override"] = captions["overrides"].as_array().into_iter().flatten().find(|entry| entry["seg_id"] == line).cloned().unwrap_or(Value::Null);
        }
    }
    if name.starts_with("subtitles_") || matches!(name, "project_mode_set" | "audio_output_set" | "translation_target_set") {
        summary["subs"] = project["subs"].clone();
        summary["sub_y"] = captions["sub_y"].clone();
    }
    if matches!(name, "voice_set" | "gain_set" | "loudness_set" | "voiceover_gain_set" | "original_track_set" | "rewrite_set" | "translation_style_set" | "translation_target_set" | "project_mode_set" | "audio_output_set") {
        summary["audio"] = project["audio"].clone();
    }
    summary
}

/// Whether an answer is a whole project.
fn is_project(value: &Value) -> bool {
    value.get("segments").is_some_and(Value::is_array) && value.get("captions").is_some()
}

/// The OpenRouter key never leaves the studio, and a proxy's password is hidden.
fn redact(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let mut clean = serde_json::Map::new();
            for (key, value) in fields {
                match key.as_str() {
                    "or_key" => {
                        let set = value.as_str().is_some_and(|key| !key.trim().is_empty());
                        clean.insert("or_key_set".into(), set.into());
                    }
                    "proxy_url" => {
                        let shown = value.as_str().map(|url| Value::String(crate::models::split_proxy_password(url).0)).unwrap_or(value);
                        clean.insert(key, shown);
                    }
                    _ => {
                        clean.insert(key, redact(value));
                    }
                }
            }
            Value::Object(clean)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(redact).collect()),
        other => other,
    }
}

/// What an agent is answered: the parts it acts on, unless it asked for
/// every field with response_format detailed.
fn shape(name: &str, args: &Value, value: Value) -> Value {
    let detailed = args.get("response_format").and_then(Value::as_str) == Some("detailed");
    match name {
        "url_probe" => compact_probe(value),
        _ if detailed => value,
        "project_get" => compact_project(&value, args),
        "project_transcript" => transcript(&value, args),
        _ if is_project(&value) => compact_change(name, args, &value),
        "jobs_list" => match job_rows(&value) {
            Ok(rows) => {
                let mut listed = json!({ "jobs": rows.iter().map(compact_job).collect::<Vec<_>>() });
                if !value["project_job"].is_null() {
                    listed["project_job"] = value["project_job"].clone();
                }
                listed
            }
            Err(problem) => json!({ "error": problem }),
        },
        "job_get" => compact_job(&value),
        "models_status" => compact_models(&value),
        "settings_get" => json!({
            "settings": value["selection"],
            "choices": {
                "asr_engine": value["asr_engines"], "whisper_model": value["whisper_models"], "whisper_compute": value["whisper_computes"],
                "llama_ubatch": value["llama_ubatches"], "higgs_ref_secs": value["higgs_ref_secs_opts"],
                "local_backend": ["auto", "gpu", "cpu"], "sep_backend": ["auto", "gpu", "cpu"], "diar_backend": ["auto", "gpu", "cpu"], "asr_backend": ["auto", "gpu", "cpu"],
            },
        }),
        "projects_list" => {
            let query = args.get("query").and_then(Value::as_str).unwrap_or_default().to_lowercase();
            let bound = |name: &str, end: bool| match args.get(name).and_then(Value::as_str) {
                Some(text) => moment(text, end).map(Some),
                None => Ok(None),
            };
            let (since, until) = match (bound("since", false), bound("until", true)) {
                (Ok(since), Ok(until)) => (since, until),
                (Err(problem), _) | (_, Err(problem)) => return json!({ "error": problem }),
            };
            let projects = value["projects"].as_array().cloned().unwrap_or_default().into_iter().filter(|project| {
                let edited = project["mtime"].as_i64().unwrap_or_default();
                (query.is_empty() || project["video"].as_str().unwrap_or_default().to_lowercase().contains(&query))
                    && since.is_none_or(|since| edited >= since)
                    && until.is_none_or(|until| edited <= until)
            });
            Value::Array(projects.collect())
        }
        _ => value,
    }
}

/// A probe as an agent chooses from it, whatever response_format says: the preview picture's base64
/// is the window's, the automatic subtitles are only their languages, and no track names its file
/// formats, which the download picks itself.
fn compact_probe(mut value: Value) -> Value {
    if let Value::Object(fields) = &mut value {
        fields.remove("thumbnail_data");
        fields.remove("thumbnail_error");
        if let Some(Value::Array(tracks)) = fields.get_mut("subtitles") {
            for track in tracks.iter_mut() {
                *track = json!({ "lang": track["lang"], "name": track["name"] });
            }
        }
        if let Some(Value::Array(tracks)) = fields.get_mut("auto_subtitles") {
            for track in tracks.iter_mut() {
                *track = track["lang"].take();
            }
        }
    }
    value
}

// ---------------------------------------------------------------- waiting

async fn status_summary() -> Value {
    let (jobs, setup, by_link) = tokio::join!(fetch("/jobs"), fetch("/setup/status"), fetch("/url/fetches"));
    let mut summary = json!({});
    match jobs.and_then(|jobs| job_rows(&jobs)) {
        Ok(rows) => {
            let (done, working): (Vec<Value>, Vec<Value>) = rows.into_iter().partition(finished);
            summary["jobs"] = Value::Array(working.iter().map(compact_job).collect());
            summary["finished_jobs"] = Value::Array(done.iter().take(5).map(compact_job).collect());
        }
        Err(problem) => summary["jobs_error"] = problem.into(),
    }
    match setup {
        Ok(setup) => {
            if let Some(download) = download_row(&setup) {
                if let Some(jobs) = summary["jobs"].as_array_mut() {
                    jobs.push(download);
                }
            }
            summary["models"] = compact_setup(&setup);
        }
        Err(problem) => summary["models_error"] = problem.into(),
    }
    match by_link {
        Ok(listed) => {
            let rows = listed["fetches"].as_array().into_iter().flatten().map(fetch_row);
            let (done, working): (Vec<Value>, Vec<Value>) = rows.partition(finished);
            if let Some(jobs) = summary["jobs"].as_array_mut() {
                jobs.extend(working);
            }
            if let Some(ended) = summary["finished_jobs"].as_array_mut() {
                ended.extend(done.into_iter().take(3));
            }
        }
        Err(problem) => summary["url_downloads_error"] = problem.into(),
    }
    summary
}

/// A download by link runs beside the job queue like the models download: it is shown and waited for as work of
/// kind download, and its result names the project it made.
fn fetch_row(fetch: &Value) -> Value {
    let status = match fetch["status"].as_str() {
        Some("downloading") => "running",
        Some("completed") => "done",
        Some("failed") => "error",
        Some(other) => other,
        None => "unknown",
    };
    let pct = match (fetch["downloaded"].as_f64(), fetch["total"].as_f64()) {
        (Some(done), Some(total)) if total > 0.0 => {
            Value::from((done / total * 100.0).min(100.0).round())
        }
        _ => Value::Null,
    };
    let mut row = json!({ "id": fetch["id"], "kind": "download", "pid": fetch["pid"], "status": status, "stage": fetch["phase"], "msg": fetch["title"], "pct": pct, "url": fetch["url"] });
    if fetch["status"] == "completed" {
        row["result"] = json!({ "project_id": fetch["pid"], "subs_imported": fetch["subsImported"] });
    }
    if !fetch["warning"].is_null() {
        row["warning"] = json!({ "code": fetch["warning"], "detail": fetch["warningDetail"] });
    }
    if !fetch["errorCode"].is_null() {
        row["error"] = json!({ "code": fetch["errorCode"], "detail": fetch["error"], "hint": fetch["hint"] });
    }
    row
}

/// The models download runs beside the job queue: while it goes it is shown and waited for as work of
/// kind download.
fn download_row(setup: &Value) -> Option<Value> {
    let active = setup.get("active")?;
    if active["status"] != "downloading" {
        return None;
    }
    let (done, total) = (active["downloaded"].as_f64().unwrap_or(0.0), active["total"].as_f64().unwrap_or(0.0),
    );
    Some(json!({
        "id": active["id"], "kind": "download", "status": "running", "stage": active["phase"],
        "pct": if total > 0.0 { Value::from((done / total * 100.0).round()) } else { Value::Null },
        "components": active["ids"], "waiting_seconds": active["waitingS"],
    }))
}

/// The kinds of work a status summary still has running or waiting.
fn busy(summary: &Value) -> Result<Vec<String>, String> {
    if let Some(problem) = summary.get("jobs_error").and_then(Value::as_str) {
        return Err(format!("The studio's jobs cannot be read: {problem}"));
    }
    Ok(summary["jobs"].as_array().into_iter().flatten().map(|job| job["kind"].as_str().unwrap_or("unknown").to_string()).collect())
}

/// The id of a download by link starts so; a job id is hex and never does.
const DOWNLOAD_BY_LINK: &str = "url";

/// How long one wait holds a call: clients give up on a tool call after about
/// a minute, so a wait answers before that and the agent calls it again.
const WAIT_DEFAULT: u64 = 30;
const WAIT_LONGEST: u64 = 55;

/// Waits for a job or for one kind of work or everything, a slice at a time.
async fn wait_for(args: &Value) -> Result<Value, String> {
    let seconds = args.get("seconds").and_then(Value::as_u64).unwrap_or(WAIT_DEFAULT).clamp(2, WAIT_LONGEST);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let job = args.get("job_id").and_then(Value::as_str).map(str::to_string);
    let until = args.get("until").and_then(Value::as_str).unwrap_or("idle").to_string();
    loop {
        let (done, now) = match &job {
            Some(job) if job.starts_with(DOWNLOAD_BY_LINK) => {
                let state = fetch(&format!("/url/fetches/{}", segment(job)))
                    .await
                    .map_err(|why| format!("No download by link {job} ({why}): its id is fetch.id of project_create_from_url; url_fetches_list shows them."))?;
                let row = fetch_row(&state);
                (finished(&row), row)
            }
            Some(job) => {
                let state = fetch(&format!("/jobs/{}", segment(job)))
                    .await
                    .map_err(|why| format!("No job {job} ({why}): job_id is what project_analyze, project_dub_audio, project_render, project_export_lang, project_retranslate, project_remix, project_align, project_resume, voices_download_pack, segment_shorten, glossary_extract or a one-call tool answering done false returned, or the fetch.id of project_create_from_url (a models download is waited for with until download). Wait for other work with until."))?;
                if state.get("status").and_then(Value::as_str).is_none() {
                    return Err(format!("The job {job} has no status: {state}"));
                }
                (finished(&state), compact_job(&state))
            }
            None => {
                let summary = status_summary().await;
                let working = busy(&summary)?;
                let done = if until == "idle" { working.is_empty() } else { !working.contains(&until) };
                (done, summary)
            }
        };
        if done {
            return Ok(json!({ "done": true, "state": now }));
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(json!({ "done": false, "note": "still running; call studio_wait again to keep waiting", "state": now }),
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Tools whose route waits in the graphics card's queue: while a job runs they
/// would wait behind it for minutes, longer than a client holds a call.
const QUEUED_BEHIND_JOBS: &[&str] = &["project_frame"];

async fn graphics_card_free() -> Result<(), String> {
    let jobs = fetch("/jobs").await.map_err(|why| format!("The studio's jobs cannot be read, so a frame could wait behind a render for minutes: {why}"))?;
    let working: Vec<String> = job_rows(&jobs)?.iter().filter(|job| !finished(job)).map(describe_job).collect();
    if working.is_empty() {
        return Ok(());
    }
    Err(format!("The graphics card is busy: {}. A frame waits behind that work; call studio_wait until idle, then project_frame again.", working.join("; ")))
}

// ---------------------------------------------------------------- annotations

/// MCP tool annotations, from what each tool does.
fn annotations(name: &str) -> Value {
    const READS: &[&str] = &["_get", "_status", "_list", "_catalog", "_files", "_capabilities", "_system", "_frame", "_waveform", "_avatar", "_models", "_voices",
    ];
    // a verb that changes something outweighs a noun that reads
    const CHANGES: &[&str] = &[
        "import", "delete", "create", "update", "cancel", "select", "download", "apply", "add", "set", "assign", "analyze", "render", "remix", "retranslate",
        "align", "patch", "put", "rename", "save", "hide", "keep", "reorder", "regen", "enable", "resume", "export", "open", "reveal",
        "shorten", "pin",
    ];
    // reads whose names the rules above miss
    const READ_NAMES: &[&str] = &["studio_wait", "proxy_test", "openrouter_verify", "project_transcript", "ui_screenshot", "ui_read_page", "ui_console", "editor_state", "url_probe",
        "project_google_tts_report",
    ];
    // writes over what was stored, so the earlier content is gone: a client asks first
    const OVERWRITES: &[&str] = &[
        "project_put", "project_analyze", "project_retranslate", "project_remix", "project_align", "segment_update", "segments_reorder", "segments_regen_all",
        "project_mode_set", "translation_target_set", "translation_style_set", "rewrite_set", "voice_set", "caption_style_set", "casting_update",
        "voice_slots_assign", "openrouter_set_key", "segments_merge", "editor_segment_update", "editor_segments_merge", "editor_mode", "editor_style",
        "segment_shorten", "take_select", "glossary_set", "series_glossary_set",
    ];
    let changes = CHANGES.iter().any(|verb| name.split('_').any(|word| word == *verb));
    let read_only = READ_NAMES.contains(&name) || !changes && READS.iter().any(|part| name.ends_with(part) || name.contains(&format!("{part}_")));
    let destructive = name.ends_with("_delete") || name.ends_with("_cancel") || name.contains("_cancel_") || name.ends_with("_delete_key") || OVERWRITES.contains(&name);
    // what reaches the internet: OpenRouter, Hugging Face, the sites of links and every download
    let open_world = name.starts_with("google_")
        && matches!(name, "google_set_key" | "google_models")
        || name.starts_with("openrouter_") && !matches!(name, "openrouter_status" | "openrouter_delete_key")
        || matches!(
            name,
            "models_download" | "voice_download" | "voices_download_pack" | "voices_catalog" | "proxy_test" | "url_probe" | "project_create_from_url" | "url_fetch_resume" | "url_tool_update"
        );
    let title = name.replace('_', " ");
    // a one-call tool on a file finds its project again and answers from the finished work
    let idempotent = read_only || name.ends_with("_set") || name.contains("_select") || name.ends_with("_file");
    json!({ "title": title, "readOnlyHint": read_only, "destructiveHint": destructive, "idempotentHint": idempotent, "openWorldHint": open_world })
}

// ---------------------------------------------------------------- origin and agent

struct Agent {
    /// When an agent last called the server, and what it called.
    last_call: Mutex<Option<(std::time::Instant, String)>>,
    calls: AtomicU64,
}

fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| Agent { last_call: Mutex::new(None), calls: AtomicU64::new(0),
    })
}

/// An agent that called within this long still counts as connected.
const AGENT_PRESENT: Duration = Duration::from_secs(600);

fn seen(what: &str) {
    *agent().last_call.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((std::time::Instant::now(), what.to_string()));
    agent().calls.fetch_add(1, Ordering::Relaxed);
}

fn agent_present() -> bool {
    agent().last_call.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().is_some_and(|(at, _)| at.elapsed() < AGENT_PRESENT)
}

/// Whether an agent is connected, for the settings page.
pub async fn status() -> Json<Value> {
    let last = agent().last_call.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    Json(json!({
        "agent_connected": agent_present(),
        "agent_last_call": last.as_ref().map(|(_, what)| what.clone()),
        "agent_seconds_ago": last.as_ref().map(|(at, _)| at.elapsed().as_secs()),
        "agent_calls": agent().calls.load(Ordering::Relaxed),
        "window_open": window::windows_open() > 0,
    }))
}

// ---------------------------------------------------------------- arguments

/// A required text argument, or a message saying which is missing.
fn text(args: &Value, name: &str) -> Result<String, String> {
    args.get(name).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string).ok_or_else(|| format!("'{name}' is required"))
}

/// A path segment, escaped.
pub(crate) fn segment(value: &str) -> String {
    value.bytes().map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) { (byte as char).to_string() } else { format!("%{byte:02X}") }
        }).collect()
}

/// The arguments without the ones that went into the path.
fn body_without(args: &Value, taken: &[&str]) -> Value {
    let mut body = args.as_object().cloned().unwrap_or_default();
    for name in taken {
        body.remove(*name);
    }
    Value::Object(body)
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn id_only(name: &str, what: &str) -> Value {
    object(json!({ name: { "type": "string", "description": what } }), &[name],
    )
}

fn nothing() -> Value {
    json!({ "type": "object", "additionalProperties": false })
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "video".into())
}

/// The project a tool acts on.
fn pid() -> Value {
    json!({ "type": "string", "description": "project id (projects_list, project_create)" })
}

fn project_only() -> Value {
    object(json!({ "pid": pid() }), &["pid"])
}

/// The route of a project, and what follows it.
fn project_path(args: &Value, rest: &str) -> Result<String, String> {
    Ok(format!("/projects/{}{rest}", segment(&text(args, "pid")?)))
}

/// A query string of the arguments given, in this order.
fn query(pairs: &[(&str, Option<String>)]) -> String {
    let given: Vec<String> = pairs.iter().filter_map(|(name, value)| {
            value.as_ref().map(|value| format!("{name}={}", segment(value)))
        }).collect();
    if given.is_empty() { String::new() } else { format!("?{}", given.join("&")) }
}

/// An argument as a query value: text as it is, a number, or a switch as 1 or 0.
fn given(args: &Value, name: &str) -> Option<String> {
    match args.get(name)? {
        Value::String(text) => Some(text.clone()),
        Value::Bool(on) => Some((if *on { "1" } else { "0" }).into()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// An edit of a project: PATCH /projects/{pid} with the op and its fields.
fn edit(args: &Value, op: &str) -> Result<Call, String> {
    let path = project_path(args, "")?;
    let mut body = body_without(args, &["pid", "response_format"]);
    body["op"] = op.into();
    send(Method::PATCH, path, body)
}

/// The detailed answer an edit can give instead of its summary.
fn detail() -> Value {
    json!({ "type": "string", "enum": ["concise", "detailed"], "description": "detailed: the whole project instead of what changed" })
}

/// Records of a glossary, as glossary_get answers them.
fn glossary_entries() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "term": { "type": "string" }, "translation": { "type": "string" }, "keep": { "type": "boolean" },
                "pronunciation": { "type": "string" }, "asr_fix": { "type": "array", "items": { "type": "string" } },
                "note": { "type": "string" }, "source": { "type": "string", "enum": ["manual", "auto", "series"] }, "lang": { "type": "string" },
            },
            "required": ["term"],
        },
    })
}

fn ids(what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": what })
}

fn idxs(what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "integer" }, "description": what })
}

/// Each edit of a project and the tool that makes it; the resource
/// studio://patch-ops lists them with their fields.
const PATCH_OPS: &[(&str, &str)] = &[
    ("segment", "segment_update"),
    ("add_segment", "segment_add"),
    ("del_segments", "segments_delete"),
    ("hide_segments", "segments_hide"),
    ("keep_segments", "segments_keep_original"),
    ("reorder_segments", "segments_reorder"),
    ("split_segment", "segment_split"),
    ("merge_segments", "segments_merge"),
    ("regen", "segment_regen"),
    ("regen_all", "segments_regen_all"),
    ("take_select", "take_select"),
    ("take_pin", "take_pin"),
    ("mode", "project_mode_set"),
    ("dub", "audio_output_set"),
    ("subs_content", "subtitles_content_set"),
    ("subs_burn", "subtitles_burn_set"),
    ("subpos", "subtitles_position_set"),
    ("translate", "translation_target_set"),
    ("translate_style", "translation_style_set"),
    ("rewrite", "rewrite_set"),
    ("recast", "voice_set"),
    ("gain", "gain_set"),
    ("loudness", "loudness_set"),
    ("voiceover_gain", "voiceover_gain_set"),
    ("keep_original", "original_track_set"),
    ("caption", "caption_style_set"),
    ("preset", "caption_preset_set"),
    ("title_add", "title_add"),
    ("title", "title_update"),
    ("del_titles", "titles_delete"),
    ("blur_add", "blur_add"),
    ("blur", "blur_update"),
    ("del_blurs", "blurs_delete"),
    ("blur_enable", "blur_enable"),
];

/// Ops for one thing that a tool of many already makes, and the op it makes:
/// sub_blur sets the same switch as blur_enable.
const PATCH_ALIASES: &[(&str, &str)] = &[
    ("del_segment", "del_segments"),
    ("hide_segment", "hide_segments"),
    ("keep_segment", "keep_segments"),
    ("title_del", "del_titles"),
    ("blur_del", "del_blurs"),
    ("sub_blur", "blur_enable"),
];

/// The tool that makes an op, or makes what an alias does.
fn tool_of_op(op: &str) -> Option<&'static str> {
    let op = PATCH_ALIASES.iter().find(|(alias, _)| *alias == op).map_or(op, |(_, same)| *same);
    PATCH_OPS.iter().find(|(known, _)| *known == op).map(|(_, tool)| *tool)
}

fn every_op() -> Vec<&'static str> {
    PATCH_OPS.iter().map(|(op, _)| *op).chain(PATCH_ALIASES.iter().map(|(op, _)| *op)).collect()
}

/// Every edit of a project: its op, its tool and the tool's fields.
fn edits() -> Value {
    let mut ops: Vec<Value> = PATCH_OPS
        .iter()
        .map(|(op, tool)| {
            let mut fields = tools().iter().find(|entry| entry.name == *tool).map(|entry| (entry.schema)()["properties"].clone()).unwrap_or(Value::Null);
            if let Some(fields) = fields.as_object_mut() {
                fields.remove("pid");
                fields.remove("response_format");
            }
            json!({ "op": op, "tool": tool, "fields": fields })
        })
        .collect();
    ops.extend(PATCH_ALIASES.iter().map(|(op, same)| json!({ "op": op, "same_as": same })),
    );
    json!({ "route": "PATCH /projects/{pid}", "body": "{ op, ...fields }", "answer": "the whole project, saved", "ops": ops })
}

/// The languages a project is dubbed from and into.
fn languages() -> Value {
    Value::Array(dub_translate::WHISPER_LANGS.iter().map(|(code, name)| json!({ "code": code, "name": name })).collect(),
    )
}

/// A resource by its uri: its type and its text.
fn resource(uri: &str) -> Option<(&'static str, String)> {
    match uri {
        SKILL_URI => Some(("text/markdown", SKILL.to_string())),
        LANGUAGES_URI => Some(("application/json", serde_json::to_string_pretty(&languages()).unwrap_or_default(),
        )),
        EDITS_URI => Some(("application/json", serde_json::to_string_pretty(&edits()).unwrap_or_default(),
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------- tools

fn tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        let mut all = vec![
            // ---------------------------------------------------------------- the studio
            Tool {
                name: "studio_status",
                description: "What the studio is doing now, in one short summary: the jobs running or waiting (analysis, voicing, render, another language, downloads) with their project, stage and percent, the last finished ones with their result or error, and whether the required models are there. Call it first, and use studio_wait to wait.",
                schema: nothing,
                call: |_| composite("status"),
            },
            Tool {
                name: "studio_wait",
                description: "Wait for work to finish instead of polling: a job (job_id, as project_analyze, project_dub_audio, project_render, project_export_lang, project_retranslate, project_remix, project_align, project_resume, voices_download_pack, segment_shorten, glossary_extract or a one-call tool still at work returned it, or the fetch.id of project_create_from_url), or until one kind of work is over - analyze, dub_audio, render, export_lang, retranslate, remix, align, shorten, download (models and downloads by link), voices_pack, separate, detect_text, glossary - or everything (until: idle, the default). Returns when it is done or after seconds (30 by default, at most 55, under the minute clients allow a call) with how far it got; call it again to keep waiting.",
                schema: || object(json!({ "job_id": { "type": "string" }, "until": { "type": "string", "enum": ["idle", "analyze", "dub_audio", "render", "export_lang", "retranslate", "remix", "align", "shorten", "download", "voices_pack", "separate", "detect_text", "glossary"] }, "seconds": { "type": "integer" } }), &[]),
                call: |_| composite("wait"),
            },
            Tool {
                name: "studio_system",
                description: "The graphics card, its used and free video memory, load, temperature and power, and the computer's and the studio's RAM: check them before a long job.",
                schema: nothing,
                call: |_| get("/hw/snapshot".into()),
            },
            Tool {
                name: "studio_capabilities",
                description: "What the studio can do here: the languages it dubs, the voice modes, whether ffmpeg is found, the speech recognisers with their models and precisions, the memory limits to choose from, and the current settings (the OpenRouter key only as or_key_set).",
                schema: nothing,
                call: |_| get("/engine/capabilities".into()),
            },
            // ---------------------------------------------------------------- jobs
            Tool {
                name: "jobs_list",
                description: "The studio's jobs, running, waiting and lately finished (preview frames are not jobs here): id, kind, project, status, stage, message, percent, place in the queue, and the result or the error. pid narrows them to one project and adds its last job as stored with it (project_job), an interrupted one included: project_resume continues it.",
                schema: || object(json!({ "pid": pid() }), &[]),
                call: |args| get(format!("/jobs{}", query(&[("pid", given(args, "pid"))]))),
            },
            Tool {
                name: "job_get",
                description: "One job: its status (queued, running, done, error, cancelled), kind, project, stage, message, percent, and its result (the output's path, a new project_id) or its error.",
                schema: || id_only("job_id", "job id a tool returned"),
                call: |args| get(format!("/jobs/{}", segment(&text(args, "job_id")?))),
            },
            Tool {
                name: "job_cancel",
                description: "Cancel a job: a waiting one never starts, a running one stops at its next step and ends as cancelled. project_resume starts it again later.",
                schema: || id_only("job_id", "job id a tool returned"),
                call: |args| post(format!("/jobs/{}/cancel", segment(&text(args, "job_id")?)), json!({})),
            },
            // ---------------------------------------------------------------- models and settings
            Tool {
                name: "models_status",
                description: "Every model and engine the studio uses: whether it is installed, its size, what is on disk, the video memory it takes, whether it is required, and whether the required set is ready (ready, driver_ok). Download what is missing with models_download.",
                schema: nothing,
                call: |_| get("/setup/status".into()),
            },
            Tool {
                name: "models_download",
                description: "Download components by id (from models_status): models, their quantisations, engines and runtimes. It runs in the background beside the jobs, not as a job: studio_wait until download waits for it, models_status shows it. Resumes what is partly there; every file is checked against its pinned SHA-256.",
                schema: || object(json!({ "ids": ids("component ids from models_status") }), &["ids"]),
                call: |args| post("/setup/download".into(), json!({ "ids": args.get("ids").cloned().unwrap_or_default() })),
            },
            Tool {
                name: "models_cancel_download",
                description: "Pause the running models download: what came is kept, and models_download with the same ids continues from there.",
                schema: nothing,
                call: |_| post("/setup/cancel".into(), json!({})),
            },
            Tool {
                name: "models_remove",
                description: "Delete downloaded components by id (from models_status) with what is partly downloaded, to free disk space; a stage whose chosen variant is removed falls back to an installed one. Answers what was removed, the bytes freed, the errors and the models' status.",
                schema: || object(json!({ "ids": ids("component ids from models_status") }), &["ids"]),
                call: |args| post("/setup/remove".into(), json!({ "ids": args.get("ids").cloned().unwrap_or_default() })),
            },
            Tool {
                name: "models_folder_open",
                description: "Open the models folder in Explorer, for the user (to put model files there by hand or see what takes the disk); answers its path.",
                schema: nothing,
                call: |_| post("/setup/open-models".into(), json!({})),
            },
            Tool {
                name: "models_import",
                description: "Take model files already on this computer instead of downloading them: every component found in the folder is copied into the studio's models; id limits it to one component. Answers what was imported and the models' status.",
                schema: || object(json!({ "path": { "type": "string", "description": "folder with the model files" }, "id": { "type": "string", "description": "one component id from models_status" } }), &["path"]),
                call: |args| {
                    let mut body = json!({ "path": text(args, "path")? });
                    if let Some(id) = args.get("id").and_then(Value::as_str) {
                        body["id"] = id.into();
                    }
                    post("/setup/import".into(), body)
                },
            },
            Tool {
                name: "models_select",
                description: "Use a downloaded variant for its stage (id from models_status): a quantisation of Higgs (higgs, higgs-q6_k, higgs-q4_k_m), of Gemma (gemma, gemma-q5_0, gemma-q6_k, gemma-q8_0) or of the separator (roformer, roformer-q5, roformer-q4), or a speech recogniser (parakeet, parakeet-fp32, parakeet-ultra, parakeet-ultra-int8, whisper-tiny ... whisper-large-v3-turbo). Applies from the next job, no restart.",
                schema: || id_only("id", "component id from models_status"),
                call: |args| post("/engine/select".into(), json!({ "id": text(args, "id")? })),
            },
            Tool {
                name: "settings_get",
                description: "The studio's settings and the values each takes: the model variant of each stage (tts, mt, sep, asr, asr_engine, whisper_model, whisper_compute, whisper_device), where each local stage runs (local_backend, sep_backend, diar_backend, asr_backend: auto, gpu, cpu), the voicing switches (qc_asr, qc_duration, multitake, breath_on, speech_rate_on, emo_ref_on, duck_on, auto_shorten: rewrite the translation of lines that do not fit their slot and voice them again during a render), the memory limits (llama_ubatch, higgs_ref_secs), who translates and who reads frames (llm_provider, vision_provider: local = the studio's Gemma, server = a local OpenAI-compatible server at srv_url with the models srv_llm and srv_vision, openrouter = the cloud with or_llm and or_vision), the other cloud stages through OpenRouter (or_tts_on, or_tts_model, or_tts_voice, or_tts_autocast, or_asr_on, or_asr, or_concurrency), whether the proxy is on (proxy_on; proxy_settings_set sets its address) and the stage benchmark (bench). The OpenRouter key is never shown: or_key_set says whether there is one.",
                schema: nothing,
                call: |_| get("/engine/capabilities".into()),
            },
            Tool {
                name: "launch_defaults_get",
                description: "What the start screen's dubbing form opens with, shared by every window: audio (nodub, dub, voiceover, transcribe), subs (none, transcribe, translate, bilingual: the translation with the original as a second line), burn, detect_text, src_lang, tgt_lang (null: the window's language), casting, casting_ref, content_type, vo_gain_db (-24..0), tr_style, tr_style_custom, sub_blur, keep_orig, container (mp4, mkv), voice_src (clone, library), voice_slots_m, voice_slots_f; saved says whether they were ever changed.",
                schema: nothing,
                call: |_| get("/settings/launch".into()),
            },
            Tool {
                name: "launch_defaults_set",
                description: "Change some of the start screen's defaults (fields as launch_defaults_get names them); an unknown field or a wrong value changes nothing and says why.",
                schema: || object(json!({ "defaults": { "type": "object", "description": "the fields to change and their values" } }), &["defaults"]),
                call: |args| match args.get("defaults") {
                    Some(Value::Object(fields)) => send(Method::PATCH, "/settings/launch".into(), Value::Object(fields.clone())),
                    _ => Err("'defaults' is required: an object of the fields to change".into()),
                },
            },
            Tool {
                name: "studio_paths",
                description: "Where the studio keeps its data on this computer: data_dir, projects_dir (a folder per project), models_dir.",
                schema: nothing,
                call: |_| get("/app/paths".into()),
            },
            Tool {
                name: "settings_set",
                description: "Change one setting to a text value. TTS: tts_provider=local|openrouter|google, google_tts_model=<id without models/>, google_tts_mode=standard|batch. Voice/autocast use or_tts_voice/or_tts_autocast, parallel requests use or_concurrency (1..16). Applies from the next job. Secrets: openrouter_set_key, google_set_key; proxy: proxy_settings_set.",
                schema: || object(json!({ "key": { "type": "string" }, "value": { "type": "string" } }), &["key", "value"]),
                call: |args| {
                    let key = text(args, "key")?;
                    if key == "google_key" {
                        return Err("The Google key is set with google_set_key.".into());
                    }
                    if key == "or_key" {
                        return Err("The OpenRouter key is set with openrouter_set_key.".into());
                    }
                    if key == "proxy_url" {
                        return Err("The proxy's address is set with proxy_settings_set.".into());
                    }
                    let value = match args.get("value") {
                        Some(Value::String(value)) => value.clone(),
                        Some(Value::Bool(on)) => (if *on { "1" } else { "0" }).to_string(),
                        Some(Value::Number(number)) => number.to_string(),
                        _ => return Err("'value' is required".into()),
                    };
                    post("/engine/select".into(), json!({ "key": key, "value": value }))
                },
            },
            Tool {
                name: "engine_presets_get",
                description: "The hardware presets (by graphics card, a weak card with the cloud, the cloud alone), this computer's card, video memory and RAM, and the preset recommended for it.",
                schema: nothing,
                call: |_| get("/engine/presets".into()),
            },
            Tool {
                name: "engine_preset_apply",
                description: "Apply a hardware preset (id from engine_presets_get): it sets the quantisations and where each stage runs, and the cloud stages for the cloud presets. Answers the settings it changed.",
                schema: || id_only("id", "preset id from engine_presets_get"),
                call: |args| post("/engine/preset".into(), json!({ "id": text(args, "id")? })),
            },
            Tool {
                name: "proxy_test",
                description: "Check, before saving, whether Hugging Face (model downloads) and OpenRouter (the cloud stages) are reachable: mode custom (default) through the proxy at url - any seller's notation, host:port:user:password included, kind giving the scheme of an address without one - mode system through the Windows proxy, mode off directly. password goes with a url that names the user without one; the stored password is used only for the stored address.",
                schema: || {
                    object(
                        json!({
                            "mode": { "type": "string", "enum": ["custom", "system", "off"] },
                            "kind": { "type": "string", "enum": ["http", "https", "socks5", "socks4"] },
                            "url": { "type": "string" },
                            "password": { "type": "string" },
                        }),
                        &[],
                    )
                },
                call: |args| {
                    let mut body = json!({ "url": args.get("url").and_then(Value::as_str).unwrap_or_default() });
                    for field in ["mode", "kind", "password"] {
                        if let Some(value) = args.get(field).and_then(Value::as_str) {
                            body[field] = value.into();
                        }
                    }
                    post("/engine/proxy/test".into(), body)
                },
            },
            Tool {
                name: "proxy_settings_get",
                description: "The proxy all the studio's traffic goes through (model downloads, OpenRouter): mode (system - as in Windows, custom - its own address, off - direct), kind (the scheme an address without one gets), its address without the password, whether a password is stored, and problem - why a stored address of its own cannot be read.",
                schema: nothing,
                call: |_| get("/engine/proxy/settings".into()),
            },
            Tool {
                name: "proxy_settings_set",
                description: "Set the proxy: mode system (as in Windows), custom (the address in url) or off (direct); kind (http, https, socks5, socks4) for an address written without a scheme; url in any seller's notation (user@host:port, host:port:user:password; a password written into it is stored apart and never shown; empty removes the address); password to change only the password (null removes it). Fields left out stay; mode custom needs an address. proxy_test checks first.",
                schema: || {
                    object(
                        json!({
                            "mode": { "type": "string", "enum": ["system", "custom", "off"] },
                            "kind": { "type": "string", "enum": ["http", "https", "socks5", "socks4"] },
                            "url": { "type": "string" },
                            "password": { "type": ["string", "null"] },
                        }),
                        &[],
                    )
                },
                call: |args| send(Method::PUT, "/engine/proxy/settings".into(), body_without(args, &[])),
            },
            Tool {
                name: "fonts_list",
                description: "The fonts subtitles and titles can use (caption_style_set and title_add take the name).",
                schema: nothing,
                call: |_| get("/fonts".into()),
            },
            Tool {
                name: "caption_presets_list",
                description: "The subtitle style presets (caption_preset_set takes the name) with their settings, and the reveal animations.",
                schema: nothing,
                call: |_| get("/presets".into()),
            },
            // ---------------------------------------------------------------- OpenRouter
            Tool {
                name: "google_status",
                description: "Whether a direct Google Gemini API key is configured and its source. Never returns the key.",
                schema: nothing,
                call: |_| get("/engine/google/settings".into()),
            },
            Tool {
                name: "google_set_key",
                description: "Verify a Gemini API key against Google's model catalog and store it separately from settings. No paid generation is made.",
                schema: || id_only("key", "Google Gemini API key"),
                call: |args| send(Method::PUT,"/engine/google/settings".into(),json!({"api_key":text(args,"key")?})),
            },
            Tool {
                name: "google_delete_key",
                description: "Remove the stored Google Gemini API key. An environment key cannot be removed by the app.",
                schema: nothing,
                call: |_| send(Method::DELETE,"/engine/google/settings".into(),json!({})),
            },
            Tool {
                name: "google_models",
                description: "List TTS models available through the direct Google API, including supported generation methods. Needs the Google key.",
                schema: nothing,
                call: |_| get("/engine/google/models".into()),
            },
            Tool {
                name: "project_google_tts_report",
                description: "Direct Google TTS usage, generated audio duration, wall time, tariff-based cost estimate (not invoice), and persisted Batch names/states for a project. settings_set tts_provider=google, google_tts_model=<id without models/>, google_tts_mode=standard|batch; project_dub_audio or project_render starts the normal pipeline. A local job cancellation pauses polling; the remote Batch continues and resume uses its saved name without submitting again.",
                schema: || id_only("pid","project id"),
                call: |args| get(format!("/projects/{}/google-tts",text(args,"pid")?)),
            },
            Tool {
                name: "openrouter_status",
                description: "Whether an OpenRouter key is set for the cloud stages and where it comes from (the studio's store or an environment variable). The key itself is never shown.",
                schema: nothing,
                call: |_| get("/engine/openrouter/settings".into()),
            },
            Tool {
                name: "openrouter_set_key",
                description: "Set the OpenRouter key the cloud stages use: OpenRouter checks it first and it is stored only if it works. It is never shown back.",
                schema: || id_only("key", "the OpenRouter API key, sk-or-..."),
                call: |args| send(Method::PUT, "/engine/openrouter/settings".into(), json!({ "api_key": text(args, "key")? })),
            },
            Tool {
                name: "openrouter_delete_key",
                description: "Remove the stored OpenRouter key; the cloud stages stop until a key is set again.",
                schema: nothing,
                call: |_| send(Method::DELETE, "/engine/openrouter/settings".into(), json!({})),
            },
            Tool {
                name: "openrouter_verify",
                description: "Check an OpenRouter key without storing it: whether it works, its label, limit and usage.",
                schema: || id_only("key", "the OpenRouter API key, sk-or-..."),
                call: |args| post("/engine/openrouter/verify".into(), json!({ "key": text(args, "key")? })),
            },
            Tool {
                name: "openrouter_models",
                description: "The OpenRouter models for a cloud stage: llm (translation), vision (reading frames), tts (voices), asr (speech recognition). Needs the key.",
                schema: || object(json!({ "kind": { "type": "string", "enum": ["llm", "vision", "tts", "asr"] } }), &["kind"]),
                call: |args| get(format!("/engine/openrouter/models{}", query(&[("kind", Some(text(args, "kind")?))]))),
            },
            Tool {
                name: "openrouter_catalog",
                description: "The state of the studio's OpenRouter model catalog (kept on disk): when it was fetched and how many models each cloud stage can choose from (llm, vision, tts, asr); openrouter_models lists them with prices and context.",
                schema: nothing,
                call: |_| get("/engine/openrouter/catalog".into()),
            },
            Tool {
                name: "openrouter_catalog_refresh",
                description: "Fetch the OpenRouter model catalog again now instead of the copy on disk.",
                schema: nothing,
                call: |_| post("/engine/openrouter/catalog/refresh".into(), json!({})),
            },
            Tool {
                name: "local_server_models",
                description: "The models of the local OpenAI-compatible server (Ollama, LM Studio, vLLM, llama-server) for translation and vision: url is its address (default: the one in settings); a stored key goes only to the address it was saved for.",
                schema: || object(json!({ "url": { "type": "string" } }), &[]),
                call: |args| get(format!("/engine/server/models{}", query(&[("url", given(args, "url"))]))),
            },
            Tool {
                name: "local_server_key_status",
                description: "Whether an API key is stored for the local server's address (url, default: the one in settings); the key itself is never shown.",
                schema: || object(json!({ "url": { "type": "string" } }), &[]),
                call: |args| get(format!("/engine/server/key{}", query(&[("url", given(args, "url"))]))),
            },
            Tool {
                name: "local_server_key_set",
                description: "Store an API key for the local server (vLLM and LM Studio can ask for one); it is sent only to that address (url, default: the one in settings).",
                schema: || object(json!({ "api_key": { "type": "string" }, "url": { "type": "string" } }), &["api_key"]),
                call: |args| {
                    let mut body = json!({ "api_key": text(args, "api_key")? });
                    if let Some(url) = given(args, "url") {
                        body["url"] = url.into();
                    }
                    send(Method::PUT, "/engine/server/key".into(), body)
                },
            },
            Tool {
                name: "local_server_key_delete",
                description: "Remove the API key stored for the local server's address (url, default: the one in settings).",
                schema: || object(json!({ "url": { "type": "string" } }), &[]),
                call: |args| send(Method::DELETE, format!("/engine/server/key{}", query(&[("url", given(args, "url"))])), json!({})),
            },
            Tool {
                name: "openrouter_voices",
                description: "The voices of an OpenRouter speech model with their gender and age, and whether it speaks Russian: for or_tts_voice and the characters' voices.",
                schema: || id_only("model", "OpenRouter TTS model id"),
                call: |args| get(format!("/engine/openrouter/voices{}", query(&[("model", Some(text(args, "model")?))]))),
            },
            // ---------------------------------------------------------------- projects
            Tool {
                name: "projects_list",
                description: "The projects, the last edited first: pid, the video's name, target language, mode, size, length, number of lines, whether it is rendered (done), and source (agent: made of a file by a tool, window: by the studio's window). query matches the video's name; since and until (today, yesterday, 2026-09-26, 2026-09-26T18:00) bound when it was last edited.",
                schema: || object(json!({ "query": { "type": "string" }, "since": { "type": "string" }, "until": { "type": "string" } }), &[]),
                call: |_| get("/projects".into()),
            },
            Tool {
                name: "project_create",
                description: "Make a project of a video or audio file on this computer (path): the studio copies it into its workspace. subtitles_path adds an .srt, .ass or .ssa file: the analysis then takes its text and timing instead of recognising speech. Answers the project_id; project_analyze is next.",
                schema: || object(json!({ "path": { "type": "string", "description": "video or audio file" }, "subtitles_path": { "type": "string", "description": ".srt, .ass or .ssa file" } }), &["path"]),
                call: |args| {
                    let video = PathBuf::from(text(args, "path")?);
                    if !video.is_file() {
                        return Err(format!("{} is not a file on this computer.", video.display()));
                    }
                    let name = file_name(&video);
                    let mut files = vec![("file".to_string(), video, name)];
                    if let Some(subtitles) = args.get("subtitles_path").and_then(Value::as_str).filter(|path| !path.trim().is_empty()) {
                        let subtitles = PathBuf::from(subtitles.trim());
                        let kind = subtitles.extension().and_then(|value| value.to_str()).unwrap_or_default().to_lowercase();
                        if !["srt", "ass", "ssa"].contains(&kind.as_str()) {
                            return Err(format!("{} is not .srt, .ass or .ssa: the studio tells subtitles from the video by that extension.", subtitles.display()));
                        }
                        if !subtitles.is_file() {
                            return Err(format!("{} is not a file on this computer.", subtitles.display()));
                        }
                        let name = file_name(&subtitles);
                        files.push(("subs".to_string(), subtitles, name));
                    }
                    Ok(Call { method: Method::POST, path: "/projects".into(), payload: Payload::Form { fields: Vec::new(), files } })
                },
            },
            // ---------------------------------------------------------------- videos by link
            Tool {
                name: "url_probe",
                description: "Look at a video link before downloading it (YouTube and the other sites yt-dlp knows): title, length, uploader, the highest height, the qualities worth offering, the site's subtitles made by people (subtitles, each with lang and name) apart from the languages of the automatic ones (auto_subtitles), the expected size and the preview's link (thumbnail). cookies is the path of a cookies.txt for videos that need a signed-in browser (age, members, bot checks). A refusal names its code and what to do (hint). Needs the component ytdlp (models_download).",
                schema: || object(json!({ "url": { "type": "string", "description": "link of one video" }, "cookies": { "type": "string", "description": "path of a Netscape cookies.txt" } }), &["url"]),
                call: |args| get(format!("/url/probe{}", query(&[("url", Some(text(args, "url")?)), ("cookies", given(args, "cookies"))]))),
            },
            Tool {
                name: "project_create_from_url",
                description: "Download a video by its link into a new project, in the background beside the jobs (not as a graphics-card job): quality best, 1080, 720, 480 or audio; subs_lang takes the site's subtitles made by people in that language (a lang of url_probe's subtitles) as the project's imported subtitles; cookies is the path of a cookies.txt. Answers the download (fetch.id): studio_wait with that id as job_id, or until download; when it is done its result names the project_id, and project_analyze is next. The user answers for their right to the content.",
                schema: || {
                    object(
                        json!({
                            "url": { "type": "string", "description": "link of one video" },
                            "quality": { "type": "string", "enum": ["best", "1080", "720", "480", "audio"] },
                            "subs_lang": { "type": "string", "description": "language of the site's subtitles made by people, from url_probe" },
                            "cookies": { "type": "string", "description": "path of a Netscape cookies.txt" },
                        }),
                        &["url"],
                    )
                },
                call: |args| post("/projects/from_url".into(), body_without(args, &["response_format"])),
            },
            Tool {
                name: "url_fetches_list",
                description: "The downloads by link, newest first: id, link, quality, status (downloading, completed, failed, cancelled, interrupted), phase, bytes, speed, the project it made (pid), a warning about the subtitles, and the error with its code and hint.",
                schema: nothing,
                call: |_| get("/url/fetches".into()),
            },
            Tool {
                name: "url_fetch_get",
                description: "One download by link, as url_fetches_list shows it.",
                schema: || id_only("id", "download id from project_create_from_url or url_fetches_list"),
                call: |args| get(format!("/url/fetches/{}", segment(&text(args, "id")?))),
            },
            Tool {
                name: "url_fetch_cancel",
                description: "Stop a download by link: yt-dlp and what it started are stopped and the part downloaded is deleted.",
                schema: || id_only("id", "download id"),
                call: |args| post(format!("/url/fetches/{}/cancel", segment(&text(args, "id")?)), json!({})),
            },
            Tool {
                name: "url_fetch_resume",
                description: "Continue an interrupted or failed download by link from where it stopped, with the same settings (after fixing what its hint said: a proxy, cookies, an update of yt-dlp).",
                schema: || id_only("id", "download id"),
                call: |args| post(format!("/url/fetches/{}/resume", segment(&text(args, "id")?)), json!({})),
            },
            Tool {
                name: "url_fetch_delete",
                description: "Remove a finished, failed or interrupted download from the list, with what it downloaded partly; the project it made stays.",
                schema: || id_only("id", "download id"),
                call: |args| send(Method::DELETE, format!("/url/fetches/{}", segment(&text(args, "id")?)), json!({})),
            },
            Tool {
                name: "url_tool_status",
                description: "The yt-dlp that downloads by link: whether the component is there, the version in use, the pinned one, the latest release and when it was checked, whether an update runs, and why the last one failed.",
                schema: nothing,
                call: |_| get("/url/tool".into()),
            },
            Tool {
                name: "url_tool_update",
                description: "Check for a newer yt-dlp now (it is checked once a day anyway) and install it beside the pinned one: it is used only after its SHA-256 matches its release and it runs; otherwise the one in use stays. Runs in the background: url_tool_status shows when it is over.",
                schema: nothing,
                call: |_| post("/url/tool/update".into(), json!({})),
            },
            Tool {
                name: "project_get",
                description: "A project: mode, target language, subtitles, audio and voices, subtitle style, titles and blur boxes (with the idx their tools take), how many lines there are and how many are changed (dirty, voiced again at the next project_dub_audio or project_render), and its lines - id, start, end, speaker, the recognised text (src_text), the translation (tgt_text), dirty, hidden, keep_original, tts_text (what the voice says when it differs from tgt_text: sound tags, speaker labels and markup taken out, the glossary's pronunciation) and tts_skip (the line is not voiced, nothing is left to say: sound_only or no_words). A dubbed or voiced-over project's voiced lines also carry fit - whether the translation fits its time slot: est (seconds at the voice's pace), slot, ratio, verdict (fits; tight: the render speeds it up within eff_cap, up to 4x with speech_rate_on, else the natural cap; impossible), calibrated (the pace measured from this voice's clips, else the language's), over, and rendered (needed, cap, eff_cap, raw, dur; over when needed > eff_cap) when the last render voiced this very text - takes (count, active, pinned, selected) and shortened (from, to). from and to (seconds) or ids narrow the lines. response_format detailed returns the whole project as stored, word timings and the glossary included, with fit, takes, tts_text and tts_skip worked out on reading: take it from there for project_put, which drops them.",
                schema: || object(json!({ "pid": pid(), "response_format": { "type": "string", "enum": ["concise", "detailed"] }, "from": { "type": "number" }, "to": { "type": "number" }, "ids": ids("line ids") }), &["pid"]),
                call: |args| get(project_path(args, "")?),
            },
            Tool {
                name: "project_transcript",
                description: "A project's transcript without the window: each line's id, start, end, speaker and text - the recognised original (text src, the default) or the translation as the subtitles show it (text tgt: a line's own subtitle text over its translation, lines that keep the original speech left out) - hidden lines left out, from and to (seconds) narrowing it; format text gives one \"[0:14.2 SPK 1] words\" line each instead of JSON lines.",
                schema: || object(json!({ "pid": pid(), "text": { "type": "string", "enum": ["src", "tgt"] }, "format": { "type": "string", "enum": ["json", "text"] }, "from": { "type": "number" }, "to": { "type": "number" } }), &["pid"]),
                call: |args| get(project_path(args, "")?),
            },
            Tool {
                name: "project_analyze",
                description: "Analyze a project's video: separate the voices from the background, find who speaks, recognise the speech, translate it into tgt_lang, read the on-screen text (detect) and style the subtitles after the original. A job: studio_wait with its job_id, then project_get shows the lines. mode: auto (dub when there is speech), dub, voiceover (the translation over the quieted original), nodub (subtitles only), transcribe (the transcript in the original language). src_lang: auto or a code (studio://languages). subs: auto, none, transcribe, translate, bilingual (the translation with the original line beside it). burn: burn the subtitles into the video (default on). rewrite: an instruction for a funny or themed version of the dub. translate_style: the tone of the translation. casting: find the characters by voice and face (content_type real or anime, auto guesses), casting_ref applies a saved casting (casting_library_list). import_translated: the subtitles given to project_create are already in tgt_lang. Analyzing again replaces the project's lines.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(),
                            "tgt_lang": { "type": "string", "description": "language code to dub into (studio://languages); not needed for mode transcribe" },
                            "mode": { "type": "string", "enum": ["auto", "dub", "voiceover", "nodub", "transcribe"] },
                            "src_lang": { "type": "string" },
                            "speaker_count": { "type": "integer", "minimum": 0, "maximum": 8, "description": "the expected number of speakers in the whole recording; 0 means automatic. Voices are matched across fragments with WeSpeaker." },
                            "subs": { "type": "string", "enum": ["auto", "none", "transcribe", "translate", "bilingual"] },
                            "burn": { "type": "boolean" },
                            "detect": { "type": "boolean", "description": "read on-screen text to blur and translate it (default on)" },
                            "rewrite": { "type": "string" },
                            "translate_style": { "type": "string" },
                            "casting": { "type": "boolean" },
                            "casting_ref": { "type": "string" },
                            "content_type": { "type": "string", "enum": ["auto", "real", "anime"] },
                            "import_translated": { "type": "boolean" },
                            "align_subs": { "type": "boolean", "description": "subtitles given to project_create in the original language: align their timing to the recognised speech" },
                        }),
                        &["pid"],
                    )
                },
                call: |args| {
                    let path = project_path(args, "/analyze")?;
                    let tgt_lang = if given(args, "mode").as_deref() == Some("transcribe") { given(args, "tgt_lang") } else { Some(text(args, "tgt_lang")?) };
                    let tail = query(&[
                        ("tgt_lang", tgt_lang),
                        ("mode", given(args, "mode")),
                        ("src_lang", given(args, "src_lang")),
                        ("speaker_count", given(args, "speaker_count")),
                        ("subs", given(args, "subs")),
                        ("rewrite", given(args, "rewrite")),
                        ("translate_style", given(args, "translate_style")),
                        ("burn", given(args, "burn")),
                        ("detect", given(args, "detect")),
                        ("casting", given(args, "casting")),
                        ("casting_ref", given(args, "casting_ref")),
                        ("content_type", given(args, "content_type")),
                        ("import_translated", given(args, "import_translated")),
                        ("align_subs", given(args, "align_subs")),
                    ]);
                    post(format!("{path}{tail}"), json!({}))
                },
            },
            Tool {
                name: "project_resume",
                description: "Start a project's interrupted or failed job again where it stopped, with the same settings, keeping what was already made. A job: studio_wait with its job_id.",
                schema: project_only,
                call: |args| post(project_path(args, "/resume")?, json!({})),
            },
            Tool {
                name: "project_retranslate",
                description: "Translate the project's recognised lines into lang without recognising the speech again, and switch it to mode: dub (the default), voiceover or nodub (subtitles). Titles are translated too. A job; the lines become dirty and are voiced at the next project_dub_audio or project_render. Without a translation model it fails and the project stays as it was; other edits made while it runs are kept, but if the lines or titles themselves change meanwhile it fails and changes nothing.",
                schema: || object(json!({ "pid": pid(), "lang": { "type": "string" }, "mode": { "type": "string", "enum": ["dub", "voiceover", "nodub"] } }), &["pid", "lang"]),
                call: |args| {
                    let path = project_path(args, "/retranslate")?;
                    post(format!("{path}{}", query(&[("lang", Some(text(args, "lang")?)), ("mode", given(args, "mode"))])), json!({}))
                },
            },
            Tool {
                name: "project_remix",
                description: "Rewrite every line of the dub by an instruction - a theme, a funny version - through the translation model. A job; every line becomes dirty. Other edits made while it runs are kept, but if the lines themselves change meanwhile it fails and changes nothing.",
                schema: || object(json!({ "pid": pid(), "instruction": { "type": "string" } }), &["pid", "instruction"]),
                call: |args| {
                    let path = project_path(args, "/remix")?;
                    post(format!("{path}{}", query(&[("instruction", Some(text(args, "instruction")?))])), json!({}))
                },
            },
            Tool {
                name: "project_align",
                description: "Align the lines' timing to the recognised speech of the original voice word by word (like the align option of project_create with subtitles): it holds a frame rate drift and cut pieces, lines that did not match move with their neighbours; the moved lines become dirty. A job: studio_wait with its job_id. Refused when the lines have no original-language text.",
                schema: project_only,
                call: |args| post(project_path(args, "/align")?, json!({})),
            },
            Tool {
                name: "project_dub_audio",
                description: "Voice the dub without making the video: the dirty lines (all of them the first time) are synthesized with each speaker's voice and mixed into dub_audio.m4a. A job: studio_wait with its job_id. Quicker than a render to check the voices.",
                schema: project_only,
                call: |args| post(project_path(args, "/dub-audio")?, json!({})),
            },
            Tool {
                name: "project_render",
                description: "Make the finished video: voice the dirty lines, mix the dub over the background, burn in the subtitles and titles, blur, and write output.mp4 (output.mkv with the original track kept, output.wav for audio). A job: studio_wait with its job_id; project_save_output then copies the result where the user wants it.",
                schema: project_only,
                call: |args| post(project_path(args, "/render")?, json!({})),
            },
            Tool {
                name: "project_export_lang",
                description: "The same video in another language: a copy of the project keeps its layout, subtitle style, titles, blur and cloned voices, its lines and titles are translated into lang, and it is rendered. Answers the new project_id and the job_id; studio_wait with the job_id. One call per language, one after another.",
                schema: || object(json!({ "pid": pid(), "lang": { "type": "string" } }), &["pid", "lang"]),
                call: |args| {
                    let path = project_path(args, "/export-lang")?;
                    post(format!("{path}{}", query(&[("lang", Some(text(args, "lang")?))])), json!({}))
                },
            },
            Tool {
                name: "project_put",
                description: "Replace the whole project with project: the object project_get with response_format detailed returns, changed (its glossary stays as it is - glossary_set changes it). Whatever it leaves out is lost - word timings, hidden lines, per-line subtitle overrides - and a long project does not fit into one answer, so change lines with segment_update and the other edits instead.",
                schema: || object(json!({ "pid": pid(), "project": { "type": "object" }, "response_format": detail() }), &["pid", "project"]),
                call: |args| send(Method::PUT, project_path(args, "")?, args.get("project").cloned().unwrap_or_default()),
            },
            Tool {
                name: "project_patch",
                description: "Apply one edit by its op, with the op's fields. Every op has its own tool (the resource studio://patch-ops lists them with their fields); use this one for an op by name.",
                schema: || json!({ "type": "object", "properties": { "pid": pid(), "op": { "type": "string", "enum": every_op() }, "response_format": detail() }, "required": ["pid", "op"], "additionalProperties": true }),
                call: |args| {
                    let op = text(args, "op")?;
                    let path = project_path(args, "")?;
                    let mut body = body_without(args, &["pid", "response_format"]);
                    body["op"] = op.into();
                    send(Method::PATCH, path, body)
                },
            },
            Tool {
                name: "project_delete",
                description: "Delete a project and everything the studio made for it; the file it was made from stays where it is.",
                schema: project_only,
                call: |args| send(Method::DELETE, project_path(args, "")?, json!({})),
            },
            Tool {
                name: "project_waveform",
                description: "The loudness of the project's audio as n peaks (600 by default), to find speech and silence.",
                schema: || object(json!({ "pid": pid(), "n": { "type": "integer" } }), &["pid"]),
                call: |args| get(format!("{}{}", project_path(args, "/waveform")?, query(&[("n", given(args, "n"))]))),
            },
            Tool {
                name: "project_frame",
                description: "A picture of the project at t seconds: source dub (the default) is the finished frame - the translated subtitles, titles and blur as the render burns them in; source original is the video's own frame. lowres makes a large video's frame smaller. Use it to see what an edit looks like. Frames are made on the graphics card, so while a job runs this answers that it is busy: studio_wait until idle first.",
                schema: || object(json!({ "pid": pid(), "t": { "type": "number", "description": "seconds" }, "source": { "type": "string", "enum": ["dub", "original"] }, "lowres": { "type": "boolean" } }), &["pid", "t"]),
                call: |args| {
                    let at = args.get("t").and_then(Value::as_f64).ok_or("'t' is required (seconds)")?;
                    match args.get("source").and_then(Value::as_str).unwrap_or("dub") {
                        "dub" => get(format!("{}{}", project_path(args, "/preview")?, query(&[("t", Some(at.to_string())), ("lr", given(args, "lowres"))]))),
                        "original" => get(format!("{}{}", project_path(args, "/original")?, query(&[("t", Some(at.to_string()))]))),
                        other => Err(format!("source is dub or original, not {other}")),
                    }
                },
            },
            Tool {
                name: "project_files",
                description: "Where the project's files are on this computer: the folder, the video it was made from, the finished output and its playable copy, the dubbed audio, project.json, casting.json, the separated voice and background (vocals, background) and the subtitle and text files written.",
                schema: project_only,
                call: |args| get(project_path(args, "/files")?),
            },
            Tool {
                name: "project_export_text",
                description: "Write the project's lines as a text file, as the window's export buttons do: format srt (numbered subtitles with timing), vtt (WebVTT, each cue naming its speaker), ass (styled as the render burns them), txt (one line per phrase with its speaker) or json (each line with its timing, speaker, text, the original under a translation and the word timings of the transcript); text tgt (the translation, the recognised text where a line has none), src (the recognised original: the transcript) or both (bilingual srt, vtt or ass: the translation and the original as two lines of each subtitle; order translation_top or original_top, the project's own when left out). Without dir the file goes into the project's own folder under the fixed name of its kind (subtitles.srt, transcript.srt, bilingual.srt, subtitles.vtt, subtitles.ass, translation.txt, transcript.lines.json and so on), replacing the earlier one; a name of your own needs dir, a folder on this computer, where a name already there gets (2), (3). Answers the path.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(),
                            "format": { "type": "string", "enum": ["srt", "vtt", "ass", "txt", "json"] },
                            "text": { "type": "string", "enum": ["tgt", "src", "both"] },
                            "order": { "type": "string", "enum": ["translation_top", "original_top"], "description": "for text both; the project's own order when left out" },
                            "dir": { "type": "string", "description": "folder on this computer" },
                            "name": { "type": "string", "description": "the file's name, only together with dir" },
                            "speaker_label": { "type": "string", "description": "the word before each speaker's number in txt, Speaker by default" },
                        }),
                        &["pid", "format"],
                    )
                },
                call: |args| {
                    let path = project_path(args, "/export-text")?;
                    post(path, body_without(args, &["pid"]))
                },
            },
            Tool {
                name: "project_save_output",
                description: "Copy the finished video (after project_render) into a folder on this computer as name, with the output's extension (mkv before mp4 when there is one); a name already there gets (2), (3). Answers the path.",
                schema: || object(json!({ "pid": pid(), "dir": { "type": "string", "description": "folder on this computer" }, "name": { "type": "string", "description": "the file's name, the video's own name for example" } }), &["pid", "dir"]),
                call: |args| {
                    let path = project_path(args, "/save-output")?;
                    let mut body = json!({ "dir": text(args, "dir")? });
                    if let Some(name) = args.get("name").and_then(Value::as_str).filter(|name| !name.trim().is_empty()) {
                        body["name"] = name.into();
                    }
                    post(path, body)
                },
            },
            Tool {
                name: "project_open_output",
                description: "Open the finished video in the computer's own player, for the user to watch.",
                schema: project_only,
                call: |args| post(project_path(args, "/open")?, json!({})),
            },
            Tool {
                name: "project_save_text",
                description: "Write a text file into the project's folder and show it selected in Explorer, for the user: name (letters, digits, dot, dash and underscore, e.g. glossary.tsv) and text - a glossary from glossary_get with format tsv, notes or a script. Answers the file's path.",
                schema: || object(json!({ "pid": pid(), "name": { "type": "string" }, "text": { "type": "string" } }), &["pid", "name", "text"]),
                call: |args| {
                    let path = project_path(args, "/save-text")?;
                    post(path, json!({ "name": text(args, "name")?, "text": args.get("text").and_then(Value::as_str).unwrap_or("") }))
                },
            },
            Tool {
                name: "project_reveal",
                description: "Show one of the project's files (name, output.mp4 by default) selected in Explorer, for the user.",
                schema: || object(json!({ "pid": pid(), "name": { "type": "string" } }), &["pid"]),
                call: |args| {
                    let path = project_path(args, "/reveal")?;
                    post(path, json!({ "name": args.get("name").and_then(Value::as_str).unwrap_or("output.mp4") }))
                },
            },
            // ---------------------------------------------------------------- the lines
            Tool {
                name: "segment_update",
                description: "Edit one line (id from project_get): its translation (tgt_text), its recognised text (src_text), its start and end in seconds, its speaker (another speaker's id, or a new one for a new voice; \"\" takes it away), hidden (neither voiced nor subtitled), keep_original (the original voice plays there). The line becomes dirty: project_dub_audio or project_render voices it again.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(), "id": { "type": "string" }, "tgt_text": { "type": "string" }, "src_text": { "type": "string" },
                            "start": { "type": "number" }, "end": { "type": "number" }, "speaker": { "type": "string" },
                            "hidden": { "type": "boolean" }, "keep_original": { "type": "boolean" }, "response_format": detail(),
                        }),
                        &["pid", "id"],
                    )
                },
                call: |args| {
                    text(args, "id")?;
                    edit(args, "segment")
                },
            },
            Tool {
                name: "segment_add",
                description: "Add a line of your own at start seconds (end: 2 s later by default) for a speaker (the first line's when left out): it is voiced with that speaker's voice and shown as a subtitle. tgt_text is what it says; id is made up when left out, and one whose letters, digits and underscores are another line's is taken (409): they name the line's files.",
                schema: || object(json!({ "pid": pid(), "start": { "type": "number" }, "end": { "type": "number" }, "speaker": { "type": "string" }, "tgt_text": { "type": "string" }, "id": { "type": "string" }, "response_format": detail() }), &["pid", "start"]),
                call: |args| edit(args, "add_segment"),
            },
            Tool {
                name: "segments_delete",
                description: "Delete lines by id: their voice and their subtitle are gone.",
                schema: || object(json!({ "pid": pid(), "ids": ids("line ids"), "response_format": detail() }), &["pid", "ids"]),
                call: |args| edit(args, "del_segments"),
            },
            Tool {
                name: "segments_hide",
                description: "Hide lines (hidden true, the default) or show them again (false): a hidden line is neither voiced nor subtitled.",
                schema: || object(json!({ "pid": pid(), "ids": ids("line ids"), "hidden": { "type": "boolean" }, "response_format": detail() }), &["pid", "ids"]),
                call: |args| edit(args, "hide_segments"),
            },
            Tool {
                name: "segments_keep_original",
                description: "Let the original voice play for lines instead of the dub (keep true, the default), or dub them again (false).",
                schema: || object(json!({ "pid": pid(), "ids": ids("line ids"), "keep": { "type": "boolean" }, "response_format": detail() }), &["pid", "ids"]),
                call: |args| edit(args, "keep_segments"),
            },
            Tool {
                name: "segments_reorder",
                description: "Put the lines in this order of ids; the lines not named follow in their order.",
                schema: || object(json!({ "pid": pid(), "ids": ids("line ids in their new order"), "response_format": detail() }), &["pid", "ids"]),
                call: |args| edit(args, "reorder_segments"),
            },
            Tool {
                name: "segment_regen",
                description: "Voice one line again at the next project_dub_audio or project_render, and only that one: every other line is marked as voiced.",
                schema: || object(json!({ "pid": pid(), "id": { "type": "string" }, "response_format": detail() }), &["pid", "id"]),
                call: |args| {
                    text(args, "id")?;
                    edit(args, "regen")
                },
            },
            Tool {
                name: "segments_regen_all",
                description: "Voice every line again at the next project_dub_audio or project_render.",
                schema: || object(json!({ "pid": pid(), "response_format": detail() }), &["pid"]),
                call: |args| edit(args, "regen_all"),
            },
            Tool {
                name: "segment_split",
                description: "Cut a line in two at a moment (at, seconds, inside the line): the recognised words and the text are divided there, tgt_text and tgt_text_2 give the two halves' translations (the translation is divided in the same share when left out). The second half gets new_id or <id>_2 (an id whose letters, digits and underscores are another line's is taken: they name the line's files). A subtitle text of the line's own (caption_style_set with seg_id and text) is divided in the same share, and both halves keep its place and style. Both halves are dirty; the answer shows both.",
                schema: || object(json!({ "pid": pid(), "id": { "type": "string" }, "at": { "type": "number", "description": "seconds" }, "tgt_text": { "type": "string" }, "tgt_text_2": { "type": "string" }, "new_id": { "type": "string" }, "response_format": detail() }), &["pid", "id", "at"]),
                call: |args| {
                    text(args, "id")?;
                    edit(args, "split_segment")
                },
            },
            Tool {
                name: "segments_merge",
                description: "Join lines that follow one another in the list into one (ids): the first keeps its id, speaker and voice, it runs from the earliest start to the latest end, the texts follow each other. The lines' own subtitle overrides become one: the place and style of the first that has one, and, when any line has a subtitle text of its own, the parts' texts in order. It is dirty.",
                schema: || object(json!({ "pid": pid(), "ids": ids("line ids, neighbours in the list"), "response_format": detail() }), &["pid", "ids"]),
                call: |args| edit(args, "merge_segments"),
            },
            Tool {
                name: "segment_shorten",
                description: "Rewrite the translation of lines shorter so they fit their time slot, through the translation model: the source, the current translation and two neighbouring lines each side go with a character limit from the slot and the voice's pace; an answer in another script, not shorter, or echoing the source is refused. ids names the lines; all_over takes every line whose fit.over is true (project_get shows fit per line: est, slot, ratio, verdict fits/tight/impossible, calibrated, and rendered with needed/eff_cap from the last render). A job: studio_wait with its job_id; its result lists shortened (from, to), rejected with the reason and unpinned lines. The new text is dirty: project_dub_audio voices it; the previous take stays in takes_list.",
                schema: || object(json!({ "pid": pid(), "ids": ids("line ids to shorten"), "all_over": { "type": "boolean", "description": "every line that does not fit its slot" } }), &["pid"]),
                call: |args| {
                    let path = project_path(args, "/shorten")?;
                    post(path, body_without(args, &["pid"]))
                },
            },
            Tool {
                name: "takes_list",
                description: "The takes of a line - the last five voicings kept by the studio (multi-take alternatives, regenerations, QC re-synthesis, shortened versions): n, the text each voices (text_matches: the line's current text), duration, QC similarity, source, voice, reference, synthesis parameters and the file; active is the take the mix plays, pinned the one a render never replaces.",
                schema: || object(json!({ "pid": pid(), "id": { "type": "string", "description": "line id" } }), &["pid", "id"]),
                call: |args| get(project_path(args, &format!("/segments/{}/takes", segment(&text(args, "id")?)))?),
            },
            Tool {
                name: "take_select",
                description: "Make a take of a line (its n from takes_list) the one the mix plays, without voicing again: a take of other text brings that text back into the line. The next project_dub_audio or project_render only mixes again. Refused (409) while another take of the line is pinned: take_pin with pinned false first.",
                schema: || object(json!({ "pid": pid(), "id": { "type": "string" }, "take": { "type": "integer", "description": "n of takes_list" }, "response_format": detail() }), &["pid", "id", "take"]),
                call: |args| {
                    text(args, "id")?;
                    edit(args, "take_select")
                },
            },
            Tool {
                name: "take_pin",
                description: "Pin the active take of a line (pinned true) so no render, regeneration or QC replaces it, or unpin it (false). Changing the line's text unpins it.",
                schema: || object(json!({ "pid": pid(), "id": { "type": "string" }, "pinned": { "type": "boolean" }, "response_format": detail() }), &["pid", "id", "pinned"]),
                call: |args| {
                    text(args, "id")?;
                    edit(args, "take_pin")
                },
            },
            // ---------------------------------------------------------------- what the project makes
            Tool {
                name: "project_mode_set",
                description: "What the project makes: subtitles (the original audio with subtitles in its own language), dub, voiceover (the translation over the quieted original), transcribe (the transcript), funny (a playful dub). Subtitles switched off stay off; dub, voiceover and funny show the translation unless the subtitles are bilingual or, over a dub or voiceover, in the original language, which they keep. Every line becomes dirty.",
                schema: || object(json!({ "pid": pid(), "value": { "type": "string", "enum": ["subtitles", "dub", "voiceover", "transcribe", "funny"] }, "response_format": detail() }), &["pid", "value"]),
                call: |args| edit(args, "mode"),
            },
            Tool {
                name: "audio_output_set",
                description: "What is heard, apart from the subtitles: none (the original), dub or voiceover. A remix instruction stays; dub and voiceover make every line dirty.",
                schema: || object(json!({ "pid": pid(), "value": { "type": "string", "enum": ["none", "dub", "voiceover"] }, "response_format": detail() }), &["pid", "value"]),
                call: |args| edit(args, "dub"),
            },
            Tool {
                name: "subtitles_content_set",
                description: "What the subtitles say, apart from what is heard (a dub can carry subtitles in the original language): value none, transcribe (the original language), translate, or bilingual (the translation with the original as a second line). For bilingual, order puts the translation on top (translation_top, the default) or the original; secondary styles the original's line: size_pct (40-100, 70 by default) of the main line's size, color (#RRGGBB) and opacity (10-100), where null makes them the main line's again. Fields left out stay; value can be left out to restyle only.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(),
                            "value": { "type": "string", "enum": ["none", "transcribe", "translate", "bilingual"] },
                            "order": { "type": "string", "enum": ["translation_top", "original_top"] },
                            "secondary": {
                                "type": "object",
                                "properties": {
                                    "size_pct": { "type": "integer", "minimum": 40, "maximum": 100 },
                                    "color": { "type": ["string", "null"], "description": "#RRGGBB; null: the main line's colour" },
                                    "opacity": { "type": ["integer", "null"], "minimum": 10, "maximum": 100, "description": "null: as the main line" },
                                },
                                "additionalProperties": false,
                            },
                            "response_format": detail(),
                        }),
                        &["pid"],
                    )
                },
                call: |args| edit(args, "subs_content"),
            },
            Tool {
                name: "subtitles_burn_set",
                description: "Burn the subtitles and titles into the video (on true) or leave the picture clean (false). Takes effect at the next render; nothing is voiced again.",
                schema: || object(json!({ "pid": pid(), "on": { "type": "boolean" }, "response_format": detail() }), &["pid", "on"]),
                call: |args| edit(args, "subs_burn"),
            },
            Tool {
                name: "subtitles_position_set",
                description: "Move the subtitle band to sub_y pixels from the top of the frame, for every line.",
                schema: || object(json!({ "pid": pid(), "sub_y": { "type": "integer" }, "response_format": detail() }), &["pid", "sub_y"]),
                call: |args| edit(args, "subpos"),
            },
            Tool {
                name: "translation_target_set",
                description: "Set the target language (the subtitles show the translation unless they are off, bilingual or, over a dub or voiceover, in the original language, which they keep; mode funny adds a playful rewrite). Nothing is translated yet: project_retranslate translates the lines, project_export_lang makes a copy in another language. Every line becomes dirty.",
                schema: || object(json!({ "pid": pid(), "lang": { "type": "string" }, "mode": { "type": "string", "enum": ["plain", "funny"] }, "response_format": detail() }), &["pid", "lang"]),
                call: |args| {
                    text(args, "lang")?;
                    edit(args, "translate")
                },
            },
            Tool {
                name: "translation_style_set",
                description: "The tone of the translation - formal, slang, for children - up to 500 characters, used at the next translation (project_retranslate, project_export_lang, project_analyze); empty removes it. Every line becomes dirty.",
                schema: || object(json!({ "pid": pid(), "style": { "type": "string" }, "response_format": detail() }), &["pid", "style"]),
                call: |args| edit(args, "translate_style"),
            },
            Tool {
                name: "rewrite_set",
                description: "Store a remix instruction and switch the project to dub; project_remix rewrites the lines by it. Every line becomes dirty.",
                schema: || object(json!({ "pid": pid(), "instruction": { "type": "string" }, "response_format": detail() }), &["pid", "instruction"]),
                call: |args| {
                    text(args, "instruction")?;
                    edit(args, "rewrite")
                },
            },
            Tool {
                name: "voice_set",
                description: "How the lines are voiced: clone (each speaker's own voice, cloned from the video), autocast (library voices by each speaker's gender), auto, or voice with voice_name - one voice of voices_list for everyone, or a comma list by speaker in the order of their ids where - keeps that speaker cloned. Every line becomes dirty.",
                schema: || object(json!({ "pid": pid(), "voice_mode": { "type": "string", "enum": ["clone", "autocast", "auto", "voice"] }, "voice_name": { "type": "string" }, "response_format": detail() }), &["pid", "voice_mode"]),
                call: |args| {
                    text(args, "voice_mode")?;
                    edit(args, "recast")
                },
            },
            Tool {
                name: "gain_set",
                description: "The dub track's volume in dB (-24 to 24), applied at the next render without voicing again.",
                schema: || object(json!({ "pid": pid(), "gain_db": { "type": "number" }, "response_format": detail() }), &["pid", "gain_db"]),
                call: |args| edit(args, "gain"),
            },
            Tool {
                name: "loudness_set",
                description: "Even out loudness (on, the default): every phrase to one level and the dub to -14 LUFS with a -1 dBTP ceiling; off keeps the mix as it came. Applied at the next render without voicing again.",
                schema: || object(json!({ "pid": pid(), "on": { "type": "boolean" }, "response_format": detail() }), &["pid", "on"]),
                call: |args| edit(args, "loudness"),
            },
            Tool {
                name: "voiceover_gain_set",
                description: "In voiceover, the original track's volume in dB under the translation (-40 to 0; 0 is full).",
                schema: || object(json!({ "pid": pid(), "gain_db": { "type": "number" }, "response_format": detail() }), &["pid", "gain_db"]),
                call: |args| edit(args, "voiceover_gain"),
            },
            Tool {
                name: "original_track_set",
                description: "Keep the original audio as a second track of the output (keep true, the default) or not, in an mp4 or mkv container.",
                schema: || object(json!({ "pid": pid(), "keep": { "type": "boolean" }, "container": { "type": "string", "enum": ["mp4", "mkv"] }, "response_format": detail() }), &["pid"]),
                call: |args| edit(args, "keep_original"),
            },
            // ---------------------------------------------------------------- subtitles, titles, blur
            Tool {
                name: "caption_style_set",
                description: "The subtitles' style for the whole video, or for one line (seg_id), where text, x, y, w and fs (font size) also reword and place that line's subtitle. color and outline are #RRGGBB, outline_w the outline's width, align left, center or right, font a name of fonts_list, size_px the font size, n_lines the most lines, shadow_dir an angle (null removes the shadow), plate a box behind the text in plate_color (#RRGGBBAA). Fields left out stay.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(), "seg_id": { "type": "string" }, "color": { "type": "string" }, "outline": { "type": "string" }, "outline_w": { "type": "integer" },
                            "align": { "type": "string", "enum": ["left", "center", "right"] }, "font": { "type": "string" }, "size_px": { "type": "integer" },
                            "n_lines": { "type": "integer" }, "italic": { "type": "boolean" }, "bold": { "type": "boolean" }, "uppercase": { "type": "boolean" },
                            "shadow_dir": { "type": ["integer", "null"] }, "plate": { "type": "boolean" }, "plate_color": { "type": "string" },
                            "text": { "type": "string" }, "x": { "type": "integer" }, "y": { "type": "integer" }, "w": { "type": "integer" }, "fs": { "type": "integer" },
                            "response_format": detail(),
                        }),
                        &["pid"],
                    )
                },
                call: |args| edit(args, "caption"),
            },
            Tool {
                name: "caption_preset_set",
                description: "Style the subtitles with a preset (name from caption_presets_list); without a name they follow the style read from the original video again.",
                schema: || object(json!({ "pid": pid(), "name": { "type": "string" }, "response_format": detail() }), &["pid"]),
                call: |args| edit(args, "preset"),
            },
            Tool {
                name: "title_add",
                description: "Add a title: text in a box of pixels x, y, w, h, shown from t0 to t1 seconds (the whole video by default), in italic, font and color (#RRGGBB, white by default).",
                schema: || {
                    object(
                        json!({
                            "pid": pid(), "text": { "type": "string" }, "x": { "type": "integer" }, "y": { "type": "integer" }, "w": { "type": "integer" }, "h": { "type": "integer" },
                            "t0": { "type": "number" }, "t1": { "type": "number" }, "italic": { "type": "boolean" }, "font": { "type": "string" }, "color": { "type": "string" },
                            "response_format": detail(),
                        }),
                        &["pid", "text", "x", "y", "w", "h"],
                    )
                },
                call: |args| edit(args, "title_add"),
            },
            Tool {
                name: "title_update",
                description: "Edit title idx (from project_get): text (the original words), tgt (the translation shown), font, color, bg, outline, outline_w, shadow_dir, italic, bold, uppercase, solid, align, size_px, lh (line height), bbox [x, y, w, h], start and end in seconds. null for lh, size_px, outline_w or shadow_dir makes it automatic again.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(), "idx": { "type": "integer" }, "text": { "type": "string" }, "tgt": { "type": "string" }, "font": { "type": ["string", "null"] },
                            "color": { "type": ["string", "null"] }, "bg": { "type": ["string", "null"] }, "outline": { "type": ["string", "null"] },
                            "outline_w": { "type": ["integer", "null"] }, "shadow_dir": { "type": ["integer", "null"] }, "italic": { "type": "boolean" }, "bold": { "type": "boolean" },
                            "uppercase": { "type": "boolean" }, "solid": { "type": "boolean" }, "align": { "type": "string" }, "size_px": { "type": ["integer", "null"] },
                            "lh": { "type": ["integer", "null"] }, "bbox": { "type": "array", "items": { "type": "integer" } }, "start": { "type": "number" }, "end": { "type": "number" },
                            "response_format": detail(),
                        }),
                        &["pid", "idx"],
                    )
                },
                call: |args| edit(args, "title"),
            },
            Tool {
                name: "titles_delete",
                description: "Delete titles by idx.",
                schema: || object(json!({ "pid": pid(), "idxs": idxs("title idx from project_get"), "response_format": detail() }), &["pid", "idxs"]),
                call: |args| edit(args, "del_titles"),
            },
            Tool {
                name: "blur_add",
                description: "Blur a box of pixels x, y, w, h from t0 to t1 seconds (the whole video by default): it hides on-screen text or a logo.",
                schema: || object(json!({ "pid": pid(), "x": { "type": "integer" }, "y": { "type": "integer" }, "w": { "type": "integer" }, "h": { "type": "integer" }, "t0": { "type": "number" }, "t1": { "type": "number" }, "response_format": detail() }), &["pid", "x", "y", "w", "h"]),
                call: |args| edit(args, "blur_add"),
            },
            Tool {
                name: "blur_update",
                description: "Change blur box idx (from project_get): its box, times, hidden, or fill - a #RRGGBB colour painted instead of the blur (\"\" blurs again).",
                schema: || {
                    object(
                        json!({
                            "pid": pid(), "idx": { "type": "integer" }, "x": { "type": "integer" }, "y": { "type": "integer" }, "w": { "type": "integer" }, "h": { "type": "integer" },
                            "t0": { "type": "number" }, "t1": { "type": "number" }, "hidden": { "type": "boolean" }, "fill": { "type": "string" }, "response_format": detail(),
                        }),
                        &["pid", "idx"],
                    )
                },
                call: |args| edit(args, "blur"),
            },
            Tool {
                name: "blurs_delete",
                description: "Delete blur boxes by idx.",
                schema: || object(json!({ "pid": pid(), "idxs": idxs("blur box idx from project_get"), "response_format": detail() }), &["pid", "idxs"]),
                call: |args| edit(args, "del_blurs"),
            },
            Tool {
                name: "blur_enable",
                description: "Turn blurring on or off for the whole video: the blur boxes and the blurred band under burned subtitles.",
                schema: || object(json!({ "pid": pid(), "on": { "type": "boolean" }, "response_format": detail() }), &["pid", "on"]),
                call: |args| edit(args, "blur_enable"),
            },
            // ---------------------------------------------------------------- casting
            Tool {
                name: "casting_get",
                description: "The characters casting found (project_analyze with casting): id, name, gender, the voice they are dubbed with (null: their own, cloned), the speech note, their speaker ids and number of lines, and whether there is an avatar and a voice sample.",
                schema: project_only,
                call: |args| get(project_path(args, "/casting")?),
            },
            Tool {
                name: "casting_update",
                description: "Change characters by id: name, gender, the voice they are dubbed with (dub_voice: a voice of voices_list, or null to clone their own) and speech_note (how they talk; it goes into the translation's tone). Changed voices make the lines dirty. Answers every character.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(),
                            "characters": { "type": "array", "items": { "type": "object", "properties": { "id": { "type": "string" }, "name": { "type": "string" }, "gender": { "type": "string" }, "dub_voice": { "type": ["string", "null"] }, "speech_note": { "type": "string" } }, "required": ["id"] } },
                        }),
                        &["pid", "characters"],
                    )
                },
                call: |args| post(project_path(args, "/casting")?, json!({ "characters": args.get("characters").cloned().unwrap_or_default() })),
            },
            Tool {
                name: "casting_avatar",
                description: "A character's face, as a picture.",
                schema: || object(json!({ "pid": pid(), "character_id": { "type": "string" } }), &["pid", "character_id"]),
                call: |args| get(format!("{}{}", project_path(args, "/casting/avatar")?, query(&[("id", Some(text(args, "character_id")?))]))),
            },
            Tool {
                name: "casting_library_list",
                description: "The saved castings - a series' cast kept across its episodes: slug, name, number of characters.",
                schema: nothing,
                call: |_| get("/casting/library".into()),
            },
            Tool {
                name: "casting_library_save",
                description: "Save the project's casting under a name, with its avatars and voice samples: project_analyze with casting and casting_ref applies it to the next episode.",
                schema: || object(json!({ "pid": pid(), "name": { "type": "string" } }), &["pid", "name"]),
                call: |args| post(project_path(args, "/casting/library")?, json!({ "name": text(args, "name")? })),
            },
            Tool {
                name: "casting_library_delete",
                description: "Delete a saved casting.",
                schema: || id_only("slug", "slug from casting_library_list"),
                call: |args| send(Method::DELETE, format!("/casting/library/{}", segment(&text(args, "slug")?)), json!({})),
            },
            Tool {
                name: "casting_library_avatar",
                description: "The face of a saved casting's character, as a picture.",
                schema: || object(json!({ "slug": { "type": "string" }, "character_id": { "type": "string" } }), &["slug", "character_id"]),
                call: |args| get(format!("/casting/library/{}/avatar{}", segment(&text(args, "slug")?), query(&[("id", Some(text(args, "character_id")?))]))),
            },
            // ---------------------------------------------------------------- glossary
            Tool {
                name: "glossary_get",
                description: "The project's glossary: terms with their translation, keep (left untranslated), pronunciation (how the voice says it; the screen keeps the translation), asr_fix (how speech recognition misspells the term; analysis corrects it), note, source (manual; auto from glossary_extract; series from the saved casting at analysis - a new analysis takes those from the profile again, so change one by setting it as manual) and lang (the language of translation and pronunciation). stale: the translation was made with other term translations or keep marks (pronunciation, asr_fix and note do not count) - project_retranslate makes it again. format tsv answers term, translation, keep, pronunciation as tab-separated text.",
                schema: || object(json!({ "pid": pid(), "format": { "type": "string", "enum": ["json", "tsv"] } }), &["pid"]),
                call: |args| get(format!("{}{}", project_path(args, "/glossary")?, query(&[("format", given(args, "format").filter(|f| f != "json"))]))),
            },
            Tool {
                name: "glossary_set",
                description: "Set the project's glossary: entries (the whole list, as glossary_get gives it) or tsv (term, translation, keep 1/0, pronunciation per line; a header line is optional). merge: true adds them to the glossary instead, a term already there taking the new entry (from tsv only its columns: asr_fix, note, source and lang stay). An entry without lang takes lang (omitted: the project's target language), and so do the tsv entries. The translation is not redone: the answer says stale when project_retranslate should run.",
                schema: || {
                    object(
                        json!({
                            "pid": pid(),
                            "entries": glossary_entries(),
                            "tsv": { "type": "string" },
                            "merge": { "type": "boolean" },
                            "lang": { "type": "string" },
                        }),
                        &["pid"],
                    )
                },
                call: |args| send(Method::PUT, project_path(args, "/glossary")?, body_without(args, &["pid"])),
            },
            Tool {
                name: "glossary_extract",
                description: "Collect glossary candidates from the project's text: names, recurring terms and brands with their translation (a pass of the translation model and the repeated names). A job: studio_wait with its job_id; its result lists entries with source auto and the terms already in the glossary left out. Nothing is saved - glossary_set with merge: true adds the ones to keep.",
                schema: project_only,
                call: |args| post(project_path(args, "/glossary/extract")?, json!({})),
            },
            Tool {
                name: "series_glossary_get",
                description: "The glossary of a saved casting (a series' profile in casting_library_list), kept across its episodes: project_analyze with casting_ref adds it to the project's glossary, the project's own entries winning. format tsv as for glossary_get.",
                schema: || object(json!({ "slug": { "type": "string", "description": "slug from casting_library_list" }, "format": { "type": "string", "enum": ["json", "tsv"] } }), &["slug"]),
                call: |args| get(format!("/casting/library/{}/glossary{}", segment(&text(args, "slug")?), query(&[("format", given(args, "format").filter(|f| f != "json"))]))),
            },
            Tool {
                name: "series_glossary_set",
                description: "Set the glossary of a saved casting: entries or tsv as for glossary_set, merge: true to add to it - glossary_get's entries with merge: true keep a project's glossary for the next episodes. lang is the language of tsv entries (omitted: any language).",
                schema: || {
                    object(
                        json!({
                            "slug": { "type": "string", "description": "slug from casting_library_list" },
                            "entries": glossary_entries(),
                            "tsv": { "type": "string" },
                            "merge": { "type": "boolean" },
                            "lang": { "type": "string" },
                        }),
                        &["slug"],
                    )
                },
                call: |args| send(Method::PUT, format!("/casting/library/{}/glossary", segment(&text(args, "slug")?)), body_without(args, &["slug"])),
            },
            // ---------------------------------------------------------------- voices
            Tool {
                name: "voices_list",
                description: "The voices of the studio's library by name, for voice_set, casting_update and voice_slots_assign.",
                schema: nothing,
                call: |_| get("/voices".into()),
            },
            Tool {
                name: "voices_catalog",
                description: "The extra voices that can be downloaded into the library, with their gender.",
                schema: nothing,
                call: |_| get("/voices/catalog".into()),
            },
            Tool {
                name: "voice_download",
                description: "Download one voice of voices_catalog into the library.",
                schema: || id_only("name", "voice name from voices_catalog"),
                call: |args| post("/voices/get".into(), json!({ "name": text(args, "name")? })),
            },
            Tool {
                name: "voices_download_pack",
                description: "Download the voice pack into the library. A job: studio_wait with its job_id.",
                schema: nothing,
                call: |_| post("/voices/download-pack".into(), json!({})),
            },
            Tool {
                name: "voice_record_devices",
                description: "The microphones the studio can record a voice from.",
                schema: nothing,
                call: |_| get("/record/devices".into()),
            },
            Tool {
                name: "voice_record_start",
                description: "Start recording a new library voice from the user's microphone: name of the voice, device from voice_record_devices (omitted: the default microphone). Ask the user to read 10-20 seconds of clear speech, watch voice_record_level, then voice_record_stop keeps it. ok false names why it could not start.",
                schema: || object(json!({ "name": { "type": "string" }, "device": { "type": "string" } }), &["name"]),
                call: |args| {
                    let mut body = json!({ "name": text(args, "name")? });
                    if let Some(device) = args.get("device").and_then(Value::as_str) {
                        body["device"] = json!(device);
                    }
                    post("/record/start".into(), body)
                },
            },
            Tool {
                name: "voice_record_level",
                description: "How loud the microphone hears the user now, 0 to 1, while voice_record_start records.",
                schema: nothing,
                call: |_| get("/record/level".into()),
            },
            Tool {
                name: "voice_record_stop",
                description: "Stop the recording and keep it as the voice: answers its name (null when nothing was recorded) and the library's voices.",
                schema: nothing,
                call: |_| post("/record/stop".into(), json!({})),
            },
            Tool {
                name: "voice_rename",
                description: "Rename a voice of the library.",
                schema: || object(json!({ "from": { "type": "string" }, "to": { "type": "string" } }), &["from", "to"]),
                call: |args| post("/voices/rename".into(), json!({ "from": text(args, "from")?, "to": text(args, "to")? })),
            },
            Tool {
                name: "voice_delete",
                description: "Delete a voice from the library.",
                schema: || id_only("name", "voice name from voices_list"),
                call: |args| post("/voices/delete".into(), json!({ "name": text(args, "name")? })),
            },
            Tool {
                name: "voice_from_speaker",
                description: "Make a library voice of a project's speaker: their longest line (up to 12 s), cleaned of music, is saved in full band as the voice name with its words. Refuses with no_separation (409) when the project has no separated vocals and the separation engine of the chosen backend is not installed (models_download roformer with bsroformer-engine, or bsroformer-engine-cpu); separation_failed says why separating the line failed.",
                schema: || object(json!({ "pid": pid(), "speaker": { "type": "string", "description": "speaker id from project_get" }, "name": { "type": "string" } }), &["pid", "speaker", "name"]),
                call: |args| post(project_path(args, "/speaker-voice")?, json!({ "speaker": text(args, "speaker")?, "name": text(args, "name")? })),
            },
            Tool {
                name: "voice_slots_assign",
                description: "Give the project's speakers library voices by gender: each speaker's gender is measured from their pitch, and male and female voices are dealt out in order of how much each speaker talks, round again past the list. An empty list keeps that gender cloned. Every line becomes dirty. Answers each speaker's voice, gender and pitch.",
                schema: || object(json!({ "pid": pid(), "male": ids("voice names for men"), "female": ids("voice names for women") }), &["pid"]),
                call: |args| {
                    let list = |name: &str| args.get(name).cloned().unwrap_or_else(|| json!([]));
                    post(project_path(args, "/voice-slots")?, json!({ "male": list("male"), "female": list("female") }))
                },
            },
        ];
        all.extend(atomic::tools());
        all.extend(window::tools());
        all
    })
}

// ---------------------------------------------------------------- calling a route

/// The router the tools call; the tests' own stands in for it, whichever
/// router another test of this crate installs.
fn studio_api() -> Result<Router, String> {
    #[cfg(test)]
    if let Some(stub) = tests::STUB.get() {
        return Ok(stub.clone());
    }
    API.get().cloned().ok_or_else(|| "the studio is still starting".to_string())
}

/// A route's answer as it came.
struct Reply {
    status: StatusCode,
    mime: String,
    bytes: axum::body::Bytes,
}

/// Builds the route's request, calls it inside the process and returns what it
/// answered. A page instead of an answer means the route is not there.
async fn call_route_raw(call: Call) -> Result<Reply, String> {
    let api = studio_api()?;
    let asked = format!("{} {}", call.method, call.path);
    let builder = Request::builder().method(call.method).uri(&call.path).header(header::HOST, "127.0.0.1").header(window::AGENT_HEADER, "1");
    let request = match call.payload {
        Payload::None => builder.body(Body::empty()),
        Payload::Window { command, .. } => return Err(format!("{command} is a command of the studio's window, not a route")),
        Payload::Json(body) => builder.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())),
        Payload::Form { fields, files } => {
            let boundary = format!("studio-mcp-{}", uuid::Uuid::new_v4().simple());
            let mut parts: Vec<Part> = Vec::new();
            for (name, value) in fields {
                parts.push(Part::Text(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")));
            }
            for (name, path, file) in files {
                if let Err(error) = tokio::fs::metadata(&path).await {
                    return Err(format!("read {}: {error}", path.display()));
                }
                parts.push(Part::Text(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n", file.replace('"', "'"))));
                parts.push(Part::File(path));
                parts.push(Part::Text("\r\n".into()));
            }
            parts.push(Part::Text(format!("--{boundary}--\r\n")));
            builder.header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}")).body(Body::from_stream(streamed(parts)))
        }
    }
    .map_err(|error| error.to_string())?;
    let response = api.oneshot(request).await.map_err(|error| error.to_string())?;
    let status = response.status();
    let mime = response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or_default().to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024 * 1024).await.map_err(|error| error.to_string())?;
    if mime.starts_with("text/html") {
        return Err(format!("The studio has no route {asked}: its page answered instead."));
    }
    Ok(Reply { status, mime, bytes,
    })
}

/// A route's answer as text, cut to a size an agent reads.
fn reply_text(reply: &Reply) -> String {
    let mut text = match serde_json::from_slice::<Value>(&reply.bytes) {
        Ok(value) => serde_json::to_string(&value).unwrap_or_default(),
        Err(_) => String::from_utf8_lossy(&reply.bytes).into_owned(),
    };
    if text.trim().is_empty() {
        text = if reply.status.is_success() { "Done.".into() } else { reply.status.to_string() };
    }
    text
}

/// Calls a route inside the process and returns what it answered.
async fn call_route(call: Call) -> Result<(StatusCode, String), String> {
    let reply = call_route_raw(call).await?;
    Ok((reply.status, reply_text(&reply)))
}

/// A piece of a multipart body: text, or a file read as it is sent.
enum Part {
    Text(String),
    File(PathBuf),
}

/// The parts as a stream, a file a megabyte at a time: a long video is sent
/// without ever being held in memory whole.
fn streamed(parts: Vec<Part>,
) -> impl futures_util::Stream<Item = std::io::Result<axum::body::Bytes>> {
    futures_util::stream::unfold((parts.into_iter(), None::<tokio::fs::File>), |(mut parts, mut open)| async move {
        loop {
            if let Some(file) = open.as_mut() {
                let mut chunk = vec![0u8; 1 << 20];
                match tokio::io::AsyncReadExt::read(file, &mut chunk).await {
                    Ok(0) => open = None,
                    Ok(read) => {
                        chunk.truncate(read);
                        return Some((Ok(chunk.into()), (parts, open)));
                    }
                    Err(error) => return Some((Err(error), (parts, None))),
                }
                continue;
            }
            match parts.next()? {
                Part::Text(text) => return Some((Ok(text.into()), (parts, None))),
                Part::File(path) => match tokio::fs::File::open(&path).await {
                    Ok(file) => open = Some(file),
                    Err(error) => return Some((Err(error), (parts, None))),
                },
            }
        }
    },
    )
}

/// Cuts an answer to what an agent reads in one go, and says how to get the rest.
fn cut(mut text: String) -> String {
    if text.len() > LIMIT {
        let mut end = LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n... (cut: ask for a part, e.g. project_get with from and to seconds or ids)",
        );
    }
    text
}

// ---------------------------------------------------------------- the protocol

/// The protocol revisions the studio speaks: the stateless one, where every
/// request carries its version, and the handshake ones older clients open
/// with `initialize`.
const MODERN: &[&str] = &["2026-07-28"];
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// How long a client may keep the tool list, the prompts and the resources:
/// they change only with the studio, but an update should show within minutes.
const LIST_TTL_MS: u64 = 300_000;
const HEADER_MISMATCH: i64 = -32020;
const UNSUPPORTED_VERSION: i64 = -32022;

fn server_info() -> Value {
    json!({ "name": SERVER_NAME, "title": STUDIO, "version": app_version() })
}

fn supported_versions() -> Vec<&'static str> {
    MODERN.iter().chain(LEGACY).copied().collect()
}

/// A complete result, signed with the server's identity.
fn rpc(id: Value, mut result: Value) -> Response {
    if let Some(fields) = result.as_object_mut() {
        fields.entry("resultType").or_insert_with(|| "complete".into());
        let meta = fields.entry("_meta").or_insert_with(|| json!({}));
        meta["io.modelcontextprotocol/serverInfo"] = server_info();
    }
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn rpc_error(id: Value, code: i64, message: String) -> Response {
    rpc_failure(StatusCode::OK, id, code, message, None)
}

fn rpc_failure(status: StatusCode, id: Value, code: i64, message: String, data: Option<Value>,
) -> Response {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    (status, Json(json!({ "jsonrpc": "2.0", "id": id, "error": error })),
    ).into_response()
}

/// A list or a read a client may cache: the same for everyone, fresh for five minutes.
fn cacheable(mut result: Value) -> Value {
    result["ttlMs"] = LIST_TTL_MS.into();
    result["cacheScope"] = "public".into();
    result
}

/// A header value, with the Base64 sentinel form decoded.
fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    use base64::Engine;
    let raw = headers.get(name)?.to_str().ok()?;
    match raw.strip_prefix("=?base64?").and_then(|inner| inner.strip_suffix("?=")) {
        Some(encoded) => base64::engine::general_purpose::STANDARD.decode(encoded).ok().and_then(|bytes| String::from_utf8(bytes).ok()),
        None => Some(raw.to_string()),
    }
}

/// Why a stateless request is refused, if it is: a version the studio does
/// not speak, or headers that do not say what its body says.
fn refused(headers: &HeaderMap, method: &str, params: &Value, version: &str,
) -> Option<(i64, String, Option<Value>)> {
    if !MODERN.contains(&version) {
        return Some((UNSUPPORTED_VERSION, "Unsupported protocol version".into(), Some(json!({ "supported": supported_versions(), "requested": version })),
        ));
    }
    let mismatch = |what: String| Some((HEADER_MISMATCH, format!("Header mismatch: {what}"), None));
    match header_value(headers, "mcp-protocol-version") {
        Some(value) if value == version => {}
        Some(value) => {
            return mismatch(format!("MCP-Protocol-Version header value '{value}' does not match body value '{version}'"))
        }
        None => return mismatch("the MCP-Protocol-Version header is missing".into()),
    }
    match header_value(headers, "mcp-method") {
        Some(value) if value == method => {}
        Some(value) => {
            return mismatch(format!("Mcp-Method header value '{value}' does not match body value '{method}'"))
        }
        None => return mismatch("the Mcp-Method header is missing".into()),
    }
    let named = match method {
        "tools/call" | "prompts/get" => params.get("name"),
        "resources/read" => params.get("uri"),
        _ => return None,
    }
    .and_then(Value::as_str)
    .unwrap_or_default();
    match header_value(headers, "mcp-name") {
        Some(value) if value == named => None,
        Some(value) => mismatch(format!("Mcp-Name header value '{value}' does not match body value '{named}'")),
        None => mismatch("the Mcp-Name header is missing".into()),
    }
}

/// A tool's answer: the text an agent reads, and the same data structured
/// when it is JSON and whole.
fn tool_result(id: Value, text: String, structured: Option<Value>, error: bool) -> Response {
    let mut result = json!({ "content": [{ "type": "text", "text": text }], "isError": error });
    if let Some(structured) = structured.filter(|value| value.is_object() || value.is_array()) {
        result["structuredContent"] = structured;
    }
    rpc(id, result)
}

fn tool_json(id: Value, value: Value) -> Response {
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    if text.len() > LIMIT {
        tool_result(id, cut(text), None, false)
    } else {
        tool_result(id, text, Some(value), false)
    }
}

/// A picture a route answered with, as an image the agent sees.
fn tool_image(id: Value, reply: &Reply, name: &str, args: &Value) -> Response {
    use base64::Engine;
    let mime = reply.mime.split(';').next().unwrap_or_default().trim().to_string();
    let what = match name {
        "project_frame" => format!(
            "The {} frame of project {} at {} s.",
            if args.get("source").and_then(Value::as_str) == Some("original") { "original" } else { "dubbed" },
            args.get("pid").and_then(Value::as_str).unwrap_or_default(),
            args.get("t").and_then(Value::as_f64).unwrap_or_default()
        ),
        _ => format!("The avatar of character {}.", args.get("character_id").and_then(Value::as_str).unwrap_or_default()),
    };
    let image = base64::engine::general_purpose::STANDARD.encode(&reply.bytes);
    rpc(id, json!({ "content": [{ "type": "image", "data": image, "mimeType": mime }, { "type": "text", "text": what }], "isError": false }),
    )
}

/// The name a tool shows the user: its words, the first capitalised.
fn tool_title(name: &str) -> String {
    let words = name.replace('_', " ");
    let mut letters = words.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

pub async fn handle(headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if !crate::guard::local_origin(&headers) {
        return crate::guard::foreign_origin();
    }
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return rpc_failure(StatusCode::BAD_REQUEST, Value::Null, -32700, "Parse error".into(), None,
        );
    };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return rpc_failure(StatusCode::BAD_REQUEST, Value::Null, -32600, "Invalid request: one JSON-RPC request or notification per POST".into(), None,
        );
    };
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    // a notification is only acknowledged
    if message.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    // A request of the stateless revision says its version in _meta and is
    // checked against its headers; one without it is of the handshake era.
    if let Some(version) = params.get("_meta").and_then(|meta| meta.get(META_VERSION)).and_then(Value::as_str) {
        if let Some((code, text, data)) = refused(&headers, method, &params, version) {
            return rpc_failure(StatusCode::BAD_REQUEST, id, code, text, data);
        }
    }
    if method != "tools/call" {
        seen(method);
    }
    match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let protocol = LEGACY.iter().find(|version| **version == asked).copied().unwrap_or(LEGACY[0]);
            rpc(id, json!({
                "protocolVersion": protocol,
                "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
                "serverInfo": server_info(),
                "instructions": INSTRUCTIONS,
            }),
            )
        }
        "server/discover" => rpc(id, cacheable(json!({
            "supportedVersions": supported_versions(),
            "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
            "instructions": INSTRUCTIONS,
        })),
        ),
        "ping" => rpc(id, json!({})),
        "resources/list" => rpc(id, cacheable(json!({ "resources": [
            { "uri": SKILL_URI, "name": "studio skill", "title": "How to drive the studio", "description": "How to drive Dub Studio: every tool by area, the ground rules, step-by-step recipes.", "mimeType": "text/markdown" },
            { "uri": LANGUAGES_URI, "name": "languages", "title": "Languages", "description": "The language codes a video is dubbed from and into: tgt_lang, src_lang, lang.", "mimeType": "application/json" },
            { "uri": EDITS_URI, "name": "project edits", "title": "Project edits", "description": "Every edit of a project (PATCH op), the tool that makes it and its fields.", "mimeType": "application/json" },
        ] })),
        ),
        "resources/templates/list" => rpc(id, cacheable(json!({ "resourceTemplates": [] }))),
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).unwrap_or_default();
            match resource(uri) {
                Some((mime, text)) => rpc(id, cacheable(json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] }),
                    ),
                ),
                None => rpc_error(id, -32002, format!("Resource not found: {uri}")),
            }
        }
        "prompts/list" => rpc(id, cacheable(json!({ "prompts": [
            { "name": "studio", "title": "Studio skill", "description": "Load the studio's skill: the tools, the ground rules and the recipes." },
            { "name": "dub_video", "title": "Dub a video", "description": "Dub a video file on this computer into a language, from the file to the finished video.", "arguments": [
                { "name": "path", "description": "the video file", "required": true },
                { "name": "lang", "description": "the language to dub into, a code such as ru, en, es", "required": true },
                { "name": "mode", "description": "dub (default), voiceover, nodub (subtitles only) or transcribe", "required": false },
            ] },
        ] })),
        ),
        "prompts/get" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let argument = |key: &str| {
                params.get("arguments").and_then(|arguments| arguments.get(key)).and_then(Value::as_str).unwrap_or_default().trim().to_string()
            };
            let text = match name {
                "studio" => Some(SKILL.to_string()),
                "dub_video" => {
                    let mode = Some(argument("mode")).filter(|mode| !mode.is_empty()).unwrap_or_else(|| "dub".into());
                    Some(format!(
                        "Dub the video {path} into {lang} (mode {mode}) with Dub Studio. Read the skill first (prompt studio, resource {SKILL_URI}).\n\n1. studio_status: the models must be ready and no job running.\n2. project_create with path {path}; keep its project_id.\n3. project_analyze with that pid, tgt_lang {lang} and mode {mode}; studio_wait with its job_id until done.\n4. project_get: read the translation line by line and fix what reads wrong with segment_update.\n5. project_render; studio_wait with its job_id.\n6. project_frame at a moment with speech to see the subtitles as they are burned in.\n7. project_save_output into the folder the user named (ask when they did not), then tell the user the path.",
                        path = argument("path"),
                        lang = argument("lang"),
                    ))
                }
                _ => None,
            };
            match text {
                Some(text) => rpc(id, json!({ "messages": [{ "role": "user", "content": { "type": "text", "text": text } }] }),
                ),
                None => rpc_error(id, -32602, format!("Unknown prompt: {name}")),
            }
        }
        "tools/list" => {
            let list: Vec<Value> = tools().iter().map(|tool| json!({ "name": tool.name, "title": tool_title(tool.name), "description": tool.description, "inputSchema": (tool.schema)(), "annotations": annotations(tool.name) })).collect();
            rpc(id, cacheable(json!({ "tools": list })))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            seen(name);
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let answer = |text: String, error: bool| tool_result(id.clone(), text, None, error);
            let Some(tool) = tools().iter().find(|tool| tool.name == name) else {
                return rpc_error(id, -32602, format!("Unknown tool: {name}; tools/list names them all."),
                );
            };
            // a tool may look at a file on disk before its call: off the runtime
            let prepare = tool.call;
            let prepared = {
                let args = args.clone();
                tokio::task::spawn_blocking(move || prepare(&args)).await.unwrap_or_else(|error| Err(format!("{name} failed: {error}")))
            };
            match prepared {
                Err(problem) => answer(problem, true),
                Ok(Call { payload: Payload::Window { command, args, seconds,
                        }, .. }) => match window::ask_window(command, args, seconds).await {
                    Ok(result) => window::window_reply(id, result),
                    Err(problem) => answer(problem, true),
                },
                Ok(call) if call.path == "composite:status" => {
                    tool_json(id, status_summary().await)
                }
                Ok(call) if call.path == "composite:wait" => match wait_for(&args).await {
                    Ok(state) => tool_json(id, state),
                    Err(problem) => answer(problem, true),
                },
                Ok(call) if call.path.starts_with(atomic::PREFIX) => {
                    match atomic::run(name, &args).await {
                    Ok(result) => tool_json(id, result),
                    Err(problem) => answer(problem, true),
                }
                }
                Ok(call) => {
                    if QUEUED_BEHIND_JOBS.contains(&name) {
                        if let Err(problem) = graphics_card_free().await {
                            return answer(problem, true);
                        }
                    }
                    match call_route_raw(call).await {
                        Ok(reply) if reply.status.is_success() && reply.mime.starts_with("image/") =>
                        {
                            tool_image(id, &reply, name, &args)
                        }
                        Ok(reply) if reply.status.is_success() => {
                            let text = reply_text(&reply);
                            match serde_json::from_str::<Value>(&text) {
                                Ok(value) => tool_json(id, shape(name, &args, redact(value))),
                                Err(_) => answer(text, false),
                            }
                        }
                        Ok(reply) => answer(reply_text(&reply), true),
                        Err(problem) => answer(problem, true),
                    }
                }
            }
        }
        _ => rpc_failure(StatusCode::NOT_FOUND, id, -32601, format!("Method not found: {method}"), None,
        ),
    }
}

/// A moment an agent names, as unix seconds in this computer's time zone:
/// today, yesterday, a date (its start, or its end for an upper bound) or a date-time.
fn moment(text: &str, end: bool) -> Result<i64, String> {
    use chrono::{Duration as Days, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
    let text = text.trim();
    let day = match text.to_lowercase().as_str() {
        "today" => Some(Local::now().date_naive()),
        "yesterday" => Some(Local::now().date_naive() - Days::days(1)),
        _ => NaiveDate::parse_from_str(text, "%Y-%m-%d").ok(),
    };
    let at = match day {
        Some(day) => day.and_time(if end { NaiveTime::from_hms_opt(23, 59, 59).unwrap_or_default() } else { NaiveTime::MIN }),
        None => NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M"))
            .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M"))
            .map_err(|_| {
                format!("'{text}' is not a moment: today, yesterday, 2026-09-26 or 2026-09-26T18:00.")
            })?,
    };
    Local.from_local_datetime(&at).earliest().map(|at| at.timestamp()).ok_or_else(|| format!("'{text}' does not exist in this time zone."))
}

/// What an agent is told when it connects.
const INSTRUCTIONS: &str = "You drive Dub Studio on this computer: it dubs, voices over, subtitles and transcribes videos. Every tool runs the same code as a button of the studio, through the routes its window calls. Start with studio_status. For one result from a file, without working in the studio, one call does it: transcribe_file (the transcript), translate_file (translated subtitles), dub_file (the dubbed video), separate_file (voice and background apart), detect_text_file (the text in the picture); export_subtitles writes a project's subtitles. Long work - project_analyze, project_dub_audio, project_render, project_export_lang, project_retranslate, project_remix, downloads - is a job: start it, then studio_wait with its job_id instead of polling. The graphics card runs one job at a time and a preview frame (project_frame) waits behind it, so look at frames while the studio is idle. Edits (segment_update, caption_style_set and the rest) are instant and saved; a line whose words, timing, speaker or voice changed is dirty, and project_dub_audio and project_render voice only the dirty lines again. Look ids up instead of guessing them: projects_list, project_get, voices_list, casting_get, casting_library_list, models_status. Files on this computer are passed by path. To work in front of the user, open the project in the studio's window with editor_open and use the editor_* tools (the user watches the lines, the timeline, the frame and the export change) and ui_* for anything else on screen; they need the window open, the rest works without it. The whole guide is the resource studio://skill (prompt 'studio').";

#[cfg(test)]
mod tests {
    use super::*;

    /// The router the tools call in these tests.
    pub(super) static STUB: OnceLock<Router> = OnceLock::new();

    fn a_project() -> Value {
        json!({
            "meta": { "video": "C:/v/clip.mp4", "duration": 10.0, "width": 1920, "height": 1080, "fps": 25.0, "src_codec": "h264" },
            "mode": "dub", "tgt_lang": "ru",
            "audio": { "voice": { "mode": "clone", "name": null }, "gain_db": 0.0 },
            "subs": { "mode": "translate", "burn": true },
            "render": { "blur": true },
            "captions": { "sub_style": { "color": "#FFFFFF" }, "sub_y": 900, "overrides": [], "titles": [{ "text": "Title", "tgt": "Титр" }], "blur_boxes": [{ "x": 1, "y": 2, "w": 3, "h": 4 }], "preset": {} },
            "raw_ctx": { "scene_context": "long" },
            "stage_ckpts": { "asr": "abc" },
            "segments": [
                { "id": "s1", "start": 0.5, "end": 2.0, "speaker": "0", "src_text": "Hello", "tgt_text": "Привет", "dirty": true, "words": [{ "w": "Hello", "s": 0.5 }], "ckpt": "k1" },
                { "id": "s2", "start": 4.0, "end": 6.0, "speaker": "1", "src_text": "Bye", "tgt_text": "Пока", "dirty": false, "hidden": true },
            ],
        })
    }

    /// A studio in miniature: a render running, an analysis done, a picture,
    /// a project, the settings with a key, and a page for what is not there;
    /// the projects and jobs of the one-call tools' tests are atomic::stub's.
    pub(super) fn stub() {
        use axum::extract::{Multipart, Path as Segment, Query};
        use axum::routing::{get, post};
        let jobs = || {
            json!({ "jobs": [
                { "id": "j1", "kind": "render", "pid": "p1", "status": "running", "stage": "tts", "msg": "voicing", "pct": 40.0 },
                { "id": "j2", "kind": "analyze", "pid": "p1", "status": "done", "stage": "done", "msg": "", "pct": 100.0, "result": { "project_id": "p1" } },
            ] })
        };
        let fetches = || {
            vec![
                json!({ "id": "urla1", "url": "https://v.example/a", "status": "downloading", "phase": "download", "title": "A clip", "downloaded": 25, "total": 100, "pid": null, "warning": null, "errorCode": null }),
                json!({ "id": "urlb2", "url": "https://v.example/b", "status": "completed", "phase": "done", "title": "B clip", "downloaded": 9, "total": 9, "pid": "p9", "subsImported": true, "warning": null, "errorCode": null }),
                json!({ "id": "urlc3", "url": "https://v.example/c", "status": "failed", "phase": "probe", "title": null, "downloaded": 0, "total": null, "pid": null, "warning": null, "errorCode": "geo_blocked", "error": "not available in your country", "hint": "set a proxy" }),
            ]
        };
        let router = Router::new()
            .route("/jobs", get(move |Query(asked): Query<std::collections::HashMap<String, String>>| async move {
                Json(asked.get("pid").and_then(|pid| atomic::stub::jobs(pid)).unwrap_or_else(jobs))
            }))
            .route("/jobs/{id}", get(move |Segment(id): Segment<String>| async move {
                if let Some(job) = atomic::stub::job(&id) {
                    return Json(job).into_response();
                }
                match jobs()["jobs"].as_array().unwrap().iter().find(|job| job["id"] == id.as_str()) {
                    Some(job) => Json(job.clone()).into_response(),
                    None => (StatusCode::NOT_FOUND, "job not found").into_response(),
                }
            }))
            .route("/setup/status", get(|| async { Json(json!({ "ready": false, "driverOk": true, "downloadPending": 5, "components": [
                { "id": "higgs", "name": "Higgs", "requirement": "required", "installed": false, "size": 5, "bytesOnDisk": 0, "vram": 1, "missing": ["a"] },
                { "id": "ocr", "name": "OCR", "requirement": "recommended", "installed": true, "size": 1, "bytesOnDisk": 1, "vram": 0, "missing": [] },
            ] })) }))
            .route("/url/fetches", get(move || async move { Json(json!({ "fetches": fetches() })) }))
            .route("/url/fetches/{id}", get(move |Segment(id): Segment<String>| async move {
                match fetches().into_iter().find(|f| f["id"] == id.as_str()) {
                    Some(f) => Json(f).into_response(),
                    None => (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response(),
                }
            }))
            .route("/url/probe", get(|| async {
                let formats = json!(["json3", "srv1", "srv2", "srv3", "ttml", "srt", "vtt"]);
                let auto: Vec<Value> = (0..157).map(|n| json!({ "lang": format!("l{n:03}"), "name": format!("Language {n} from English"), "formats": formats })).collect();
                Json(json!({
                    "url": "https://www.youtube.com/watch?v=abc", "title": "A long clip", "duration": 600.0, "thumbnail": "https://i.ytimg.com/vi/abc/hqdefault.jpg",
                    "thumbnail_data": format!("data:image/jpeg;base64,{}", "A".repeat(60_000)), "thumbnail_error": null, "uploader": "Someone", "extractor": "Youtube",
                    "max_height": 2160, "has_video": true, "has_audio": true, "qualities": ["best", "1080", "720", "480", "audio"],
                    "subtitles": [{ "lang": "en", "name": "English", "formats": formats }, { "lang": "ru", "name": "Russian", "formats": formats }],
                    "auto_subtitles": auto, "expected_bytes": 123456789, "tool_version": "2026.09.01",
                }))
            }))
            .route("/engine/capabilities", get(|| async { Json(json!({ "selection": { "or_key": "sk-or-secret", "proxy_url": "http://user:pass@host:8080", "bench": "1" }, "asr_engines": ["parakeet", "whisper"] })) }))
            .route(
                "/projects/{pid}",
                get(|Segment(pid): Segment<String>| async move { atomic::stub::project(&pid).unwrap_or_else(|| Json(a_project()).into_response()) })
                    .patch(|Segment(pid): Segment<String>, Json(edit): Json<Value>| async move { atomic::stub::patch(&pid, &edit).unwrap_or_else(|| Json(a_project()).into_response()) }),
            )
            .route("/projects/{pid}/preview", get(|| async { ([(header::CONTENT_TYPE, "image/jpeg")], vec![0xFFu8, 0xD8, 0xFF, 0xD9]).into_response() }))
            .route("/host", get(|headers: HeaderMap| async move { Json(json!({ "host": headers.get(header::HOST).and_then(|value| value.to_str().ok()) })) }))
            .route("/projects", post(|mut form: Multipart| async move {
                let mut parts = Vec::new();
                while let Some(field) = form.next_field().await.unwrap() {
                    let name = field.name().unwrap_or_default().to_string();
                    let file = field.file_name().unwrap_or_default().to_string();
                    let size = field.bytes().await.unwrap().len();
                    parts.push(json!({ "name": name, "file": file, "size": size }));
                }
                Json(json!({ "project_id": "p2", "parts": parts }))
            }))
            .merge(atomic::stub::routes())
            .fallback(|| async { axum::response::Html("<!doctype html><title>Dub Studio</title>") })
            .layer(axum::extract::DefaultBodyLimit::disable());
        let _ = STUB.set(router);
    }

    pub(super) async fn call_tool(name: &str, arguments: Value) -> Value {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
        let response = handle(HeaderMap::new(), axum::body::Bytes::from(body.to_string())).await;
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 24).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    pub(super) fn answer_text(reply: &Value) -> String {
        reply["result"]["content"].as_array().unwrap().iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n")
    }

    // ---------------------------------------------------------------- the router and the edits, read from their source

    /// The routes of build_router: (METHOD, path pattern).
    fn router_routes() -> Vec<(String, String)> {
        let source = include_str!("lib.rs").replace("\r\n", "\n");
        let body = source.split("pub fn build_router(").nth(1).expect("build_router").split("\n}\n").next().unwrap();
        let code: String = body.lines().map(|line| line.split("//").next().unwrap_or_default()).collect::<Vec<_>>().join("\n");
        let mut routes = Vec::new();
        for chunk in code.split(".route(").skip(1) {
            let path = chunk.trim_start().strip_prefix('"').and_then(|rest| rest.split('"').next()).expect("a route's path").to_string();
            let handlers = chunk.split(".fallback(").next().unwrap().split(".layer(").next().unwrap().split(".with_state(").next().unwrap();
            for method in ["get", "post", "patch", "put", "delete"] {
                let call = format!("{method}(");
                let called = handlers.match_indices(&call).any(|(at, _)| {
                    !handlers[..at].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_')
                });
                if called {
                    routes.push((method.to_uppercase(), path.clone()));
                }
            }
        }
        routes
    }

    /// The ops patch::apply knows.
    fn patch_ops() -> Vec<String> {
        let source = include_str!("patch.rs").replace("\r\n", "\n");
        let body = source.split("pub fn apply(").nth(1).expect("patch::apply").split("other =>").next().unwrap();
        body.lines().filter_map(|line| {
                line.trim().strip_prefix('"').and_then(|rest| rest.split_once("\" =>")).map(|(op, _)| op.to_string())
            }).collect()
    }

    fn route_of(pattern: &str, path: &str) -> bool {
        let path = path.split('?').next().unwrap_or_default();
        let (wanted, given): (Vec<&str>, Vec<&str>) = (pattern.split('/').collect(), path.split('/').collect());
        wanted.len() == given.len() && wanted.iter().zip(&given).all(|(part, value)| part.starts_with('{') && !value.is_empty() || part == value)
    }

    /// A pattern with its holes filled, as a tool would call it.
    fn filled(pattern: &str) -> String {
        pattern.split('/').map(|part| if part.starts_with('{') { "x1" } else { part }).collect::<Vec<_>>().join("/")
    }

    /// Routes an agent is not given, and why.
    const NOT_TOOLS: &[(&str, &str, &str)] = &[
        ("GET", "/health", "the service's identity for the desktop shell's second launch: initialize names the server and its version"),
        ("PATCH", "/engine/opts", "an echo kept for the old page: models_select and settings_set choose the models"),
        ("POST", "/setup/browse", "a folder dialog for the person: models_import takes the path"),
        ("POST", "/pick-folder", "a folder dialog for the person: the agent names the folder"),
        ("GET", "/voices/sample", "audio for the page's player"),
        ("GET", "/projects/{pid}/casting/voice", "audio for the page's player"),
        ("GET", "/projects/{pid}/output", "the video for the page's player: project_files names the file"),
        ("GET", "/projects/{pid}/dub", "the audio for the page's player: project_files names the file"),
        ("GET", "/projects/{pid}/segments/{id}/takes/{n}/audio", "a take for the page's player: takes_list names its file"),
        ("GET", "/jobs/{job_id}/events", "the page's progress stream: job_get and studio_wait"),
        ("POST", "/url/probe", "the page's probe with cookies.txt as its content (a browser does not know file paths): url_probe passes the path"),
        ("POST", "/mcp", "the MCP server itself"),
        ("PUT", "/settings/ui-lang", "the window reports the language it is shown in, so the server speaks it"),
        ("GET", "/mcp/status", "the settings page's view of the agent"),
        ("GET", "/mcp/window", "the window's own stream of commands: the ui_* and editor_* tools go through it"),
        ("POST", "/mcp/window/result", "the window's answers to those commands"),
        ("POST", "/mcp/window/focus", "the window the person turned to"),
    ];

    /// Routes the tools call that come with the parallel work on jobs; once one
    /// is in the router it leaves this list.
    const PENDING: &[(&str, &str)] = &[];

    /// What the composite tools read.
    const COMPOSITE_ROUTES: &[(&str, &[(&str, &str)])] = &[
        ("composite:status", &[("GET", "/jobs"), ("GET", "/setup/status"), ("GET", "/url/fetches"),
            ],
        ),
        ("composite:wait", &[("GET", "/jobs/x1"), ("GET", "/url/fetches/x1"), ("GET", "/jobs"), ("GET", "/setup/status"), ("GET", "/url/fetches"),
            ],
        ),
    ];

    /// Arguments for every property of a tool, and one set more for each other
    /// choice of an enum.
    fn samples(tool: &Tool, video: &Path, subtitles: &Path) -> Vec<Value> {
        let schema = (tool.schema)();
        let properties = schema["properties"].as_object().cloned().unwrap_or_default();
        let sample = |name: &str, property: &Value| -> Value {
            if let Some(first) = property["enum"].as_array().and_then(|choices| choices.first()) {
                return first.clone();
            }
            let kind = match &property["type"] {
                Value::Array(kinds) => kinds[0].as_str().unwrap_or("string").to_string(),
                other => other.as_str().unwrap_or("string").to_string(),
            };
            match (kind.as_str(), name) {
                ("integer", _) => json!(1),
                ("number", _) => json!(1.5),
                ("boolean", _) => json!(true),
                ("array", _) if property["items"]["type"] == "integer" => json!([1]),
                ("array", _) => json!(["x1"]),
                ("object", _) => json!({}),
                (_, "path") => json!(video.to_string_lossy()),
                (_, "subtitles_path") => json!(subtitles.to_string_lossy()),
                _ => json!("x1"),
            }
        };
        let base: serde_json::Map<String, Value> = properties.iter().map(|(name, property)| (name.clone(), sample(name, property))).collect();
        let mut all = vec![Value::Object(base.clone())];
        for (name, property) in &properties {
            for choice in property["enum"].as_array().into_iter().flatten().skip(1) {
                let mut other = base.clone();
                other.insert(name.clone(), choice.clone());
                all.push(Value::Object(other));
            }
        }
        all
    }

    /// The routes a tool reaches: (METHOD, path).
    fn reached_by(tool: &Tool, video: &Path, subtitles: &Path) -> Vec<(String, String)> {
        let mut reached = Vec::new();
        for args in samples(tool, video, subtitles) {
            let call = (tool.call)(&args).unwrap_or_else(|problem| panic!("{} refused {args}: {problem}", tool.name));
            if matches!(call.payload, Payload::Window { .. }) {
                continue;
            }
            match COMPOSITE_ROUTES.iter().chain(atomic::ROUTES).find(|(path, _)| *path == call.path) {
                Some((_, reads)) => reached.extend(reads.iter().map(|(method, path)| (method.to_string(), path.to_string())),
                ),
                None => {
                    assert!(!call.path.starts_with("composite:"), "{} is a composite this test does not know", tool.name);
                    reached.push((call.method.to_string(), call.path.clone()));
                }
            }
        }
        reached
    }

    #[test]
    fn the_transcript_is_what_is_heard_and_the_translation_what_is_burned() {
        let project = json!({
            "tgt_lang": "ru",
            "captions": { "overrides": [{ "seg_id": "d", "text": "До встречи" }] },
            "segments": [
                { "id": "a", "start": 59.96, "end": 61.0, "speaker": "0", "src_text": "Hello", "tgt_text": "Привет" },
                { "id": "b", "start": 61.0, "end": 62.0, "speaker": "0", "src_text": "Thanks for watching", "tgt_text": "Спасибо", "hidden": true },
                { "id": "c", "start": 62.0, "end": 63.0, "speaker": "1", "src_text": "Bonjour", "tgt_text": "Бонжур", "keep_original": true },
                { "id": "d", "start": 63.0, "end": 64.0, "speaker": "1", "src_text": "Bye", "tgt_text": "Пока" },
            ]
        });
        let heard = transcript(&project, &json!({ "format": "text" }));
        assert_eq!(heard["text"], "[1:00.0 SPK 0] Hello\n[1:02.0 SPK 1] Bonjour\n[1:03.0 SPK 1] Bye");
        let burned = transcript(&project, &json!({ "text": "tgt" }));
        let texts: Vec<&str> = burned["lines"].as_array().unwrap().iter().map(|line| line["text"].as_str().unwrap()).collect();
        assert_eq!((texts, burned["language"].as_str()), (vec!["Привет", "До встречи"], Some("ru")));
    }

    #[test]
    fn every_route_and_every_edit_is_a_tool() {
        let folder = tempfile::tempdir().unwrap();
        let (video, subtitles) = (folder.path().join("clip.mp4"), folder.path().join("clip.srt"),
        );
        std::fs::write(&video, b"x").unwrap();
        std::fs::write(&subtitles, b"x").unwrap();
        let routes = router_routes();
        assert!(routes.len() > 50, "build_router read: {routes:?}");
        let mut covered = vec![false; routes.len()];
        for tool in tools() {
            for (method, path) in reached_by(tool, &video, &subtitles) {
                let found: Vec<usize> = routes.iter().enumerate().filter(|(_, (m, p))| *m == method && route_of(p, &path)).map(|(at, _)| at).collect();
                let pending = PENDING.iter().any(|(m, p)| *m == method && route_of(p, &path));
                assert!(!found.is_empty() || pending, "{} calls {method} {path}, a route the studio does not have", tool.name);
                for at in found {
                    covered[at] = true;
                }
            }
        }
        let by_tool = covered.clone();
        let mut stale = Vec::new();
        for (method, path) in NOT_TOOLS.iter().map(|(method, path, _)| (method, path)) {
            let at = routes.iter().position(|(m, p)| m == method && p == path).unwrap_or_else(|| {
                    panic!("{method} {path} of NOT_TOOLS is not a route of build_router")
                });
            if by_tool[at] {
                stale.push(format!("{method} {path}"));
            }
            covered[at] = true;
        }
        assert!(stale.is_empty(), "these routes of NOT_TOOLS are reached by a tool: drop them from NOT_TOOLS: {stale:?}");
        let missing: Vec<String> = routes.iter().zip(&covered).filter(|(_, done)| !**done).map(|((method, path), _)| format!("{method} {path}")).collect();
        assert!(missing.is_empty(), "these routes have no tool: give each one, or put it into NOT_TOOLS with why: {missing:?}");
        let landed: Vec<String> = PENDING.iter().filter(|(method, pattern)| {
                routes.iter().any(|(m, p)| m == method && route_of(p, &filled(pattern)))
            }).map(|(method, pattern)| format!("{method} {pattern}")).collect();
        assert!(landed.is_empty(), "these routes are in the router now: drop them from PENDING: {landed:?}");

        let ops = patch_ops();
        assert!(ops.len() >= 35, "patch::apply read: {ops:?}");
        for op in &ops {
            assert!(every_op().contains(&op.as_str()), "the edit {op} has no tool: add it to PATCH_OPS, or to PATCH_ALIASES when a tool already makes what it does");
        }
        for op in every_op() {
            assert!(ops.iter().any(|known| known == op), "{op} is not an op of patch::apply");
        }
        for (op, name) in PATCH_OPS {
            let tool = tools().iter().find(|tool| tool.name == *name).unwrap_or_else(|| panic!("{name} is not a tool"));
            let call = (tool.call)(&samples(tool, &video, &subtitles)[0]).unwrap();
            assert_eq!((call.method.clone(), call.path.as_str()), (Method::PATCH, "/projects/x1"), "{name}");
            match call.payload {
                Payload::Json(body) => assert_eq!(body["op"], *op, "{name} makes {op}"),
                _ => panic!("{name} sends a JSON body"),
            }
        }
        for (alias, same) in PATCH_ALIASES {
            assert!(PATCH_OPS.iter().any(|(op, _)| op == same), "{alias} names {same}, which no tool makes");
        }
    }

    #[test]
    fn the_glossary_tools_turn_their_arguments_into_their_routes() {
        let find = |name: &str| {
            tools().iter().find(|tool| tool.name == name).expect("the tool")
        };
        let body = |call: Call| match call.payload {
            Payload::Json(body) => body,
            _ => panic!("a JSON body"),
        };
        let call = (find("glossary_get").call)(&json!({ "pid": "p1" })).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::GET, "/projects/p1/glossary"));
        let call = (find("glossary_get").call)(&json!({ "pid": "p1", "format": "tsv" })).unwrap();
        assert_eq!(call.path, "/projects/p1/glossary?format=tsv");
        let call = (find("glossary_set").call)(&json!({ "pid": "p1", "entries": [{ "term": "Harry", "translation": "Гарри" }], "merge": true })).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::PUT, "/projects/p1/glossary"));
        assert_eq!(body(call), json!({ "entries": [{ "term": "Harry", "translation": "Гарри" }], "merge": true }));
        let call = (find("glossary_extract").call)(&json!({ "pid": "p1" })).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::POST, "/projects/p1/glossary/extract"));
        let call = (find("series_glossary_get").call)(&json!({ "slug": "my show", "format": "json" })).unwrap();
        assert_eq!(call.path, "/casting/library/my%20show/glossary");
        let call = (find("series_glossary_set").call)(&json!({ "slug": "show", "tsv": "Harry\tГарри", "lang": "ru" }),
        ).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::PUT, "/casting/library/show/glossary"));
        assert_eq!(body(call), json!({ "tsv": "Harry\tГарри", "lang": "ru" }));
        assert!((find("series_glossary_get").call)(&json!({})).is_err(), "a slug is required");
    }

    #[test]
    fn every_tool_is_in_the_skill() {
        let missing: Vec<&str> = tools().iter().map(|tool| tool.name).filter(|name| !SKILL.contains(&format!("`{name}`"))).collect();
        assert!(missing.is_empty(), "docs/mcp-skill.md does not name {missing:?}");
    }

    #[test]
    fn every_tool_has_a_unique_name_and_an_object_schema() {
        let mut names: Vec<&str> = tools().iter().map(|tool| tool.name).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "two tools share a name");
        for tool in tools() {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{} has no object schema", tool.name);
            assert!(!tool.description.is_empty(), "{} has no description", tool.name);
        }
    }

    #[test]
    fn a_tool_turns_its_arguments_into_its_route() {
        let find = |name: &str| {
            tools().iter().find(|tool| tool.name == name).expect("the tool")
        };
        let call = (find("segment_update").call)(&json!({ "pid": "p 1", "id": "s1", "tgt_text": "Привет", "response_format": "detailed" })).unwrap();
        assert_eq!(call.method, Method::PATCH);
        assert_eq!(call.path, "/projects/p%201");
        match call.payload {
            Payload::Json(body) => assert_eq!(body, json!({ "op": "segment", "id": "s1", "tgt_text": "Привет" })),
            _ => panic!("a JSON body"),
        }
        let call = (find("project_analyze").call)(&json!({ "pid": "p1", "tgt_lang": "ru", "mode": "voiceover", "burn": false, "casting": true, "rewrite": "make it rhyme & shine" })).unwrap();
        assert_eq!(call.path, "/projects/p1/analyze?tgt_lang=ru&mode=voiceover&rewrite=make%20it%20rhyme%20%26%20shine&burn=0&casting=1");
        let call = (find("project_analyze").call)(&json!({ "pid": "p1", "mode": "transcribe" })).unwrap();
        assert_eq!(call.path, "/projects/p1/analyze?mode=transcribe");
        assert!((find("project_analyze").call)(&json!({ "pid": "p1", "mode": "dub" })).is_err(), "a dub needs tgt_lang");
        let call = (find("project_frame").call)(&json!({ "pid": "p1", "t": 12.5, "source": "original" })).unwrap();
        assert_eq!(call.path, "/projects/p1/original?t=12.5");
        let call = (find("jobs_list").call)(&json!({})).unwrap();
        assert_eq!(call.path, "/jobs");
        assert!((find("project_get").call)(&json!({})).is_err(), "a missing id is refused");
        assert!((find("settings_set").call)(&json!({ "key": "or_key", "value": "sk" })).is_err(), "the key goes through openrouter_set_key");
        assert!((find("settings_set").call)(&json!({ "key": "proxy_url", "value": "http://host:1" })).is_err(), "the proxy goes through proxy_settings_set");
        assert!((find("project_create").call)(&json!({ "path": "Z:/nowhere/clip.mp4" })).is_err(), "a file that is not there is refused before the upload");
        let call = (find("url_probe").call)(&json!({ "url": "https://www.youtube.com/watch?v=a b&t=1", "cookies": "C:/c/cookies.txt" })).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::GET, "/url/probe?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3Da%20b%26t%3D1&cookies=C%3A%2Fc%2Fcookies.txt"));
        assert!((find("url_probe").call)(&json!({})).is_err(), "a link is required");
        let call = (find("project_create_from_url").call)(&json!({ "url": "https://v.example/a", "quality": "720", "subs_lang": "en", "response_format": "detailed" })).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::POST, "/projects/from_url"));
        match call.payload {
            Payload::Json(body) => assert_eq!(body, json!({ "url": "https://v.example/a", "quality": "720", "subs_lang": "en" })),
            _ => panic!("a JSON body"),
        }
        let call = (find("url_fetch_delete").call)(&json!({ "id": "urla1" })).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::DELETE, "/url/fetches/urla1"));
        let call = (find("url_fetch_resume").call)(&json!({ "id": "urla1" })).unwrap();
        assert_eq!((call.method, call.path.as_str()), (Method::POST, "/url/fetches/urla1/resume"));
    }

    #[test]
    fn bilingual_subtitles_and_their_export_are_tools() {
        let find = |name: &str| {
            tools().iter().find(|tool| tool.name == name).expect("the tool")
        };
        let args = json!({ "pid": "p1", "value": "bilingual", "order": "original_top", "secondary": { "size_pct": 60, "color": null } });
        let call = (find("subtitles_content_set").call)(&args).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::PATCH, "/projects/p1"));
        match call.payload {
            Payload::Json(body) => assert_eq!(body, json!({ "op": "subs_content", "value": "bilingual", "order": "original_top", "secondary": { "size_pct": 60, "color": null } })),
            _ => panic!("a JSON body"),
        }
        let call = (find("project_export_text").call)(&json!({ "pid": "p1", "format": "vtt", "text": "both", "order": "translation_top" }),
        ).unwrap();
        assert_eq!((call.method.clone(), call.path.as_str()), (Method::POST, "/projects/p1/export-text"));
        match call.payload {
            Payload::Json(body) => assert_eq!(body, json!({ "format": "vtt", "text": "both", "order": "translation_top" })),
            _ => panic!("a JSON body"),
        }
        let schema = (find("project_analyze").schema)();
        assert!(schema["properties"]["subs"]["enum"].as_array().unwrap().contains(&json!("bilingual")));
    }

    #[test]
    fn a_tool_that_changes_something_is_not_read_only() {
        for name in ["project_create", "segment_update", "models_download", "project_dub_audio", "voice_from_speaker", "project_export_text", "blur_enable", "job_cancel",
        ] {
            assert_eq!(annotations(name)["readOnlyHint"], false, "{name}");
        }
        for name in ["studio_status", "studio_wait", "project_get", "project_frame", "project_files", "jobs_list", "casting_avatar", "openrouter_models", "settings_get", "caption_presets_list", "proxy_test",
        ] {
            assert_eq!(annotations(name)["readOnlyHint"], true, "{name}");
        }
        for name in ["models_download", "voice_download", "openrouter_set_key", "openrouter_models",
        ] {
            assert_eq!(annotations(name)["openWorldHint"], true, "{name}");
        }
        for name in ["openrouter_status", "openrouter_delete_key", "project_render",
        ] {
            assert_eq!(annotations(name)["openWorldHint"], false, "{name}");
        }
    }

    #[test]
    fn a_tool_that_writes_over_what_was_stored_is_destructive() {
        for name in ["project_put", "project_analyze", "segment_update", "segments_delete", "project_delete", "voice_delete", "casting_update", "openrouter_delete_key", "job_cancel", "models_cancel_download",
        ] {
            assert_eq!(annotations(name)["destructiveHint"], true, "{name}");
        }
        for name in ["project_create", "segment_add", "project_render", "project_export_lang", "title_add", "settings_set",
        ] {
            assert_eq!(annotations(name)["destructiveHint"], false, "{name}");
        }
    }

    #[test]
    fn the_key_never_leaves_the_studio() {
        let clean = redact(json!({ "selection": { "or_key": "sk-or-secret", "proxy_url": "socks5://user:pass@host:1080", "bench": "1" }, "list": [{ "or_key": "" }] }),
        );
        assert_eq!(clean, json!({ "selection": { "or_key_set": true, "proxy_url": "socks5://user@host:1080", "bench": "1" }, "list": [{ "or_key_set": false }] }));
    }

    #[test]
    fn an_agent_is_answered_what_it_acts_on() {
        let short = compact_project(&a_project(), &json!({}));
        assert_eq!(short["segments_total"], 2);
        assert_eq!(short["dirty"], 1);
        assert_eq!(short["segments"][1]["hidden"], true);
        assert_eq!(short["captions"]["titles"][0]["idx"], 0);
        let text = short.to_string();
        assert!(!text.contains("scene_context") && !text.contains("\"words\"") && !text.contains("\"ckpt\""), "no vision context, word timings or checksums");
        let window = compact_project(&a_project(), &json!({ "from": 3.0, "to": 10.0 }));
        assert_eq!(window["segments"].as_array().unwrap().len(), 1);
        assert_eq!(window["segments"][0]["id"], "s2");

        let change = compact_change("segment_update", &json!({ "pid": "p1", "id": "s1" }), &a_project(),
        );
        assert_eq!(change["changed"][0]["tgt_text"], "Привет");
        assert!(change.get("titles").is_none());
        let titles = compact_change("title_update", &json!({ "pid": "p1", "idx": 0 }), &a_project(),
        );
        assert_eq!(titles["titles"][0]["tgt"], "Титр");
        assert_eq!(shape("segment_update", &json!({ "response_format": "detailed" }), a_project()), a_project(), "detailed is the whole project");
        let by_op = compact_change("project_patch", &json!({ "pid": "p1", "op": "blur_del", "idx": 0 }), &a_project(),
        );
        assert_eq!(by_op["blur_boxes"][0]["idx"], 0, "an op answers as its tool");
        assert!(compact_change("segments_reorder", &json!({ "pid": "p1", "ids": ["s2", "s1"] }), &a_project()).get("changed").is_none(), "a new order is not every line again");

        let job = compact_job(&json!({ "id": "j", "kind": "remix", "pid": "p", "status": "done", "result": a_project() }),
        );
        assert_eq!(job["result"], "the project, updated: project_get reads it");
        assert!(job.get("error").is_none());
    }

    #[test]
    fn idle_waits_for_every_kind_of_work() {
        assert!(busy(&json!({ "jobs": [] })).unwrap().is_empty());
        assert_eq!(busy(&json!({ "jobs": [{ "kind": "render", "status": "running" }, { "status": "queued" }] })).unwrap(), ["render", "unknown"]);
        assert!(busy(&json!({ "jobs_error": "GET /jobs: 404" })).is_err(), "jobs that cannot be read are not idle");
        assert!(finished(&json!({ "status": "cancelled" })) && !finished(&json!({ "status": "running" })) && !finished(&json!({ "status": "cancelling" })));
    }

    #[test]
    fn the_server_names_the_apps_version() {
        let version = app_version();
        assert_eq!(version.split('.').count(), 3, "{version}");
        assert!(version.split('.').all(|part| part.parse::<u32>().is_ok()), "{version}");
    }

    #[tokio::test]
    async fn a_multipart_body_streams_its_files() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("clip.mp4");
        std::fs::write(&path, vec![7u8; (1 << 20) + 5]).unwrap();
        let parts = vec![Part::Text("head".into()), Part::File(path), Part::Text("tail".into()),
        ];
        let chunks: Vec<axum::body::Bytes> = futures_util::StreamExt::collect::<Vec<_>>(streamed(parts)).await.into_iter().map(Result::unwrap).collect();
        let whole: Vec<u8> = chunks.concat();
        assert_eq!(whole.len(), 4 + (1 << 20) + 5 + 4);
        assert!(whole.starts_with(b"head") && whole.ends_with(b"tail"));
    }

    #[tokio::test]
    async fn a_video_by_its_path_arrives_as_the_page_uploads_it() {
        stub();
        let folder = tempfile::tempdir().unwrap();
        let (video, subtitles) = (folder.path().join("My clip.mp4"), folder.path().join("My clip.srt"),
        );
        std::fs::write(&video, vec![1u8; 3 * (1 << 20) + 7]).unwrap();
        let cues = b"1\n00:00:00,000 --> 00:00:01,000\nHi\n";
        std::fs::write(&subtitles, cues).unwrap();
        let reply = call_tool("project_create", json!({ "path": video.to_string_lossy(), "subtitles_path": subtitles.to_string_lossy() })).await;
        let answer = &reply["result"]["structuredContent"];
        assert_eq!(answer["project_id"], "p2", "{reply}");
        assert_eq!(answer["parts"], json!([{ "name": "file", "file": "My clip.mp4", "size": 3 * (1 << 20) + 7 }, { "name": "subs", "file": "My clip.srt", "size": cues.len() }]));
    }

    #[tokio::test]
    async fn a_job_is_waited_for_and_a_frame_waits_for_an_idle_card() {
        stub();
        let done = wait_for(&json!({ "job_id": "j2" })).await.unwrap();
        assert_eq!((done["done"].clone(), done["state"]["result"]["project_id"].clone()), (json!(true), json!("p1")));
        let other_work = wait_for(&json!({ "until": "analyze" })).await.unwrap();
        assert_eq!(other_work["done"], true, "only a render runs");
        let running = wait_for(&json!({ "job_id": "j1", "seconds": 2 })).await.unwrap();
        assert_eq!((running["done"].clone(), running["state"]["stage"].clone()), (json!(false), json!("tts")));
        assert!(wait_for(&json!({ "job_id": "nope" })).await.unwrap_err().starts_with("No job nope"));

        let status = call_tool("studio_status", json!({})).await;
        let summary = &status["result"]["structuredContent"];
        assert_eq!(summary["jobs"][0]["kind"], "render");
        assert_eq!(summary["finished_jobs"][0]["id"], "j2");
        assert_eq!(summary["models"]["missing_required"][0]["id"], "higgs");
        let by_link = summary["jobs"].as_array().unwrap().iter().find(|job| job["id"] == "urla1").expect("a download by link is running work");
        assert_eq!((by_link["kind"].clone(), by_link["status"].clone(), by_link["pct"].clone()), (json!("download"), json!("running"), json!(25.0)));
        assert!(summary["finished_jobs"].as_array().unwrap().iter().any(|job| job["id"] == "urlc3" && job["error"]["code"] == "geo_blocked"));

        let fetched = wait_for(&json!({ "job_id": "urlb2" })).await.unwrap();
        assert_eq!((fetched["done"].clone(), fetched["state"]["result"]["project_id"].clone()), (json!(true), json!("p9")), "a download by link is waited for by its id");
        let failed = wait_for(&json!({ "job_id": "urlc3" })).await.unwrap();
        assert_eq!((failed["done"].clone(), failed["state"]["error"]["hint"].clone()), (json!(true), json!("set a proxy")));
        let running = wait_for(&json!({ "job_id": "urla1", "seconds": 2 })).await.unwrap();
        assert_eq!(running["done"], false);
        assert!(wait_for(&json!({ "job_id": "urlzz" })).await.unwrap_err().starts_with("No download by link urlzz"));
        let downloads = wait_for(&json!({ "until": "download", "seconds": 2 })).await.unwrap();
        assert_eq!(downloads["done"], false, "until download waits for the link too");

        let frame = call_tool("project_frame", json!({ "pid": "p1", "t": 3.0 })).await;
        assert_eq!(frame["result"]["isError"], true);
        assert!(answer_text(&frame).contains("busy: running render of project p1"), "{frame}");
    }

    #[tokio::test]
    async fn a_tool_calls_its_route_as_this_computer() {
        stub();
        let reply = call_route_raw(Call { method: Method::GET, path: "/host".into(), payload: Payload::None,
        }).await.unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&reply.bytes).unwrap()["host"], "127.0.0.1");
    }

    #[tokio::test]
    async fn a_picture_comes_back_as_an_image() {
        stub();
        let reply = call_route_raw(Call { method: Method::GET, path: "/projects/p1/preview?t=1".into(), payload: Payload::None,
        }).await.unwrap();
        let response = tool_image(json!(7), &reply, "project_frame", &json!({ "pid": "p1", "t": 1.0 }),
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let answer: Value = serde_json::from_slice(&bytes).unwrap();
        let content = &answer["result"]["content"];
        assert_eq!((content[0]["type"].clone(), content[0]["mimeType"].clone(), content[0]["data"].clone()), (json!("image"), json!("image/jpeg"), json!("/9j/2Q==")));
        assert!(content[1]["text"].as_str().unwrap().contains("dubbed frame of project p1 at 1 s"));
    }

    #[tokio::test]
    async fn answers_are_short_and_keep_the_key_inside() {
        stub();
        let settings = call_tool("settings_get", json!({})).await;
        let text = answer_text(&settings);
        assert!(!text.contains("sk-or-secret") && !text.contains("user:pass"), "{text}");
        assert_eq!(settings["result"]["structuredContent"]["settings"]["or_key_set"], true);
        let edit = call_tool("segment_update", json!({ "pid": "p1", "id": "s1", "tgt_text": "Привет" }),
        ).await;
        assert_eq!(edit["result"]["structuredContent"]["changed"][0]["id"], "s1", "{edit}");
        let missing = call_tool("openrouter_status", json!({})).await;
        assert_eq!(missing["result"]["isError"], true);
        assert!(answer_text(&missing).contains("no route GET /engine/openrouter/settings"), "{missing}");
    }

    #[tokio::test]
    async fn a_probe_is_what_the_agent_chooses_from() {
        stub();
        for args in [json!({ "url": "https://www.youtube.com/watch?v=abc" }), json!({ "url": "https://www.youtube.com/watch?v=abc", "response_format": "detailed" }),
        ] {
            let reply = call_tool("url_probe", args).await;
            let text = answer_text(&reply);
            assert!(text.len() < 8_000, "{} characters", text.len());
            assert!(!text.contains("thumbnail_data") && !text.contains("base64") && !text.contains("vtt"), "{text}");
            let probe = &reply["result"]["structuredContent"];
            assert_eq!((probe["title"].clone(), probe["tool_version"].clone(), probe["thumbnail"].clone()), (json!("A long clip"), json!("2026.09.01"), json!("https://i.ytimg.com/vi/abc/hqdefault.jpg")), "{reply}");
            assert_eq!(probe["subtitles"], json!([{ "lang": "en", "name": "English" }, { "lang": "ru", "name": "Russian" }]));
            let auto = probe["auto_subtitles"].as_array().unwrap();
            assert_eq!((auto.len(), auto[0].clone()), (157, json!("l000")));
        }
    }

    #[tokio::test]
    async fn a_stateless_request_is_checked_against_its_headers() {
        let call = |headers: &[(&str, &str)], body: Value| {
            let mut map = HeaderMap::new();
            for (name, value) in headers {
                map.insert(axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(), value.parse().unwrap(),
                );
            }
            async move {
                let response = handle(map, axum::body::Bytes::from(body.to_string())).await;
                let status = response.status();
                let bytes = axum::body::to_bytes(response.into_body(), 1 << 22).await.unwrap();
                (status, serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
                )
            }
        };
        let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} });
        let modern = [("mcp-protocol-version", "2026-07-28"), ("mcp-method", "server/discover"),
        ];
        let (status, found) = call(&modern, json!({ "jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": { "_meta": meta } })).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(found["result"]["supportedVersions"][0], "2026-07-28");
        assert_eq!(found["result"]["resultType"], "complete");
        assert_eq!(found["result"]["cacheScope"], "public");
        assert!(found["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"].is_string());

        let (status, found) = call(&[("mcp-protocol-version", "2026-07-28"), ("mcp-method", "tools/call"), ("mcp-name", "studio_system")], json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "studio_status", "_meta": meta } })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "a name that differs from the body is refused");
        assert_eq!(found["error"]["code"], HEADER_MISMATCH);

        let (status, found) = call(&[("mcp-protocol-version", "2026-07-28")], json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": { "_meta": meta } })).await;
        assert_eq!((status, found["error"]["code"].clone()), (StatusCode::BAD_REQUEST, json!(HEADER_MISMATCH)), "a missing Mcp-Method is refused");

        let old = json!({ "io.modelcontextprotocol/protocolVersion": "1900-01-01" });
        let (status, found) = call(&[("mcp-protocol-version", "1900-01-01"), ("mcp-method", "tools/list")], json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": { "_meta": old } })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(found["error"]["code"], UNSUPPORTED_VERSION);
        assert_eq!(found["error"]["data"]["requested"], "1900-01-01");

        let encoded = format!("=?base64?{}?=", { use base64::Engine; base64::engine::general_purpose::STANDARD.encode("studio://skill") });
        let (status, found) = call(&[("mcp-protocol-version", "2026-07-28"), ("mcp-method", "resources/read"), ("mcp-name", encoded.as_str())], json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/read", "params": { "uri": "studio://skill", "_meta": meta } })).await;
        assert_eq!(status, StatusCode::OK, "a Base64 name is decoded before it is compared");
        assert!(found["result"]["contents"][0]["text"].as_str().unwrap().contains("MCP"));

        let (status, found) = call(&[("mcp-protocol-version", "2026-07-28"), ("mcp-method", "nope/nope")], json!({ "jsonrpc": "2.0", "id": 6, "method": "nope/nope", "params": { "_meta": meta } })).await;
        assert_eq!((status, found["error"]["code"].clone()), (StatusCode::NOT_FOUND, json!(-32601)));

        let (status, _) = call(&[], json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        ).await;
        assert_eq!(status, StatusCode::ACCEPTED);

        let mut foreign = HeaderMap::new();
        foreign.insert(header::ORIGIN, "https://evil.example".parse().unwrap());
        let response = handle(foreign, axum::body::Bytes::from(json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }).to_string(),
            ),
        ).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "a web page is refused");
    }

    #[tokio::test]
    async fn the_server_introduces_itself_and_lists_its_tools() {
        let reply = |body: Value| async move {
            let response = handle(HeaderMap::new(), axum::body::Bytes::from(body.to_string())).await;
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 22).await.unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        };
        let hello = reply(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).await;
        assert_eq!(hello["result"]["capabilities"], json!({ "tools": {}, "resources": {}, "prompts": {} }));
        assert_eq!(hello["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(hello["result"]["serverInfo"], json!({ "name": "dub-studio", "title": "Dub Studio", "version": app_version() }));
        let languages = reply(json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/read", "params": { "uri": "studio://languages" } })).await;
        assert!(languages["result"]["contents"][0]["text"].as_str().unwrap().contains("\"ru\""));
        let edits = reply(json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/read", "params": { "uri": "studio://patch-ops" } })).await;
        let edits: Value = serde_json::from_str(edits["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(edits["ops"].as_array().unwrap().len(), every_op().len());
        assert!(edits["ops"][0]["fields"]["tgt_text"].is_object(), "an op lists its tool's fields");
        let skill = reply(json!({ "jsonrpc": "2.0", "id": 6, "method": "prompts/get", "params": { "name": "studio" } })).await;
        assert!(skill["result"]["messages"][0]["content"]["text"].as_str().unwrap().contains("MCP"));
        let dub = reply(json!({ "jsonrpc": "2.0", "id": 7, "method": "prompts/get", "params": { "name": "dub_video", "arguments": { "path": "C:/v/a.mp4", "lang": "es" } } })).await;
        let recipe = dub["result"]["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(recipe.contains("C:/v/a.mp4") && recipe.contains("tgt_lang es") && recipe.contains("mode dub"), "{recipe}");
        let list = reply(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), tools().len());
        assert_eq!(list["result"]["tools"][0]["title"], "Studio status");
        let unknown = reply(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "nope" } })).await;
        assert_eq!(unknown["error"]["code"], -32602);
    }
}
