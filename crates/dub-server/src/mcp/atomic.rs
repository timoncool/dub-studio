//! One call, one result, for a file on this computer: its transcript, its translated subtitles,
//! the file dubbed, its voice and background apart, the text burned into its picture; and a
//! project's subtitles written as a file.
//!
//! Each tool makes a project of the file where it lies (POST /projects/from-path, marked as the
//! agent's), runs the studio's own stages on it through the routes the window calls and waits for
//! them as studio_wait does. The project is keyed by the file and the tool's arguments: calling
//! again finds it with its finished stages and answers at once, or goes on from the stage that is
//! left. A stage that outlasts the call is answered as the job still at work; the agent waits for
//! it with studio_wait and calls the tool again.

use std::path::Path;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};
use tokio::time::Instant;

use super::{call_route, composite, compact_job, fetch, finished, job_rows, object, query, segment, text, Call, Payload, Tool, WAIT_LONGEST};

/// The path the tools' calls carry, handled by `run`.
pub(super) const PREFIX: &str = "composite:atomic:";

/// How long a call waits for the studio, by default, before it answers with the running job.
const WAIT: u64 = 45;

/// The key of the project a file's voice split and on-screen text are worked on.
const MEDIA_KEY: &str = "media";

/// The routes each tool calls, with their holes filled, for the test that every tool reaches
/// only routes the studio has.
#[cfg(test)]
pub(super) const ROUTES: &[(&str, &[(&str, &str)])] = &[
    ("composite:atomic:transcribe_file", &[("POST", "/projects/from-path"), ("GET", "/jobs"), ("GET", "/jobs/x1"), ("GET", "/projects/x1"), ("POST", "/projects/x1/analyze"), ("POST", "/projects/x1/export-text")]),
    ("composite:atomic:translate_file", &[("POST", "/projects/from-path"), ("GET", "/jobs"), ("GET", "/jobs/x1"), ("GET", "/projects/x1"), ("POST", "/projects/x1/analyze"), ("POST", "/projects/x1/export-text")]),
    (
        "composite:atomic:dub_file",
        &[
            ("GET", "/voices"),
            ("POST", "/projects/from-path"),
            ("GET", "/jobs"),
            ("GET", "/jobs/x1"),
            ("GET", "/projects/x1"),
            ("POST", "/projects/x1/analyze"),
            ("PATCH", "/projects/x1"),
            ("GET", "/projects/x1/files"),
            ("POST", "/projects/x1/render"),
            ("POST", "/projects/x1/save-output"),
            ("GET", "/setup/status"),
        ],
    ),
    ("composite:atomic:separate_file", &[("POST", "/projects/from-path"), ("GET", "/jobs"), ("GET", "/jobs/x1"), ("POST", "/projects/x1/separate")]),
    ("composite:atomic:detect_text_file", &[("POST", "/projects/from-path"), ("GET", "/jobs"), ("GET", "/jobs/x1"), ("POST", "/projects/x1/detect-text")]),
    ("composite:atomic:export_subtitles", &[("GET", "/projects/x1"), ("POST", "/projects/x1/export-text")]),
];

fn path_arg() -> Value {
    json!({ "type": "string", "description": "the video or audio file, its full path on this computer" })
}

fn seconds_arg() -> Value {
    json!({ "type": "integer", "description": "how long to wait for the studio before answering with the running job: 45 by default, at most 55" })
}

