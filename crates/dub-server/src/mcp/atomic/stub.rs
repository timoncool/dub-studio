//! The one-call tools' side of the tests' studio: a project is made of a file named after it
//! (C:/media/tr1.mp4 is project tr1, planned by the test first), its jobs end the first time they
//! are looked at - or never, or with an error, as the test plans - and every request it gets is
//! logged. Analysis, render, separation and reading behave as the routes say they do.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use axum::extract::{Path as Segment, RawQuery};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

/// How a job of the test ends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Ends {
    Done,
    Never,
    Fails,
}

#[derive(Clone)]
pub(crate) struct Plan {
    pub analyze: Ends,
    pub render: Ends,
    pub stage: Ends,
    /// POST analyze or separate answers 409 once, with a job of its kind already at work.
    pub conflict: bool,
    /// The voice separator is installed: the analysis of a voiced mode and the render of a dub
    /// leave the stems.
    pub separator: bool,
    /// The video the render makes, a real file for save-output to copy.
    pub output: Option<String>,
    /// The file is audio: no picture.
    pub audio: bool,
}

impl Default for Plan {
    fn default() -> Self {
        Plan { analyze: Ends::Done, render: Ends::Done, stage: Ends::Done, conflict: false, separator: true, output: None, audio: false }
    }
}

struct Project {
    plan: Plan,
    body: Value,
    rendered: bool,
    stems: bool,
    regions: bool,
    last: Value,
    log: Vec<String>,
}

struct Job {
    pid: String,
    kind: &'static str,
    ends: Ends,
    looked: bool,
    query: String,
}

#[derive(Default)]
struct Studio {
    projects: HashMap<String, Project>,
    jobs: HashMap<String, Job>,
    made: u32,
}

fn studio() -> MutexGuard<'static, Studio> {
    static STUDIO: OnceLock<Mutex<Studio>> = OnceLock::new();
    STUDIO.get_or_init(|| Mutex::new(Studio::default())).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A project the test works on, fresh: made of its file, not analyzed yet.
pub(crate) fn plan(pid: &str, plan: Plan) {
    let (width, height) = if plan.audio { (0, 0) } else { (1280, 720) };
    let body = json!({
        "meta": { "video": format!("C:/media/{pid}.mp4"), "duration": 4.0, "width": width, "height": height, "fps": 25.0 },
        "mode": "nodub", "tgt_lang": "ru", "subs": { "mode": "none", "burn": true },
        "audio": { "voice": { "mode": "clone", "name": null }, "keep_original_track": false, "container": "mp4" },
        "captions": {}, "render": {}, "stage_ckpts": {}, "segments": [],
    });
    let project = Project { plan, body, rendered: false, stems: false, regions: false, last: Value::Null, log: Vec::new() };
    studio().projects.insert(pid.to_string(), project);
}

/// The requests the project got, in order: "METHOD /path?query", then what was sent.
pub(crate) fn log(pid: &str) -> Vec<String> {
    studio().projects.get(pid).map(|project| project.log.clone()).unwrap_or_default()
}

pub(crate) fn forget_log(pid: &str) {
    if let Some(project) = studio().projects.get_mut(pid) {
        project.log.clear();
    }
}

fn note(studio: &mut Studio, pid: &str, entry: String) {
    if let Some(project) = studio.projects.get_mut(pid) {
        project.log.push(entry);
    }
}

pub(crate) fn project(pid: &str) -> Option<Response> {
    let mut studio = studio();
    note(&mut studio, pid, format!("GET /projects/{pid}"));
    studio.projects.get(pid).map(|project| Json(project.body.clone()).into_response())
}

pub(crate) fn patch(pid: &str, edit: &Value) -> Option<Response> {
    let mut studio = studio();
    note(&mut studio, pid, format!("PATCH /projects/{pid} {edit}"));
    let project = studio.projects.get_mut(pid)?;
    let audio = &mut project.body["audio"];
    match edit["op"].as_str() {
        Some("recast") => {
            audio["voice"] = json!({ "mode": edit["voice_mode"], "name": edit.get("voice_name").cloned().unwrap_or(Value::Null) });
            for line in project.body["segments"].as_array_mut().into_iter().flatten() {
                line["dirty"] = true.into();
            }
        }
        Some("keep_original") => {
            audio["keep_original_track"] = edit["keep"].clone();
            audio["container"] = edit["container"].clone();
        }
        other => return Some((StatusCode::BAD_REQUEST, format!("the stub makes no {other:?}")).into_response()),
    }
    Some(Json(project.body.clone()).into_response())
}