fn language_arg(what: &str) -> Value {
    json!({ "type": "string", "description": what })
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "transcribe_file",
            description: "The transcript of a video or audio file on this computer in one call, without working in the studio: who speaks when and what they say. format: text (one line per phrase with its speaker, the default), srt, vtt (each cue names its speaker) or json (each line with its id, start, end, speaker, text and word timings). src_lang: the spoken language (a code of studio://languages), auto by default. diarize false leaves the speakers apart untold (quicker, one speaker). The first call makes a project of the file (project_id, listed by projects_list with source agent) and analyzes it; calling again with the same file and arguments answers from it at once. When the analysis takes longer than seconds the answer is done false with the running job: studio_wait with its job_id, then call transcribe_file again. The whole transcript is also written into the project's folder (file); a long one is cut in the answer.",
            schema: || {
                object(
                    json!({
                        "path": path_arg(),
                        "src_lang": language_arg("the spoken language, a code of studio://languages; auto by default"),
                        "diarize": { "type": "boolean", "description": "tell the speakers apart (default true)" },
                        "speaker_count": { "type": "integer", "minimum": 0, "maximum": 8, "description": "Ожидаемое число спикеров на всю запись; 0 — автоматически." },
                        "format": { "type": "string", "enum": ["text", "srt", "vtt", "json"] },
                        "seconds": seconds_arg(),
                    }),
                    &["path"],
                )
            },
            call: |args| {
                text(args, "path")?;
                composite("atomic:transcribe_file")
            },
        },
        Tool {
            name: "translate_file",
            description: "Subtitles of a video or audio file on this computer translated into tgt_lang in one call, without working in the studio: the speech is recognised and translated with the picture as context, as the studio's subtitles mode does. format: srt (the default), vtt or json (each line with its id, start, end, text and the original words). style: the tone of the translation - formal, slang, for children. src_lang: the spoken language, auto by default. The first call makes a project of the file (project_id) and analyzes it; calling again with the same file and arguments answers from it at once. When the work takes longer than seconds the answer is done false with the running job: studio_wait with its job_id, then call translate_file again. The subtitles are also written into the project's folder (file); long ones are cut in the answer.",
            schema: || {
                object(
                    json!({
                        "path": path_arg(),
                        "tgt_lang": language_arg("the language to translate into, a code of studio://languages"),
                        "src_lang": language_arg("the spoken language; auto by default"),
                        "style": { "type": "string", "description": "the tone of the translation" },
                        "format": { "type": "string", "enum": ["srt", "vtt", "json"] },
                        "seconds": seconds_arg(),
                    }),
                    &["path", "tgt_lang"],
                )
            },
            call: |args| {
                text(args, "path")?;
                text(args, "tgt_lang")?;
                composite("atomic:translate_file")
            },
        },
        Tool {
            name: "dub_file",
            description: "A video or audio file on this computer dubbed into tgt_lang in one call, as the studio makes it: speech recognised and translated, the on-screen text blurred and translated, the lines voiced, mixed over the background and rendered. mode: dub (the speakers' voices replaced, the default), voiceover (the translation over the quieted original) or subtitles (the original audio, subtitles burned in). voice: clone (each speaker's own voice, the default), autocast (library voices dealt out by each speaker's gender, the talkiest first: male_voices and female_voices name them, from voices_list) or pack:<name> (one library voice for everyone). subs: translation (the default), original or none; burn false keeps them out of the picture. keep_original adds the original audio as a second track. out_dir copies the result into a folder on this computer as <file>.<tgt_lang> (with (2), (3) when that name is another file's; called again, the copy already there is answered). It answers the finished video and audio, the project_id, what each stage did and what fell back (degradations, each with its stage, code and detail): background_not_separated (no voice separator: the dub has no music or effects under it), ocr_skipped (no on-screen text reader: the text in the picture is left as it is), single_speaker (every line voiced as one speaker), voices_not_cast (with autocast: these speakers kept their own cloned voice), voices_not_as_asked, no_speech. Analysis and render take minutes: when they take longer than seconds the answer is done false with the running job - studio_wait with its job_id, then call dub_file again with the same arguments: the finished stages are kept and the next one starts.",
            schema: || {
                object(
                    json!({
                        "path": path_arg(),
                        "tgt_lang": language_arg("the language to dub into, a code of studio://languages"),
                        "src_lang": language_arg("the spoken language; auto by default"),
                        "mode": { "type": "string", "enum": ["dub", "voiceover", "subtitles"] },
                        "voice": { "type": "string", "description": "clone, autocast or pack:<name of voices_list>" },
                        "male_voices": { "type": "array", "items": { "type": "string" }, "description": "with autocast: library voices for men, the first for the one who talks most" },
                        "female_voices": { "type": "array", "items": { "type": "string" }, "description": "with autocast: library voices for women" },
                        "subs": { "type": "string", "enum": ["translation", "original", "none"] },
                        "burn": { "type": "boolean", "description": "burn the subtitles into the picture (default true)" },
                        "keep_original": { "type": "boolean", "description": "keep the original audio as a second track" },
                        "out_dir": { "type": "string", "description": "a folder on this computer to copy the result into" },
                        "seconds": seconds_arg(),
                    }),
                    &["path", "tgt_lang"],
                )
            },
            call: |args| {
                text(args, "path")?;
                text(args, "tgt_lang")?;
                composite("atomic:dub_file")
            },
        },
        Tool {
            name: "separate_file",
            description: "The voice and the background (music and effects) of a video's or an audio file's sound, apart, in one call, with the studio's separator: it answers the paths of the two 44.1 kHz WAV files (vocals, background) in the project's folder. Calling again with the same file answers them at once. When the separation takes longer than seconds the answer is done false with the running job: studio_wait with its job_id, then call separate_file again.",
            schema: || object(json!({ "path": path_arg(), "seconds": seconds_arg() }), &["path"]),
            call: |args| {
                text(args, "path")?;
                composite("atomic:separate_file")
            },
        },
        Tool {
            name: "detect_text_file",
            description: "The text burned into a video's picture in one call - signs, captions, credits, the original subtitles - as the studio reads it to blur and translate it: each region with its recognised text, its box in pixels (x, y, w, h, on the frame of width and height) and the seconds it is on screen (t0, t1), read at fps frames a second. Calling again with the same file answers at once. When the reading takes longer than seconds the answer is done false with the running job: studio_wait with its job_id, then call detect_text_file again. Everything is also in the project's file text_regions.json; a long list is cut in the answer.",
            schema: || object(json!({ "path": path_arg(), "seconds": seconds_arg() }), &["path"]),
            call: |args| {
                text(args, "path")?;
                composite("atomic:detect_text_file")
            },
        },
        Tool {
            name: "export_subtitles",
            description: "Write a project's lines as a subtitle file: format srt (the default), vtt (each cue names its speaker), ass (styled as the studio burns them, titles included) or txt (one line per phrase with its speaker). lang: the project's target language (its translation, the default) or original (the recognised words; the original language's code works too). Without dir the file goes into the project's folder under its fixed name (subtitles.srt, transcript.vtt, subtitles.ass, ...), replacing the earlier one; name goes with dir, where a name already there gets (2), (3). Answers the path.",
            schema: || {
                object(
                    json!({
                        "project_id": { "type": "string", "description": "project id (projects_list, or a one-call tool's answer)" },
                        "lang": language_arg("the project's target language, or original"),
                        "format": { "type": "string", "enum": ["srt", "vtt", "ass", "txt"] },
                        "dir": { "type": "string", "description": "folder on this computer" },
                        "name": { "type": "string", "description": "the file's name, only together with dir" },
                    }),
                    &["project_id"],
                )
            },
            call: |args| {
                text(args, "project_id")?;
                composite("atomic:export_subtitles")
            },
        },
    ]
}

/// Runs a one-call tool.
pub(super) async fn run(name: &str, args: &Value) -> Result<Value, String> {
    let seconds = args.get("seconds").and_then(Value::as_u64).unwrap_or(WAIT).clamp(1, WAIT_LONGEST);
    let until = Instant::now() + Duration::from_secs(seconds);
    match name {
        "transcribe_file" => transcribe(args, until).await,
        "translate_file" => translate(args, until).await,
        "dub_file" => dub(args, until).await,
        "separate_file" => split(args, until).await,
        "detect_text_file" => read_text(args, until).await,
        "export_subtitles" => export_subtitles(args).await,
        other => Err(format!("{other} is not a one-call tool")),
    }
}

// ---------------------------------------------------------------- arguments

/// A text argument out of a fixed set, or its default when it is left out.
fn choice(args: &Value, name: &str, allowed: &[&str], default: &str) -> Result<String, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(default.to_string()),
        Some(Value::String(value)) if allowed.contains(&value.trim()) => Ok(value.trim().to_string()),
        Some(other) => Err(format!("{name} is one of {}, not {other}", allowed.join(", "))),
    }
}

/// A switch, or its default when it is left out.
fn switch(args: &Value, name: &str, default: bool) -> Result<bool, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(on)) => Ok(*on),
        Some(other) => Err(format!("{name} is true or false, not {other}")),
    }
}

/// A language code of studio://languages; auto where the spoken language may be guessed.
fn language(args: &Value, name: &str, auto: bool) -> Result<String, String> {
    let given = args.get(name).and_then(Value::as_str).map(|code| code.trim().to_lowercase()).filter(|code| !code.is_empty());
    match given {
        None if auto => Ok("auto".into()),
        None => Err(format!("'{name}' is required")),
        Some(code) if auto && code == "auto" => Ok(code),
        Some(code) if dub_translate::WHISPER_LANGS.iter().any(|(known, _)| *known == code) => Ok(code),
        Some(code) => Err(format!("{name} {code} is not a language code of studio://languages")),
    }
}

/// A list of names, empty when it is left out.
fn names(args: &Value, name: &str) -> Result<Vec<String>, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).ok_or_else(|| format!("{name} lists voice names, not {item}")))
            .collect(),
        Some(other) => Err(format!("{name} is a list of voice names, not {other}")),
    }
}

// ---------------------------------------------------------------- routes

/// A route's answer: its status, and its body as JSON or as the text it said.
async fn request(method: Method, path: String, body: Option<Value>) -> Result<(StatusCode, Value), String> {
    let payload = body.map_or(Payload::None, Payload::Json);
    let (status, said) = call_route(Call { method, path, payload }).await?;
    let value = serde_json::from_str(&said).unwrap_or(Value::String(said));
    Ok((status, value))
}

/// Why a route refused, in its own words.
fn refusal(asked: &str, status: StatusCode, said: &Value) -> String {
    let said = said.as_str().map_or_else(|| said.to_string(), str::to_string);
    format!("{asked}: {status} {said}")
}

/// The project of a file for a key, found again or made.
async fn project_of(path: &str, key: &str, tool: &str) -> Result<String, String> {
    let (status, made) = request(Method::POST, "/projects/from-path".into(), Some(json!({ "path": path, "key": key, "tool": tool }))).await?;
    if !status.is_success() {
        return Err(made.as_str().map_or_else(|| made.to_string(), str::to_string));
    }
    made["project_id"].as_str().map(str::to_string).ok_or_else(|| format!("POST /projects/from-path answered no project_id: {made}"))
}

async fn fetch_project(pid: &str) -> Result<Value, String> {
    let path = format!("/projects/{}", segment(pid));
    match request(Method::GET, path.clone(), None).await? {
        (StatusCode::NOT_FOUND, _) => Err(format!("No project {pid}: projects_list names them.")),
        (status, said) if !status.is_success() => Err(refusal(&format!("GET {path}"), status, &said)),
        (_, project) => Ok(project),
    }
}

async fn files(pid: &str) -> Result<Value, String> {
    fetch(&format!("/projects/{}/files", segment(pid))).await
}

/// The project's jobs: the one at work, if any, and its last job as stored with it (project_job).
struct Work {
    active: Option<Value>,
    last: Value,
}

async fn work(pid: &str) -> Result<Work, String> {
    let listed = fetch(&format!("/jobs{}", query(&[("pid", Some(pid.to_string()))]))).await.map_err(|why| format!("The studio's jobs cannot be read, so the work on this file cannot be followed: {why}"))?;
    let active = job_rows(&listed)?.into_iter().find(|job| job["pid"] == pid && !finished(job));
    Ok(Work { active, last: listed.get("project_job").cloned().unwrap_or(Value::Null) })
}

fn job_id(job: &Value) -> Result<String, String> {
    job.get("id").or_else(|| job.get("job_id")).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("a job without an id: {job}"))
}

/// How a wait for a job ended.
enum Waited {
    Done(Value),
    Failed(Value),
    Running(Value),
    Gone,
}

/// Waits for a job until it ends or the time is up, holding the studio's long poll where it
/// has one and looking again every two seconds where it answers at once.
async fn wait(id: &str, until: Instant) -> Result<Waited, String> {
    loop {
        let asked = Instant::now();
        let hold = until.saturating_duration_since(asked).as_secs().clamp(1, 20);
        let path = format!("/jobs/{}{}", segment(id), query(&[("wait", Some(hold.to_string()))]));
        let state = match request(Method::GET, path.clone(), None).await? {
            (StatusCode::NOT_FOUND, _) => return Ok(Waited::Gone),
            (status, said) if !status.is_success() => return Err(refusal(&format!("GET {path}"), status, &said)),
            (_, state) => state,
        };
        let Some(status) = state.get("status").and_then(Value::as_str) else {
            return Err(format!("The job {id} has no status: {state}"));
        };
        if finished(&state) {
            return Ok(if status == "done" { Waited::Done(state) } else { Waited::Failed(state) });
        }
        if Instant::now() >= until {
            return Ok(Waited::Running(state));
        }
        if asked.elapsed() < Duration::from_secs(1) {
            tokio::time::sleep(Duration::from_secs(2).min(until.saturating_duration_since(Instant::now()))).await;
        }
    }
}

/// Waits while a job works on the project; the one still at work when the time is up.
async fn settle(pid: &str, until: Instant) -> Result<Option<Value>, String> {
    loop {
        let Some(active) = work(pid).await?.active else { return Ok(None) };
        if Instant::now() >= until {
            return Ok(Some(active));
        }
        if let Waited::Running(state) = wait(&job_id(&active)?, until).await? {
            return Ok(Some(state));
        }
    }
}