fn status(job: &Job) -> &'static str {
    match (job.looked, job.ends) {
        (false, _) | (true, Ends::Never) => "running",
        (true, Ends::Done) => "done",
        (true, Ends::Fails) => "error",
    }
}

fn snapshot(id: &str, job: &Job, result: Value) -> Value {
    let mut state = json!({ "id": id, "kind": job.kind, "pid": job.pid, "status": status(job), "stage": job.kind, "msg": "", "pct": 50.0 });
    match status(job) {
        "done" => state["result"] = result,
        "error" => state["error"] = format!("{} broke on purpose", job.kind).into(),
        _ => {}
    }
    state
}

/// GET /jobs?pid= of a planned project; None for the other tests' projects.
pub(crate) fn jobs(pid: &str) -> Option<Value> {
    let mut studio = studio();
    note(&mut studio, pid, format!("GET /jobs?pid={pid}"));
    let project = studio.projects.get(pid)?;
    let listed: Vec<Value> = studio.jobs.iter().filter(|(_, job)| job.pid == pid).map(|(id, job)| snapshot(id, job, result_of(job))).collect();
    Some(json!({ "jobs": listed, "project_job": project.last }))
}

/// GET /jobs/{id} of a planned project's job: the first look ends a job that is to end.
pub(crate) fn job(id: &str) -> Option<Value> {
    let mut studio = studio();
    let (pid, first) = {
        let job = studio.jobs.get_mut(id)?;
        let first = !job.looked;
        job.looked = true;
        (job.pid.clone(), first)
    };
    note(&mut studio, &pid, format!("GET /jobs/{id}"));
    let Studio { projects, jobs, .. } = &mut *studio;
    let job = jobs.get(id)?;
    let project = projects.get_mut(&pid)?;
    if first && job.ends == Ends::Done {
        finish(job, id, project);
    }
    Some(snapshot(id, job, result_of(job)))
}

/// What a finished job leaves in its project.
fn finish(job: &Job, id: &str, project: &mut Project) {
    match job.kind {
        "analyze" => {
            project.body = analyzed(&project.body, &job.query);
            project.stems |= project.plan.separator && project.body["mode"] != "nodub";
            project.last = json!({ "kind": "analyze", "state": "done", "job_id": id });
        }
        "render" => {
            project.rendered = true;
            project.stems |= project.plan.separator && project.body["mode"] == "dub";
            for line in project.body["segments"].as_array_mut().into_iter().flatten() {
                line["dirty"] = false.into();
            }
            project.last = json!({ "kind": "render", "state": "done", "job_id": id });
        }
        "separate" => project.stems = true,
        _ => project.regions = true,
    }
}

fn result_of(job: &Job) -> Value {
    match job.kind {
        "analyze" => json!({ "project_id": job.pid }),
        "render" => json!({ "output": "output.mp4" }),
        "separate" => stems(&job.pid),
        _ => regions(&job.pid),
    }
}

fn stems(pid: &str) -> Value {
    json!({ "vocals": format!("C:/w/{pid}/stems/vocals.wav"), "background": format!("C:/w/{pid}/stems/instrumental.wav") })
}

fn regions(pid: &str) -> Value {
    json!({ "file": format!("C:/w/{pid}/text_regions.json"), "width": 1280, "height": 720, "fps": 4, "count": 1, "regions": [{ "text": "EXIT", "x": 10, "y": 20, "w": 120, "h": 40, "t0": 1.0, "t1": 3.5 }] })
}