/// Starts a job by its route and waits for it: None when it is done, the job when the time ran
/// out. A job of the same kind already at work on the project is waited for instead.
async fn job_step(route: String, what: &str, until: Instant) -> Result<Option<Value>, String> {
    let (status, answer) = request(Method::POST, route.clone(), Some(json!({}))).await?;
    let id = match (status, answer["error"].as_str(), answer["job_id"].as_str()) {
        (StatusCode::CONFLICT, Some("job_conflict"), Some(id)) => id.to_string(),
        (status, _, Some(id)) if status.is_success() => id.to_string(),
        _ => return Err(refusal(&format!("POST {}", route.split('?').next().unwrap_or_default()), status, &answer)),
    };
    match wait(&id, until).await? {
        Waited::Running(job) => Ok(Some(job)),
        Waited::Failed(job) => Err(format!("The {what} failed: {}", failure(&job))),
        Waited::Done(_) | Waited::Gone => Ok(None),
    }
}

/// A failed job in one line: what, where and why.
fn failure(job: &Value) -> String {
    let why = match &job["error"] {
        Value::String(error) => error.clone(),
        Value::Null => job["status"].as_str().unwrap_or("failed").to_string(),
        error => error.get("text").and_then(Value::as_str).map_or_else(|| error.to_string(), str::to_string),
    };
    format!("{} of project {} ({}): {why}", job["kind"].as_str().unwrap_or("the job"), job["pid"].as_str().unwrap_or("?"), job["status"].as_str().unwrap_or("error"))
}

/// The answer while a job still works on the project: the stage it is, by its kind.
fn pending(tool: &str, pid: &str, job: &Value) -> Value {
    let id = job_id(job).unwrap_or_default();
    let stage = match job["kind"].as_str() {
        Some("analyze") => "analysis",
        Some("separate") => "separation",
        Some("detect_text") => "text reading",
        Some(kind) => kind,
        None => "work",
    };
    json!({
        "done": false,
        "project_id": pid,
        "stage": stage,
        "job": compact_job(job),
        "next": format!("The {stage} is still at work. studio_wait with job_id {id} until it is done, then call {tool} again with the same arguments: the finished stages are kept and it goes on from there."),
    })
}

// ---------------------------------------------------------------- analysis

/// The analysis a tool asks for: the query of POST /projects/{pid}/analyze, in its order.
struct Analysis {
    pairs: Vec<(&'static str, String)>,
}

impl Analysis {
    fn get(&self, name: &str) -> &str {
        self.pairs.iter().find(|(key, _)| *key == name).map_or("", |(_, value)| value.as_str())
    }

    fn query(&self) -> String {
        query(&self.pairs.iter().map(|(key, value)| (*key, Some(value.clone()))).collect::<Vec<_>>())
    }

    /// The project's key: the tool, the analysis and what the tool does after it.
    fn key(&self, tool: &str, more: &[(&str, &str)]) -> String {
        let extra: String = more.iter().map(|(key, value)| format!("&{key}={}", segment(value))).collect();
        format!("{tool}{}{extra}", self.query())
    }

    /// Whether the project holds an analysis as asked: a transcript, in the mode, subtitles and
    /// languages asked for (a project analyzed again from the window with other settings is not).
    fn done(&self, project: &Value) -> bool {
        project["stage_ckpts"].get("asr").is_some()
            && project["mode"] == self.get("mode")
            && project["subs"]["mode"] == self.get("subs")
            && project["tgt_lang"] == self.get("tgt_lang")
            && project["meta"]["src_lang"] == self.get("src_lang")
            && project["meta"]["speaker_count"].as_u64().unwrap_or(0) == self.get("speaker_count").parse::<u64>().unwrap_or(0)
    }
}

/// The project analyzed as asked: at once when it already is, else after its analysis.
enum Analyzed {
    Ready { project: Value, now: bool },
    Waiting(Value),
}

async fn analyze(pid: &str, analysis: &Analysis, until: Instant) -> Result<Analyzed, String> {
    let mut started = false;
    loop {
        if let Some(job) = settle(pid, until).await? {
            return Ok(Analyzed::Waiting(job));
        }
        let project = fetch_project(pid).await?;
        if analysis.done(&project) {
            return Ok(Analyzed::Ready { project, now: started });
        }
        if started {
            return Err(format!(
                "The analysis of project {pid} finished, yet the project does not hold it as asked (mode {}, subtitles {}, language {}): project_get shows what it holds.",
                analysis.get("mode"),
                analysis.get("subs"),
                analysis.get("tgt_lang")
            ));
        }
        if let Some(job) = job_step(format!("/projects/{}/analyze{}", segment(pid), analysis.query()), "analysis", until).await? {
            return Ok(Analyzed::Waiting(job));
        }
        started = true;
    }
}

/// The project's lines written into its folder, their text answered too.
async fn export(pid: &str, format: &str, which: &str) -> Result<Value, String> {
    let route = format!("/projects/{}/export-text", segment(pid));
    let (status, written) = request(Method::POST, route.clone(), Some(json!({ "format": format, "text": which, "content": true }))).await?;
    if !status.is_success() {
        return Err(refusal(&format!("POST {route}"), status, &written));
    }
    Ok(written)
}

/// The text of an export: JSON lines as JSON, the rest as the file's text.
fn content(format: &str, written: &Value) -> Result<Value, String> {
    let said = written["content"].as_str().ok_or_else(|| format!("the export answered no content: {written}"))?;
    if format == "json" {
        serde_json::from_str(said).map_err(|e| format!("the exported JSON does not read back: {e}"))
    } else {
        Ok(said.into())
    }
}

/// The speakers of the project's lines in the order the render gives them voices: sorted, a line
/// without one being speaker 0.
fn speakers(project: &Value) -> Vec<String> {
    let lines = project["segments"].as_array().map(Vec::as_slice).unwrap_or_default();
    let ids: std::collections::BTreeSet<String> = lines.iter().map(|line| line["speaker"].as_str().unwrap_or("0").to_string()).collect();
    ids.into_iter().collect()
}

// ---------------------------------------------------------------- the tools

async fn transcribe(args: &Value, until: Instant) -> Result<Value, String> {
    let path = text(args, "path")?;
    let format = choice(args, "format", &["text", "srt", "vtt", "json"], "text")?;
    let src = language(args, "src_lang", true)?;
    let diarize = switch(args, "diarize", true)?;
    let count = match args.get("speaker_count") {
        None => 0,
        Some(value) => value.as_u64().filter(|n| *n <= 8)
            .ok_or_else(|| "speaker_count: ожидается целое число от 0 до 8".to_string())?,
    };
    if !diarize && count > 1 {
        return Err("speaker_count больше 1 несовместим с diarize=false".into());
    }
    // A transcript with speakers is the studio's transcribe mode; without them it is the
    // subtitles mode in the original language, which neither separates nor tells speakers apart.
    let mut analysis = Analysis {
        pairs: vec![
            ("tgt_lang", if src == "auto" { "en".to_string() } else { src.clone() }),
            ("mode", if diarize { "transcribe" } else { "nodub" }.to_string()),
            ("src_lang", src),
            ("subs", "transcribe".into()),
            ("detect", "0".into()),
            ("casting", "0".into()),
        ],
    };
    if args.get("speaker_count").is_some() {
        analysis.pairs.push(("speaker_count", count.to_string()));
    }
    let pid = project_of(&path, &analysis.key("transcribe_file", &[]), "transcribe_file").await?;
    let project = match analyze(&pid, &analysis, until).await? {
        Analyzed::Waiting(job) => return Ok(pending("transcribe_file", &pid, &job)),
        Analyzed::Ready { project, .. } => project,
    };
    let written = export(&pid, if format == "text" { "txt" } else { &format }, "src").await?;
    Ok(json!({
        "done": true,
        "project_id": pid,
        "format": format,
        "file": written["path"],
        "lines": written["lines"],
        "speakers": speakers(&project).len(),
        "duration": project["meta"]["duration"],
        "transcript": content(&format, &written)?,
    }))
}

async fn translate(args: &Value, until: Instant) -> Result<Value, String> {
    let path = text(args, "path")?;
    let tgt = language(args, "tgt_lang", false)?;
    let src = language(args, "src_lang", true)?;
    let format = choice(args, "format", &["srt", "vtt", "json"], "srt")?;
    if src == tgt {
        return Err(format!("src_lang and tgt_lang are both {tgt}: transcribe_file gives the words in their own language."));
    }
    let style = args.get("style").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
    let mut pairs = vec![("tgt_lang", tgt.clone()), ("mode", "nodub".to_string()), ("src_lang", src), ("subs", "translate".to_string())];
    if !style.is_empty() {
        pairs.push(("translate_style", style));
    }
    pairs.extend([("detect", "0".to_string()), ("casting", "0".to_string())]);
    let analysis = Analysis { pairs };
    let pid = project_of(&path, &analysis.key("translate_file", &[]), "translate_file").await?;
    if let Analyzed::Waiting(job) = analyze(&pid, &analysis, until).await? {
        return Ok(pending("translate_file", &pid, &job));
    }
    let written = export(&pid, &format, "tgt").await?;
    Ok(json!({
        "done": true,
        "project_id": pid,
        "language": tgt,
        "format": format,
        "file": written["path"],
        "lines": written["lines"],
        "subtitles": content(&format, &written)?,
    }))
}

/// How the lines of a dub are voiced.
enum Voices {
    Clone,
    Autocast { male: Vec<String>, female: Vec<String> },
    Pack(String),
}

async fn dub(args: &Value, until: Instant) -> Result<Value, String> {
    let path = text(args, "path")?;
    let tgt = language(args, "tgt_lang", false)?;
    let src = language(args, "src_lang", true)?;
    let mode = choice(args, "mode", &["dub", "voiceover", "subtitles"], "dub")?;
    let subs = choice(args, "subs", &["translation", "original", "none"], "translation")?;
    let burn = switch(args, "burn", true)?;
    let keep = switch(args, "keep_original", false)?;
    let voice = args.get("voice").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).unwrap_or("clone").to_string();
    let (male, female) = (names(args, "male_voices")?, names(args, "female_voices")?);
    let voices = match voice.as_str() {
        "clone" => Voices::Clone,
        "autocast" if male.is_empty() && female.is_empty() => {
            return Err("autocast deals library voices out by gender: name them in male_voices and female_voices (voices_list lists the library).".into())
        }
        "autocast" => Voices::Autocast { male: male.clone(), female: female.clone() },
        other => match other.strip_prefix("pack:").map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => Voices::Pack(name.to_string()),
            None => return Err(format!("voice is clone, autocast or pack:<name of voices_list>, not {other}")),
        },
    };
    if !matches!(voices, Voices::Autocast { .. }) && !(male.is_empty() && female.is_empty()) {
        return Err("male_voices and female_voices go with voice autocast.".into());
    }
    if mode == "subtitles" {
        if subs == "none" || !burn {
            return Err("mode subtitles burns subtitles into the picture: with subs none or burn false the file would be the original.".into());
        }
        if !matches!(voices, Voices::Clone) || keep {
            return Err("voice and keep_original go with mode dub or voiceover; mode subtitles keeps the original audio.".into());
        }
    }
    let out_dir = args.get("out_dir").and_then(Value::as_str).map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
    if let Some(dir) = &out_dir {
        if !tokio::fs::metadata(dir).await.is_ok_and(|meta| meta.is_dir()) {
            return Err(format!("out_dir {dir} is not a folder on this computer"));
        }
    }
    let wanted: Vec<&String> = match &voices {
        Voices::Clone => Vec::new(),
        Voices::Autocast { male, female } => male.iter().chain(female).collect(),
        Voices::Pack(name) => vec![name],
    };
    if !wanted.is_empty() {
        let library = fetch("/voices").await?;
        let known: Vec<&str> = library["voices"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let missing: Vec<&str> = wanted.iter().map(|name| name.as_str()).filter(|name| !known.contains(name)).collect();
        if !missing.is_empty() {
            return Err(format!("The library has no voice {}: voices_list names its voices, voice_download adds one of voices_catalog.", missing.join(", ")));
        }
    }

    let mut pairs = vec![
        ("tgt_lang", tgt.clone()),
        ("mode", if mode == "subtitles" { "nodub".to_string() } else { mode.clone() }),
        ("src_lang", src),
        ("subs", match subs.as_str() { "translation" => "translate", "original" => "transcribe", _ => "none" }.to_string()),
        ("burn", if burn { "1" } else { "0" }.to_string()),
        ("detect", "1".into()),
        ("casting", "0".into()),
    ];
    if keep {
        pairs.extend([("keep_original", "1".to_string()), ("container", "mp4".to_string())]);
    }
    if let Voices::Autocast { male, female } = &voices {
        pairs.push(("voice_slots", json!({ "male": male, "female": female }).to_string()));
    }
    let analysis = Analysis { pairs };
    let voice_key = match &voices {
        Voices::Pack(name) => format!("pack:{name}"),
        _ => String::new(),
    };
    let pid = project_of(&path, &analysis.key("dub_file", &[("voice", &voice_key)]), "dub_file").await?;
    let (project, analyzed_now) = match analyze(&pid, &analysis, until).await? {
        Analyzed::Waiting(job) => return Ok(pending("dub_file", &pid, &job)),
        Analyzed::Ready { project, now } => (project, now),
    };
    if mode == "subtitles" && project["meta"]["width"].as_i64().unwrap_or_default() <= 0 {
        return Err(format!("{path} is audio: there is no picture to burn subtitles into. translate_file writes them as a file."));
    }

    // The voices and the second track go on the analyzed project; the ones given to the analysis
    // (autocast, the second track) are there already where the analysis applied them.
    let mut changed = false;
    if mode != "subtitles" {
        let heard = &project["audio"]["voice"];
        let edit = match &voices {
            Voices::Pack(name) if !(heard["mode"] == "voice" && heard["name"] == name.as_str()) => Some(json!({ "op": "recast", "voice_mode": "voice", "voice_name": name })),
            Voices::Clone if heard["mode"] != "clone" => Some(json!({ "op": "recast", "voice_mode": "clone" })),
            _ => None,
        };
        let track = project["audio"]["keep_original_track"] == true;
        let second = (track != keep).then(|| json!({ "op": "keep_original", "keep": keep, "container": "mp4" }));
        for edit in edit.into_iter().chain(second) {
            let route = format!("/projects/{}", segment(&pid));
            let (status, answer) = request(Method::PATCH, route.clone(), Some(edit)).await?;
            if !status.is_success() {
                return Err(refusal(&format!("PATCH {route}"), status, &answer));
            }
            changed = true;
        }
    }

    let mut rendered_now = false;
    loop {
        if let Some(job) = settle(&pid, until).await? {
            return Ok(pending("dub_file", &pid, &job));
        }
        let made = files(&pid).await?;
        if rendered_now {
            if made["output"].is_null() {
                return Err(format!("The render of project {pid} finished without a video: project_files shows the folder, jobs_list with pid the job."));
            }
            break;
        }
        let current = fetch_project(&pid).await?;
        let dirty = current["segments"].as_array().into_iter().flatten().any(|line| line["dirty"] == true);
        let last = work(&pid).await?.last;
        let up_to_date = !made["output"].is_null() && !dirty && !changed && last["kind"] == "render" && last["state"] == "done";
        if up_to_date {
            break;
        }
        if let Some(job) = job_step(format!("/projects/{}/render", segment(&pid)), "render", until).await? {
            return Ok(pending("dub_file", &pid, &job));
        }
        rendered_now = true;
    }

    let made = files(&pid).await?;
    let output = made["output"].as_str().unwrap_or_default().to_string();
    let is_audio = Path::new(&output).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("wav"));
    let saved = match &out_dir {
        Some(dir) => Some(save(&pid, &output, dir, &format!("{}.{tgt}", stem(&path))).await?),
        None => None,
    };
    let current = fetch_project(&pid).await?;
    let setup = fetch("/setup/status").await?;
    let fell_back = degradations(&current, &made, &setup, &voices)?;
    let heard = &current["audio"]["voice"];
    let voices_used = if mode == "subtitles" {
        json!("none: the original audio")
    } else {
        json!({ "mode": heard["mode"], "names": heard["name"], "second_track": current["audio"]["keep_original_track"] })
    };
    let lines = current["segments"].as_array().map_or(0, Vec::len);
    Ok(json!({
        "done": true,
        "project_id": pid,
        "video": if is_audio { Value::Null } else { output.clone().into() },
        "audio": if is_audio { output.clone().into() } else { made["dub_audio"].clone() },
        "playable": made["playable_output"],
        "saved": saved,
        "lines": lines,
        "stages": {
            "analysis": if analyzed_now { "done now" } else { "done before" },
            "voices": voices_used,
            "render": if rendered_now { "done now" } else { "done before" },
        },
        "degradations": fell_back,
    }))
}