/// The project an analysis with this query makes: two lines, two speakers where it tells them
/// apart (speaker 0 a man, speaker 1 a woman), translated where the mode translates, and what the
/// analysis applies after itself: autocast gives each the first voice of their gender, or leaves
/// them cloned ("-") where none is given.
fn analyzed(before: &Value, query: &str) -> Value {
    let uri: axum::http::Uri = format!("/analyze?{query}").parse().expect("the analysis's query is a URI's");
    let asked: HashMap<String, String> = axum::extract::Query::try_from_uri(&uri).expect("the analysis's query decodes").0;
    let get = |key: &str| asked.get(key).cloned().unwrap_or_default();
    let (mode, subs, tgt) = (get("mode"), get("subs"), get("tgt_lang"));
    let translated = mode == "dub" || mode == "voiceover" || subs == "translate" || subs == "bilingual";
    let speakers = mode != "nodub";
    let line = |id: &str, start: f64, end: f64, speaker: &str, words: &str| {
        let tgt_text = if translated { format!("[{tgt}] {words}") } else { words.to_string() };
        let timed: Vec<Value> = words.split(' ').enumerate().map(|(at, word)| json!({ "word": word, "start": start + at as f64 * 0.25, "end": start + (at + 1) as f64 * 0.25 })).collect();
        json!({ "id": id, "start": start, "end": end, "speaker": speaker, "src_text": words, "tgt_text": tgt_text, "dirty": false, "words": timed })
    };
    let mut after = before.clone();
    after["mode"] = mode.clone().into();
    after["tgt_lang"] = tgt.clone().into();
    after["subs"] = json!({ "mode": subs, "burn": get("burn") != "0" });
    after["meta"]["src_lang"] = get("src_lang").into();
    after["meta"]["speaker_count"] = get("speaker_count").parse::<u64>().unwrap_or(0).into();
    after["stage_ckpts"] = json!({ "asr": "k", "translate": "t" });
    if get("detect") == "1" {
        after["stage_ckpts"]["ocr"] = json!("o");
    }
    if speakers {
        after["stage_ckpts"]["diarize"] = json!("d");
    }
    after["segments"] = json!([line("s0", 0.5, 1.5, "0", "Hello there"), line("s1", 2.0, 3.0, if speakers { "1" } else { "0" }, "Bye now")]);
    after["audio"]["voice"] = match asked.get("voice_slots") {
        None => json!({ "mode": "clone", "name": null }),
        Some(slots) => {
            let slots: Value = serde_json::from_str(slots).expect("voice_slots is JSON");
            let first = |gender: &str| slots[gender][0].as_str().unwrap_or("-").to_string();
            let names = if speakers { vec![first("male"), first("female")] } else { vec![first("male")] };
            if names.iter().all(|name| name == "-") {
                json!({ "mode": "clone", "name": null })
            } else {
                json!({ "mode": "voice", "name": names.join(",") })
            }
        }
    };
    after["audio"]["keep_original_track"] = (get("keep_original") == "1").into();
    after
}

fn start(studio: &mut Studio, pid: &str, kind: &'static str, query: String) -> String {
    studio.made += 1;
    let id = format!("job{}", studio.made);
    let ends = match (kind, studio.projects.get(pid)) {
        ("analyze", Some(project)) => project.plan.analyze,
        ("render", Some(project)) => project.plan.render,
        (_, Some(project)) => project.plan.stage,
        (_, None) => Ends::Fails,
    };
    studio.jobs.insert(id.clone(), Job { pid: pid.to_string(), kind, ends, looked: false, query });
    id
}

fn unplanned(pid: &str) -> Response {
    (StatusCode::NOT_FOUND, format!("project not found: the test did not plan {pid}")).into_response()
}