/// The file's name without its folder and extension.
fn stem(path: &str) -> String {
    Path::new(path).file_stem().map_or_else(|| "dub".to_string(), |stem| stem.to_string_lossy().into_owned())
}

/// Copies the result into the folder, unless a copy of it is there already: under the name, or
/// under the name with (2), (3)... that save-output gives a copy when the name is another file's.
async fn save(pid: &str, output: &str, dir: &str, name: &str) -> Result<String, String> {
    let ext = Path::new(output).extension().map_or_else(|| "mp4".to_string(), |ext| ext.to_string_lossy().into_owned());
    let made = tokio::fs::metadata(output).await.map_err(|e| format!("the finished video {output} cannot be read: {e}"))?;
    for n in 1u32.. {
        let file = if n == 1 { format!("{name}.{ext}") } else { format!("{name} ({n}).{ext}") };
        let target = Path::new(dir).join(file);
        let Ok(there) = tokio::fs::metadata(&target).await else { break };
        let not_older = matches!((made.modified(), there.modified()), (Ok(made), Ok(there)) if there >= made);
        if there.is_file() && there.len() == made.len() && not_older {
            return Ok(target.to_string_lossy().into_owned());
        }
    }
    let route = format!("/projects/{}/save-output", segment(pid));
    // the route names the copy after the file name it is given, without that name's extension
    let (status, answer) = request(Method::POST, route.clone(), Some(json!({ "dir": dir, "name": format!("{name}.{ext}") }))).await?;
    if !status.is_success() {
        return Err(refusal(&format!("POST {route}"), status, &answer));
    }
    answer["path"].as_str().map(str::to_string).ok_or_else(|| format!("POST {route} answered no path: {answer}"))
}

/// A component of the studio's models and engines, as GET /setup/status lists it.
fn component<'a>(setup: &'a Value, id: &str) -> Result<&'a Value, String> {
    setup["components"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|component| component["id"] == id)
        .ok_or_else(|| format!("GET /setup/status lists no component {id}, so what fell back in the dub cannot be told"))
}

/// Whether a model the analysis reads on-screen text with is missing: it reads only when both
/// the detector and the recogniser are there (dub_ocr::OcrPaths::all_exist).
fn reader_missing(setup: &Value) -> Result<bool, String> {
    let ocr = dub_ocr::OcrPaths::under(Path::new("models"));
    let missing = component(setup, "ocr")?["missing"].as_array().ok_or_else(|| "GET /setup/status lists no missing files of component ocr".to_string())?;
    Ok(missing.iter().filter_map(Value::as_str).any(|file| Path::new(file) == ocr.det || Path::new(file) == ocr.rec))
}

/// The library voice each speaker is voiced with, None for the speaker's own cloned voice, read
/// from the project's voice as the render reads it: a CSV by speaker, an empty place taking the
/// first name, "-" keeping the clone.
fn voice_of_each(heard: &Value, speakers: &[String]) -> Vec<Option<String>> {
    if heard["mode"] != "voice" {
        return vec![None; speakers.len()];
    }
    let names: Vec<&str> = heard["name"].as_str().unwrap_or_default().split(',').map(str::trim).collect();
    let Some(first) = names.iter().copied().find(|name| !name.is_empty() && *name != crate::voice_slots::CLONE_SLOT) else {
        return vec![None; speakers.len()];
    };
    (0..speakers.len())
        .map(|at| names.get(at).copied().filter(|name| !name.is_empty()).unwrap_or(first))
        .map(|name| (name != crate::voice_slots::CLONE_SLOT).then(|| name.to_string()))
        .collect()
}

/// What fell back on the way, read off the dub as it came out: its lines, its stems, the text
/// found in its picture and the voices it speaks with, and the studio's components where a
/// fallback comes of a model that is not installed.
fn degradations(project: &Value, made: &Value, setup: &Value, voices: &Voices) -> Result<Vec<Value>, String> {
    let mut found = Vec::new();
    let lines = project["segments"].as_array().map_or(0, Vec::len);
    let voiced = matches!(project["mode"].as_str(), Some("dub" | "voiceover"));
    if lines == 0 {
        found.push(json!({ "stage": "analysis", "code": "no_speech", "detail": "no speech was found: nothing is voiced, the sound is the original" }));
    }
    let separated = !made["vocals"].is_null() && !made["background"].is_null();
    if lines > 0 && project["mode"] == "dub" && project["audio"]["keep_music"] != false && !separated {
        found.push(json!({
            "stage": "separation",
            "code": "background_not_separated",
            "detail": "the voice was not split from the background, as the voice separator is not installed (models_status): the dub has no music or effects under it, and the speech was recognised on the mixed sound",
        }));
    }
    // A stage the analysis finished leaves its key in stage_ckpts; the reader and the diarizer fail
    // softly (the analysis goes on without them), so their key missing is what tells a failure.
    let finished = |stage: &str| project["stage_ckpts"].get(stage).is_some();
    let video = project["meta"]["width"].as_i64().unwrap_or_default() > 0;
    if video && !finished("ocr") {
        let detail = if reader_missing(setup)? {
            "the on-screen text reader is not installed (models_status: ocr): the text burned into the picture is neither blurred nor translated"
        } else {
            "the on-screen text reader failed during the analysis (its job's messages say why): the text burned into the picture is neither blurred nor translated"
        };
        found.push(json!({ "stage": "text", "code": "ocr_skipped", "detail": detail }));
    }
    if lines == 0 || !voiced {
        return Ok(found);
    }
    let speakers = speakers(project);
    if speakers.len() == 1 {
        let detail = if component(setup, "sortformer")?["installed"] == false {
            "the diarizer is not installed (models_status: sortformer), so the speakers were not told apart: every line is voiced as one speaker"
        } else if !finished("diarize") {
            "the diarization failed during the analysis (its job's messages say why), so the speakers were not told apart: every line is voiced as one speaker"
        } else {
            "one speaker was heard: every line is voiced as one speaker - right for a clip with one, a sign that the voices were not told apart where there are more"
        };
        found.push(json!({ "stage": "diarization", "code": "single_speaker", "detail": detail }));
    }
    let given = voice_of_each(&project["audio"]["voice"], &speakers);
    let heard = || {
        let each: Vec<String> = speakers.iter().zip(&given).map(|(speaker, voice)| format!("speaker {speaker}: {}", voice.as_deref().unwrap_or("own cloned voice"))).collect();
        each.join(", ")
    };
    match voices {
        Voices::Autocast { .. } => {
            let cloned: Vec<&String> = speakers.iter().zip(&given).filter(|(_, voice)| voice.is_none()).map(|(speaker, _)| speaker).collect();
            if !cloned.is_empty() {
                found.push(json!({
                    "stage": "voices",
                    "code": "voices_not_cast",
                    "speakers": cloned,
                    "detail": "these speakers got no library voice of male_voices and female_voices and speak in their own cloned voice: no voice was given for their gender, or the analysis could not deal the voices out",
                }));
            }
        }
        Voices::Pack(name) if given.iter().any(|voice| voice.as_deref() != Some(name.as_str())) => {
            found.push(json!({ "stage": "voices", "code": "voices_not_as_asked", "detail": format!("not every speaker speaks with {name}: {}", heard()) }));
        }
        Voices::Clone if given.iter().any(Option::is_some) => {
            found.push(json!({ "stage": "voices", "code": "voices_not_as_asked", "detail": format!("not every speaker speaks in their own cloned voice: {}", heard()) }));
        }
        Voices::Pack(_) | Voices::Clone => {}
    }
    Ok(found)
}