/// The routes only the one-call tools call.
pub(crate) fn routes() -> Router {
    Router::new()
        .route(
            "/projects/from-path",
            post(|Json(body): Json<Value>| async move {
                let path = body["path"].as_str().unwrap_or_default().to_string();
                let pid = Path::new(&path).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
                let mut studio = studio();
                if !studio.projects.contains_key(&pid) {
                    return (StatusCode::BAD_REQUEST, format!("{path} is not a file on this computer")).into_response();
                }
                note(&mut studio, &pid, format!("POST /projects/from-path key={} tool={}", body["key"].as_str().unwrap_or_default(), body["tool"].as_str().unwrap_or_default()));
                Json(json!({ "project_id": pid, "reused": false, "filename": format!("{pid}.mp4") })).into_response()
            }),
        )
        .route(
            "/projects/{pid}/analyze",
            post(|Segment(pid): Segment<String>, RawQuery(query): RawQuery| async move {
                let query = query.unwrap_or_default();
                let mut studio = studio();
                note(&mut studio, &pid, format!("POST /projects/{pid}/analyze?{query}"));
                let Some(project) = studio.projects.get_mut(&pid) else { return unplanned(&pid) };
                let conflict = std::mem::take(&mut project.plan.conflict);
                let id = start(&mut studio, &pid, "analyze", query);
                if conflict {
                    return (StatusCode::CONFLICT, Json(json!({ "error": "job_conflict", "job_id": id, "kind": "analyze" }))).into_response();
                }
                Json(json!({ "job_id": id })).into_response()
            }),
        )
        .route(
            "/projects/{pid}/render",
            post(|Segment(pid): Segment<String>| async move {
                let mut studio = studio();
                note(&mut studio, &pid, format!("POST /projects/{pid}/render"));
                if !studio.projects.contains_key(&pid) {
                    return unplanned(&pid);
                }
                Json(json!({ "job_id": start(&mut studio, &pid, "render", String::new()) })).into_response()
            }),
        )
        .route(
            "/projects/{pid}/separate",
            post(|Segment(pid): Segment<String>| async move {
                let mut studio = studio();
                note(&mut studio, &pid, format!("POST /projects/{pid}/separate"));
                match studio.projects.get(&pid) {
                    None => unplanned(&pid),
                    Some(project) if project.stems => {
                        let mut found = stems(&pid);
                        found["cached"] = true.into();
                        Json(found).into_response()
                    }
                    Some(project) if project.plan.conflict => {
                        studio.projects.get_mut(&pid).expect("planned").plan.conflict = false;
                        let id = start(&mut studio, &pid, "separate", String::new());
                        (StatusCode::CONFLICT, Json(json!({ "error": "job_conflict", "job_id": id, "kind": "separate" }))).into_response()
                    }
                    Some(_) => Json(json!({ "job_id": start(&mut studio, &pid, "separate", String::new()) })).into_response(),
                }
            }),
        )
        .route(
            "/projects/{pid}/detect-text",
            post(|Segment(pid): Segment<String>| async move {
                let mut studio = studio();
                note(&mut studio, &pid, format!("POST /projects/{pid}/detect-text"));
                match studio.projects.get(&pid) {
                    None => unplanned(&pid),
                    Some(project) if project.regions => {
                        let mut found = regions(&pid);
                        found["cached"] = true.into();
                        Json(found).into_response()
                    }
                    Some(_) => Json(json!({ "job_id": start(&mut studio, &pid, "detect_text", String::new()) })).into_response(),
                }
            }),
        )
        .route(
            "/projects/{pid}/export-text",
            post(|Segment(pid): Segment<String>, Json(body): Json<Value>| async move {
                let mut studio = studio();
                let (format, which) = (body["format"].as_str().unwrap_or_default(), body["text"].as_str().unwrap_or("tgt"));
                note(&mut studio, &pid, format!("POST /projects/{pid}/export-text {format} {which}"));
                let Some(project) = studio.projects.get(&pid) else { return unplanned(&pid) };
                let proj: dub_core::Project = serde_json::from_value(project.body.clone()).expect("the stub's project is a Project");
                let text = if which == "src" { crate::project_files::Which::Src } else { crate::project_files::Which::Tgt };
                let rows = crate::project_files::lines(&proj, text);
                let file = match body["dir"].as_str() {
                    Some(dir) => format!("{dir}/{}.{format}", body["name"].as_str().unwrap_or("export")),
                    None => format!("C:/w/{pid}/export.{format}"),
                };
                let mut answer = json!({ "ok": true, "path": file, "lines": rows.len() });
                if body["content"] == true {
                    answer["content"] = crate::project_files::render_text(&rows, format, "Speaker").into();
                }
                Json(answer).into_response()
            }),
        )
        .route(
            "/projects/{pid}/files",
            get(|Segment(pid): Segment<String>| async move {
                let mut studio = studio();
                note(&mut studio, &pid, format!("GET /projects/{pid}/files"));
                let Some(project) = studio.projects.get(&pid) else { return unplanned(&pid) };
                let output = match (&project.plan.output, project.rendered) {
                    (_, false) => Value::Null,
                    (Some(real), true) => real.clone().into(),
                    (None, true) => format!("C:/w/{pid}/output.mp4").into(),
                };
                let (vocals, background) = if project.stems { (stems(&pid)["vocals"].clone(), stems(&pid)["background"].clone()) } else { (Value::Null, Value::Null) };
                Json(json!({ "folder": format!("C:/w/{pid}"), "output": output, "playable_output": output, "dub_audio": null, "vocals": vocals, "background": background })).into_response()
            }),
        )
        .route(
            "/projects/{pid}/save-output",
            post(|Segment(pid): Segment<String>, Json(body): Json<Value>| async move {
                let mut studio = studio();
                note(&mut studio, &pid, format!("POST /projects/{pid}/save-output {body}"));
                let Some(project) = studio.projects.get(&pid) else { return unplanned(&pid) };
                let name = body["name"].as_str().unwrap_or("output");
                let stem = Path::new(name).file_stem().and_then(|stem| stem.to_str()).unwrap_or(name);
                let folder = Path::new(body["dir"].as_str().unwrap_or_default());
                let mut target = folder.join(format!("{stem}.mp4"));
                let mut n = 2;
                while target.exists() {
                    target = folder.join(format!("{stem} ({n}).mp4"));
                    n += 1;
                }
                if let Some(made) = &project.plan.output {
                    std::fs::copy(made, &target).expect("the stub copies the render");
                }
                Json(json!({ "ok": true, "path": target.to_string_lossy() })).into_response()
            }),
        )
        .route("/voices", get(|| async { Json(json!({ "voices": ["Anna", "Boris", "Vera"] })) }))
}