/// Runs a single-stage job of the file's media project: at once when its result is there. A job
/// the studio refuses to queue twice (409 job_conflict) is waited for, then the stage is asked
/// for again.
async fn media_stage(tool: &str, route: &str, stage: &str, args: &Value, until: Instant) -> Result<Value, String> {
    let path = text(args, "path")?;
    let pid = project_of(&path, MEDIA_KEY, tool).await?;
    let asked = format!("/projects/{}/{route}", segment(&pid));
    let mut started = false;
    loop {
        if let Some(job) = settle(&pid, until).await? {
            return Ok(pending(tool, &pid, &job));
        }
        let (status, answer) = request(Method::POST, asked.clone(), Some(json!({}))).await?;
        if status.is_success() && answer["cached"] == true {
            return Ok(finish(&pid, answer));
        }
        let (id, own) = match (status, answer["error"].as_str(), answer["job_id"].as_str()) {
            (StatusCode::CONFLICT, Some("job_conflict"), Some(id)) => (id.to_string(), false),
            (status, _, Some(id)) if status.is_success() => (id.to_string(), true),
            _ => return Err(refusal(&format!("POST {asked}"), status, &answer)),
        };
        if own && started {
            return Err(format!("The {stage} of project {pid} finished, yet its result is not there: POST {asked} started it again."));
        }
        match wait(&id, until).await? {
            Waited::Running(job) => return Ok(pending(tool, &pid, &job)),
            Waited::Failed(job) if own => return Err(format!("The {stage} failed: {}", failure(&job))),
            Waited::Done(job) if own => return Ok(finish(&pid, job["result"].clone())),
            Waited::Failed(_) | Waited::Done(_) | Waited::Gone => started |= own,
        }
    }
}

/// A single stage's result with the project it was made in.
fn finish(pid: &str, mut result: Value) -> Value {
    if let Some(fields) = result.as_object_mut() {
        fields.remove("cached");
        fields.insert("done".into(), true.into());
        fields.insert("project_id".into(), pid.into());
    }
    result
}

async fn split(args: &Value, until: Instant) -> Result<Value, String> {
    media_stage("separate_file", "separate", "separation", args, until).await
}

async fn read_text(args: &Value, until: Instant) -> Result<Value, String> {
    media_stage("detect_text_file", "detect-text", "text reading", args, until).await
}

async fn export_subtitles(args: &Value) -> Result<Value, String> {
    let pid = text(args, "project_id")?;
    let format = choice(args, "format", &["srt", "vtt", "ass", "txt"], "srt")?;
    let project = fetch_project(&pid).await?;
    if project["segments"].as_array().is_none_or(Vec::is_empty) {
        return Err(format!("Project {pid} has no lines yet: transcribe_file, translate_file or project_analyze makes them."));
    }
    let which = lines_of(&pid, &project, args.get("lang").and_then(Value::as_str))?;
    let mut body = json!({ "format": format, "text": which });
    for field in ["dir", "name"] {
        if let Some(value) = args.get(field).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
            body[field] = value.into();
        }
    }
    let route = format!("/projects/{}/export-text", segment(&pid));
    let (status, written) = request(Method::POST, route.clone(), Some(body)).await?;
    if !status.is_success() {
        return Err(refusal(&format!("POST {route}"), status, &written));
    }
    Ok(json!({
        "project_id": pid,
        "path": written["path"],
        "lines": written["lines"],
        "format": format,
        "text": if which == "tgt" { "translation" } else { "transcript" },
    }))
}

/// Which lines a language asks for: tgt, the translation, or src, the recognised words.
fn lines_of(pid: &str, project: &Value, lang: Option<&str>) -> Result<&'static str, String> {
    let translated = matches!(project["mode"].as_str(), Some("dub" | "voiceover")) || matches!(project["subs"]["mode"].as_str(), Some("translate" | "bilingual"));
    let tgt = project["tgt_lang"].as_str().unwrap_or_default().to_lowercase();
    let src = project["meta"]["src_lang"].as_str().unwrap_or("auto").to_lowercase();
    let lang = lang.map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty());
    let original = if src == "auto" { "original".to_string() } else { format!("original ({src})") };
    match lang.as_deref() {
        None if translated => Ok("tgt"),
        None | Some("original") => Ok("src"),
        Some(l) if translated && l == tgt => Ok("tgt"),
        Some(l) if src != "auto" && l == src => Ok("src"),
        Some(l) if translated => Err(format!("Project {pid} holds its translation into {tgt} and its {original} words: lang {l} is neither. Another language: translate_file, or project_export_lang.")),
        Some(l) => Err(format!("Project {pid} is a transcript of its {original} words, without a translation: lang {l} is not there. A translation: translate_file.")),
    }
}

#[cfg(test)]
pub(super) mod stub;

#[cfg(test)]
mod tests;
