//! Google Gemini TTS as the voicing of a render, the same planning as Speech Studio's `app/google_speech.rs`: model
//! and mode from «Cloud», the voice of each phrase. The render hands all its phrases at once ([`synth_jobs`]) and gets
//! their sound in the cache of the project folder: the standard mode with parallel requests, Batch (half the price)
//! as one batch, «auto» as a batch from eleven phrases on. The cache key is the model, voice, text, direction and take:
//! voicing a line again is a new request, a retry after a failure is sound already paid for. The batch journal is
//! written before sending: after a failure the batch is found in Google by its name, not sent a second time. Every job
//! ends with a report in the project folder (`cloud/google-report.json`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const SR: u32 = 24_000;
const BATCH_POLL_SECS: u64 = 30;
/// Сколько фраз «авто» отправляет прямыми запросами: больше — пачкой (правило Game Dub для доводки).
const AUTO_DIRECT_MAX: usize = 10;
/// Пачка, которой нет в списке Google через столько после отправки, не создалась.
const UNLISTED_AFTER_SECS: i64 = 30 * 60;
/// Чтение статуса и результатов повторяется: обрыв сети за часы ожидания не должен ронять задачу.
const READ_ATTEMPTS: u32 = 20;
/// Обычный запрос при 429 и 5xx повторяется столько раз.
const DIRECT_ATTEMPTS: u32 = 3;
/// The stage the render's journal shows Google's progress under.
const STAGE: &str = "tts";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Standard,
    Batch,
    Auto,
}

pub struct Setup {
    pub model: String,
    pub voice: String,
    pub mode: Mode,
}

/// A phrase of the job: text, direction and take; `voice` is the phrase's own (a dub's speaker), empty for the
/// project's, and `tag` what else tells two takes of one text apart (a dub's line cache key), empty for none.
pub struct Want {
    pub text: String,
    pub style: String,
    pub n: u32,
    pub voice: String,
    pub tag: String,
}

/// Модель (проекта, иначе студии), голос проекта и режим студии; без модели или голоса — ошибка с тем, что задать.
pub fn setup(models_root: &Path, model: &str, voice: &str) -> Result<Setup, String> {
    let sel = crate::models::load_selection(models_root);
    let model = match model.trim() {
        "" => sel.get("google_tts_model").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        m => m.to_string(),
    };
    let model = if model.is_empty() { crate::google_tts::DEFAULT_MODEL.to_string() } else { model };
    let voice = match voice.trim() {
        "" => crate::cloud_voices::default_voice(&model).ok_or_else(|| t!("google-no-voice"))?,
        v => v,
    };
    let mode = match sel.get("google_tts_mode").and_then(Value::as_str) {
        Some("batch") => Mode::Batch,
        Some("auto") => Mode::Auto,
        _ => Mode::Standard,
    };
    Ok(Setup { model, voice: voice.to_string(), mode })
}

fn client() -> Result<crate::google_tts::Client, String> {
    let (key, _) = crate::credentials::google_api_key().ok_or_else(|| t!("cloud-no-key", provider = "Google"))?;
    crate::google_tts::Client::new(key)
}

fn voice_of<'a>(s: &'a Setup, w: &'a Want) -> &'a str {
    if w.voice.trim().is_empty() { &s.voice } else { w.voice.trim() }
}

fn key_of(s: &Setup, w: &Want) -> String {
    let mut basis = format!("{}|{}|{}|{}|{}", s.model, voice_of(s, w), w.text, w.style, w.n);
    if !w.tag.is_empty() {
        basis.push('|');
        basis.push_str(&w.tag);
    }
    blake3::hash(basis.as_bytes()).to_hex()[..20].to_string()
}

fn cloud_dir(project_dir: &Path) -> PathBuf {
    project_dir.join("cloud")
}

fn cache(project_dir: &Path, key: &str) -> PathBuf {
    cloud_dir(project_dir).join("google").join(format!("{key}.wav"))
}

/// Фразы, которые задача не получила, с причиной: блок с такой фразой падает с ней, а не идёт другим путём.
fn failed_path(project_dir: &Path) -> PathBuf {
    cloud_dir(project_dir).join("google-failed.json")
}

fn journal_path(project_dir: &Path) -> PathBuf {
    cloud_dir(project_dir).join("google-batch.json")
}

fn report_path(project_dir: &Path) -> PathBuf {
    cloud_dir(project_dir).join("google-report.json")
}

fn history_dir(project_dir: &Path) -> PathBuf {
    cloud_dir(project_dir).join("google-batches")
}

/// Звук ответа Google: PCM L16 24 кГц как есть, другой формат — через ffmpeg.
fn samples_of(mime: &str, bytes: &[u8]) -> Result<Vec<f32>, String> {
    if mime.contains("L16") || mime.contains("pcm") {
        return Ok(bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect());
    }
    let dir = tempfile::Builder::new().prefix("google-").tempdir().map_err(|e| e.to_string())?;
    let (src, wav) = (dir.path().join("in.bin"), dir.path().join("out.wav"));
    std::fs::write(&src, bytes).map_err(|e| e.to_string())?;
    crate::media::extract_audio(&src, &wav, SR, 1)?;
    crate::wavio::read_mono_f32(&wav).map(|(s, _)| s)
}

fn spend(repo_root: &Path, pid: &str, s: &Setup, usage: &Value, batch: bool) -> Option<f64> {
    let cost = crate::google_tts::estimate(&s.model, batch, usage).get("usd").and_then(Value::as_f64);
    crate::spending::record(repo_root, crate::spending::Spent {
        at: chrono::Utc::now().timestamp_millis(),
        day: chrono::Local::now().format("%Y-%m-%d").to_string(),
        project: pid.to_string(),
        provider: if batch { "google-batch" } else { "google" }.into(),
        model: s.model.clone(),
        task: "dub".into(),
        estimate: cost,
        cost: None,
        cost_source: if cost.is_some() { "estimate" } else { "unknown" }.into(),
        origin: crate::mcp::current_actor(),
    });
    cost
}

/// Ответ с целым клипом — в кэш; иначе ошибка с причиной Google (фильтр, обрыв).
fn keep(project_dir: &Path, key: &str, response: &Value) -> Result<Vec<f32>, String> {
    let (mime, bytes) = crate::google_tts::audio(response)?;
    let samples = samples_of(&mime, &bytes)?;
    let path = cache(project_dir, key);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    crate::wavio::write_mono_f32(&path, &samples, SR)?;
    Ok(samples)
}

fn cancelled() -> bool {
    crate::jobs::current().is_some_and(|c| c.is_cancelled())
}

/// Пауза с проверкой отмены каждую секунду.
fn pause(secs: u64) -> Result<(), String> {
    for _ in 0..secs {
        if cancelled() {
            return Err(crate::jobs::CANCELLED.to_string());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Повтор того, что безопасно повторять (статус, скачивание): ждём дольше с каждой неудачей.
fn retried<T>(what: &str, progress: &dyn Fn(Value), mut f: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if attempt < READ_ATTEMPTS => {
                let wait = 30 * u64::from(attempt.min(10));
                progress(json!({ "type": "progress", "stage": STAGE, "msg": t!("google-retry", what = what.to_string(), attempt = attempt as i64, of = READ_ATTEMPTS as i64, secs = wait as i64, error = e.chars().take(200).collect::<String>()) }));
                pause(wait)?;
            }
            Err(e) => return Err(t!("google-read-failed", what = what.to_string(), attempts = READ_ATTEMPTS as i64, error = explain(&e))),
        }
    }
}

/// Отказ по деньгам или доступу (предоплата кончилась, потолок трат, ключ без прав): следующие запросы не пройдут.
fn account_refused(error: &str) -> bool {
    error.contains(" 402 ") || error.contains(" 403 ") || error.contains("spending cap") || error.contains("prepayment")
}

/// Ответ Google, который пользователь может исправить сам, — фразой с действием; остальное — как прислал Google.
pub fn explain(error: &str) -> String {
    let low = error.to_lowercase();
    let code = [" 402 ", " 403 ", " 429 ", " 400 "].iter().find(|c| error.contains(**c)).map(|c| c.trim()).unwrap_or("");
    let said = if low.contains("prepayment") || code == "402" {
        t!("google-err-prepayment")
    } else if low.contains("spending cap") {
        t!("google-err-cap")
    } else if low.contains("api_key_invalid") || low.contains("api key not valid") {
        t!("google-err-key")
    } else if code == "403" {
        t!("google-err-access")
    } else if code == "429" {
        t!("google-err-rate")
    } else if low.contains("prohibited_content") || low.contains("block reason") || low.contains("\"safety\"") {
        return t!("google-err-filter");
    } else {
        return error.to_string();
    };
    if code.is_empty() { said } else { format!("{said} (Google {code})") }
}

/// Обычный запрос: 429 и 5xx повторяются с паузой, отказ фильтра и прочее — сразу ошибка.
fn direct(c: &crate::google_tts::Client, s: &Setup, voice: &str, text: &str, style: &str) -> Result<Value, String> {
    let request = crate::google_tts::request(text, voice, style)?;
    let mut attempt = 0;
    loop {
        attempt += 1;
        match c.generate(&s.model, &request) {
            Ok(r) => return Ok(r),
            Err(e) if attempt < DIRECT_ATTEMPTS && !account_refused(&e) && (e.contains(" 429 ") || e.contains(" 500 ") || e.contains(" 502 ") || e.contains(" 503 ") || e.contains(" 504 ")) => {
                pause(5 * 3u64.pow(attempt - 1))?;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Фраза Gemini TTS (24 кГц): из кэша задачи; фраза, которую задача не получила, — ошибка с причиной; фраза вне
/// задачи — обычным запросом.
pub fn phrase(repo_root: &Path, models_root: &Path, project_dir: &Path, pid: &str, s: &Setup, w: &Want) -> Result<(Vec<f32>, Option<f64>), String> {
    let key = key_of(s, w);
    let path = cache(project_dir, &key);
    if path.is_file() {
        return crate::wavio::read_mono_f32(&path).map(|(x, _)| (x, None));
    }
    let failed: HashMap<String, String> = std::fs::read(failed_path(project_dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let Some(error) = failed.get(&key) {
        return Err(t!("google-phrase-failed", error = explain(error)));
    }
    crate::spending::check_limit(repo_root, models_root, None)?;
    let response = direct(&client()?, s, voice_of(s, w), &w.text, &w.style).map_err(|e| explain(&e))?;
    let samples = keep(project_dir, &key, &response)?;
    let cost = spend(repo_root, pid, s, &response["usageMetadata"], false);
    Ok((samples, cost))
}

/// Звук всех фраз задачи заранее, в кэш: обычными запросами в несколько потоков или пачкой, по режиму. Фразы, которые
/// не получились, записываются с причиной; отчёт задачи — в `cloud/google-report.json`.
pub fn prefetch(repo_root: &Path, models_root: &Path, project_dir: &Path, pid: &str, s: &Setup, wants: &[Want], progress: &dyn Fn(Value)) -> Result<Value, String> {
    let mut keys: Vec<(String, &Want)> = Vec::new();
    for w in wants {
        let key = key_of(s, w);
        if !cache(project_dir, &key).is_file() && !keys.iter().any(|(k, _)| *k == key) {
            keys.push((key, w));
        }
    }
    let pending = journal_path(project_dir).is_file();
    if keys.is_empty() && !pending {
        return Ok(json!({ "phrases": wants.len(), "cached": wants.len() }));
    }
    let batch = pending || s.mode == Mode::Batch || (s.mode == Mode::Auto && keys.len() > AUTO_DIRECT_MAX);
    let started = Instant::now();
    let c = client()?;
    let (failed, cost, batch_name) = if batch { batch_run(&c, repo_root, models_root, project_dir, pid, s, &keys, progress)? } else { direct_run(&c, repo_root, models_root, project_dir, pid, s, &keys, progress)? };
    let path = failed_path(project_dir);
    let mut all: HashMap<String, String> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    for (key, _) in &keys {
        all.remove(key);
    }
    all.extend(failed.iter().map(|(k, e)| (k.clone(), e.clone())));
    std::fs::create_dir_all(cloud_dir(project_dir)).map_err(|e| e.to_string())?;
    save(&path, &json!(all))?;
    let report = json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "mode": if batch { "batch" } else { "standard" },
        "setting": format!("{:?}", s.mode).to_lowercase(),
        "model": s.model,
        "voice": s.voice,
        "phrases": wants.len(),
        "requested": keys.len(),
        "cached": wants.len() - keys.len(),
        "ok": keys.len() - failed.len(),
        "failed": failed.iter().map(|(k, e)| json!({ "key": k, "text": keys.iter().find(|(kk, _)| kk == k).map(|(_, w)| w.text.clone()), "error": e })).collect::<Vec<_>>(),
        "cost_estimate_usd": cost,
        "seconds": started.elapsed().as_secs_f64(),
        "batch": batch_name,
    });
    save(&report_path(project_dir), &report)?;
    Ok(report)
}

/// Обычные запросы в `or_concurrency` потоков.
#[allow(clippy::too_many_arguments)]
fn direct_run(c: &crate::google_tts::Client, repo_root: &Path, models_root: &Path, project_dir: &Path, pid: &str, s: &Setup, keys: &[(String, &Want)], progress: &dyn Fn(Value)) -> Result<(HashMap<String, String>, Option<f64>, Option<String>), String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    crate::spending::check_limit(repo_root, models_root, None)?;
    let threads = crate::models::openrouter_concurrency(models_root).min(keys.len()).max(1);
    let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let failed: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
    // первый отказ по деньгам или доступу останавливает остальные запросы: они получат ту же причину
    let refused: Mutex<Option<String>> = Mutex::new(None);
    let cost: Mutex<Option<f64>> = Mutex::new(None);
    let ctl = crate::jobs::current();
    let total = keys.len();
    progress(json!({ "type": "progress", "stage": STAGE, "msg": t!("google-direct-sending", n = total as i64, threads = threads as i64) }));
    let (tell, told) = std::sync::mpsc::channel::<usize>();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let (next, done, failed, cost, ctl, tell, refused) = (&next, &done, &failed, &cost, ctl.clone(), tell.clone(), &refused);
            scope.spawn(move || {
                let _entered = ctl.map(crate::jobs::enter);
                loop {
                    if cancelled() {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some((key, w)) = keys.get(i) else { break };
                    let stopped = refused.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    let got = match stopped {
                        Some(reason) => Err(reason),
                        None => direct(c, s, voice_of(s, w), &w.text, &w.style).and_then(|r| keep(project_dir, key, &r).map(|_| r)),
                    };
                    match got {
                        Ok(r) => {
                            if let Some(usd) = spend(repo_root, pid, s, &r["usageMetadata"], false) {
                                let mut sum = cost.lock().unwrap_or_else(|p| p.into_inner());
                                *sum = Some(sum.unwrap_or(0.0) + usd);
                            }
                        }
                        Err(e) => {
                            if account_refused(&e) {
                                refused.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(|| e.clone());
                            }
                            failed.lock().unwrap_or_else(|p| p.into_inner()).insert(key.clone(), e);
                        }
                    }
                    if tell.send(done.fetch_add(1, Ordering::SeqCst) + 1).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tell);
        for k in told {
            progress(json!({ "type": "progress", "stage": STAGE, "msg": t!("google-direct-progress", done = k as i64, total = total as i64), "pct": (k * 100 / total) as i64 }));
        }
    });
    if cancelled() {
        return Err(crate::jobs::CANCELLED.to_string());
    }
    Ok((failed.into_inner().unwrap_or_default(), cost.into_inner().unwrap_or_default(), None))
}

/// Пачка: журнал до отправки, поиск по имени после потерянного ответа, ожидание с повторами чтения, результаты в кэш.
#[allow(clippy::too_many_arguments)]
fn batch_run(c: &crate::google_tts::Client, repo_root: &Path, models_root: &Path, project_dir: &Path, pid: &str, s: &Setup, keys: &[(String, &Want)], progress: &dyn Fn(Value)) -> Result<(HashMap<String, String>, Option<f64>, Option<String>), String> {
    let path = journal_path(project_dir);
    std::fs::create_dir_all(cloud_dir(project_dir)).map_err(|e| e.to_string())?;
    let mut journal: Value = match std::fs::read(&path) {
        Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({
            "state": "new",
            "model": s.model,
            "voice": s.voice,
            "display_name": format!("dub-studio-{pid}-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            "keys": keys.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
            "texts": keys.iter().map(|(_, w)| w.text.clone()).collect::<Vec<_>>(),
            "created_at": chrono::Utc::now().to_rfc3339(),
        }),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let display = journal["display_name"].as_str().unwrap_or_default().to_string();
    if journal["name"].is_null() && journal.get("submitted").is_some() {
        // отправка ушла, а ответ потерялся: пачка ищется в Google по своему имени
        match retried("list", progress, || c.batch_find(&display))?.as_slice() {
            [one] => {
                journal["name"] = one.clone().into();
                save(&path, &journal)?;
            }
            [] if journal["submitted_at"].as_i64().is_some_and(|t| chrono::Utc::now().timestamp() - t >= UNLISTED_AFTER_SECS) => {
                if let Some(o) = journal.as_object_mut() {
                    o.remove("submitted");
                }
                save(&path, &journal)?;
            }
            [] => return Err(t!("google-batch-unknown", name = display.clone(), journal = path.display().to_string())),
            many => return Err(t!("google-batch-many", name = display.clone(), n = many.len() as i64, journal = path.display().to_string())),
        }
    }
    if journal["name"].is_null() {
        // ключи задачи берутся из журнала: пачка та же, что начали отправлять
        let wanted: Vec<String> = journal["keys"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
        let rows: Vec<Value> = keys
            .iter()
            .filter(|(k, _)| wanted.contains(k))
            .map(|(k, w)| -> Result<Value, String> { Ok(json!({ "metadata": { "key": k }, "request": crate::google_tts::request(&w.text, voice_of(s, w), &w.style)? })) })
            .collect::<Result<_, _>>()?;
        if rows.is_empty() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok((HashMap::new(), None, None));
        }
        crate::spending::check_limit(repo_root, models_root, None)?;
        if journal["input_file"].is_null() {
            progress(json!({ "type": "progress", "stage": STAGE, "msg": t!("google-batch-sending", n = rows.len() as i64) }));
            journal["input_file"] = c.upload_requests(&Value::Array(rows))?.into();
            save(&path, &journal)?;
        }
        if cancelled() {
            return Err(crate::jobs::CANCELLED.to_string());
        }
        journal["submitted"] = true.into();
        journal["submitted_at"] = chrono::Utc::now().timestamp().into();
        save(&path, &journal)?;
        match c.batch_create(&s.model, &display, journal["input_file"].as_str().unwrap_or_default())? {
            crate::google_tts::Created::Name(name) => {
                journal["name"] = name.into();
                journal["state"] = "BATCH_STATE_PENDING".into();
                save(&path, &journal)?;
            }
            crate::google_tts::Created::Rejected(code, e) => {
                if let Some(o) = journal.as_object_mut() {
                    o.remove("submitted");
                    if code != 429 {
                        // входной файл живёт в Google 48 ч: после отказа он заливается заново
                        o.remove("input_file");
                    }
                }
                journal["rejected"] = e.clone().into();
                save(&path, &journal)?;
                return Err(if code == 429 { t!("google-batch-queue-full", error = explain(&e)) } else { t!("google-batch-rejected", error = explain(&e)) });
            }
        }
    }
    let name = journal["name"].as_str().ok_or("the batch journal has no name")?.to_string();
    let op = loop {
        if cancelled() {
            return Err(crate::jobs::CANCELLED.to_string());
        }
        let op = retried("status", progress, || c.batch_get(&name))?;
        let stats = op.pointer("/metadata/batchStats").cloned().unwrap_or(Value::Null);
        journal["state"] = op.pointer("/metadata/state").cloned().unwrap_or(Value::Null);
        journal["stats"] = stats.clone();
        journal["google_create_time"] = op.pointer("/metadata/createTime").cloned().unwrap_or(Value::Null);
        journal["google_end_time"] = op.pointer("/metadata/endTime").cloned().unwrap_or(Value::Null);
        save(&path, &journal)?;
        if op["done"] == true {
            break op;
        }
        let count = |k: &str| stats.get(k).and_then(|v| v.as_str().and_then(|t| t.parse::<i64>().ok()).or_else(|| v.as_i64())).unwrap_or(0);
        progress(json!({ "type": "progress", "stage": STAGE, "msg": t!("google-batch-waiting", state = journal["state"].as_str().unwrap_or("BATCH_STATE_PENDING").to_string(), done = count("successfulRequestCount") + count("failedRequestCount"), total = count("requestCount")) }));
        pause(BATCH_POLL_SECS)?;
    };
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    std::fs::create_dir_all(history_dir(project_dir)).map_err(|e| e.to_string())?;
    let rows = op.pointer("/response/inlinedResponses/inlinedResponses").and_then(Value::as_array).cloned();
    let Some(rows) = rows else {
        // истекла (48 ч в очереди), отменена или упала без результатов: журнал уходит в историю, следующая задача
        // отправит те же фразы новой пачкой
        let state = journal["state"].as_str().unwrap_or("unknown").to_lowercase();
        journal["error"] = op.get("error").cloned().unwrap_or(Value::Null);
        save(&history_dir(project_dir).join(format!("{stamp}-{state}.json")), &journal)?;
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        return Err(t!("google-batch-dead", name = name.clone(), state = state, error = op.get("error").map(Value::to_string).unwrap_or_default()));
    };
    let wanted: Vec<String> = journal["keys"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let mut failed: HashMap<String, String> = HashMap::new();
    let mut cost: Option<f64> = None;
    let mut seen: Vec<String> = Vec::new();
    for row in &rows {
        let Some(key) = row.pointer("/metadata/key").and_then(Value::as_str) else { continue };
        seen.push(key.to_string());
        let response = row.get("response").cloned().unwrap_or(Value::Null);
        match keep(project_dir, key, &response) {
            Ok(_) => {
                if let Some(usd) = spend(repo_root, pid, s, &response["usageMetadata"], true) {
                    cost = Some(cost.unwrap_or(0.0) + usd);
                }
            }
            Err(e) => {
                failed.insert(key.to_string(), row.get("error").filter(|e| !e.is_null()).map(Value::to_string).unwrap_or(e));
            }
        }
    }
    for key in wanted.iter().filter(|k| !seen.contains(k)) {
        failed.insert(key.clone(), t!("google-batch-no-row"));
    }
    journal["state"] = "downloaded".into();
    journal["ok"] = (seen.len() - failed.len().min(seen.len())).into();
    journal["failed"] = failed.len().into();
    journal["cost_estimate_usd"] = json!(cost);
    save(&history_dir(project_dir).join(format!("{stamp}-done.json")), &journal)?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((failed, cost, Some(name)))
}

/// Отчёт проекта: идущая пачка, последние задачи Gemini и история пачек.
pub fn report(project_dir: &Path) -> Value {
    let read = |p: &Path| std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let mut batches: Vec<Value> = std::fs::read_dir(history_dir(project_dir)).map(|rd| rd.flatten().filter_map(|e| read(&e.path())).collect()).unwrap_or_default();
    batches.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
    json!({ "pending": read(&journal_path(project_dir)), "last": read(&report_path(project_dir)), "batches": batches })
}

/// GET /projects/{pid}/google-tts — the project's Google report: the batch under way, the last job, the batches done.
pub async fn project_report(
    axum::extract::State(st): axum::extract::State<crate::AppState>,
    axum::extract::Path(pid): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match st.proj_dir(&pid) {
        Ok(wd) => axum::Json(report(&wd)).into_response(),
        Err(r) => r,
    }
}

/// A render's phrases voiced by Google: the planning above with every phrase at once, then each phrase's sound into
/// its line file as the timeline reads it (PCM16 24 kHz mono). A phrase Google did not voice names why.
pub fn synth_jobs(repo_root: &Path, models_root: &Path, wd: &Path, jobs: &[crate::cloud_tts::Job], progress: &crate::render::Progress) -> Result<crate::cloud_tts::Synthesized, String> {
    let s = setup(models_root, "", "")?;
    let pid = wd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let wants: Vec<Want> = jobs
        .iter()
        .map(|j| Want { text: j.text.clone(), style: j.style.clone(), n: 1, voice: j.voice.clone(), tag: j.key.clone() })
        .collect();
    prefetch(repo_root, models_root, wd, &pid, &s, &wants, &|ev| progress(ev))?;
    let mut done = crate::cloud_tts::Synthesized { ok: vec![false; jobs.len()], failures: Vec::new() };
    for (i, (job, want)) in jobs.iter().zip(&wants).enumerate() {
        crate::jobs::check_cancelled()?;
        let line = phrase(repo_root, models_root, wd, &pid, &s, want).and_then(|(samples, _)| {
            let pcm: Vec<u8> = samples.iter().flat_map(|x| ((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
            let wav = dub_llm::openrouter::pcm16_to_wav(&pcm, SR, 1).map_err(|e| e.to_string())?;
            dub_core::atomic::write(&job.out, &wav).map_err(|e| e.to_string())
        });
        match line {
            Ok(()) => done.ok[i] = true,
            Err(e) => done.failures.push(format!("{}: {e}", job.key)),
        }
    }
    Ok(done)
}

fn save(path: &Path, value: &Value) -> Result<(), String> {
    dub_core::atomic::write(path, serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?.as_slice()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_with(mode: Mode) -> Setup {
        Setup { model: "m".into(), voice: "Kore".into(), mode }
    }

    fn want(text: &str, style: &str, n: u32) -> Want {
        Want { text: text.into(), style: style.into(), n, voice: String::new(), tag: String::new() }
    }

    #[test]
    fn setup_needs_a_model_and_a_voice_and_prefers_the_project_model() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(setup(root.path(), "", "Kore").unwrap().model, crate::google_tts::DEFAULT_MODEL);
        let default_voice = crate::cloud_voices::default_voice("gemini-3.8-flash-lite-tts").unwrap();
        assert_eq!(setup(root.path(), "gemini-3.8-flash-lite-tts", " ").unwrap().voice, default_voice);
        let s = setup(root.path(), "gemini-3.8-flash-lite-tts", " Kore ").unwrap();
        assert_eq!((s.model.as_str(), s.voice.as_str(), s.mode), ("gemini-3.8-flash-lite-tts", "Kore", Mode::Standard));
    }

    #[test]
    fn cache_key_changes_with_voice_text_style_take_and_tag() {
        let s = setup_with(Mode::Standard);
        let a = key_of(&s, &want("Привет", "", 1));
        assert_eq!(a, key_of(&s, &want("Привет", "", 1)));
        assert_ne!(a, key_of(&Setup { voice: "Puck".into(), ..setup_with(Mode::Standard) }, &want("Привет", "", 1)));
        assert_ne!(a, key_of(&s, &Want { voice: "Puck".into(), ..want("Привет", "", 1) }), "a speaker's own voice");
        assert_eq!(a, key_of(&s, &Want { voice: "Kore".into(), ..want("Привет", "", 1) }), "the project's voice named outright");
        assert_ne!(a, key_of(&s, &want("Привет!", "", 1)));
        assert_ne!(a, key_of(&s, &want("Привет", "шёпотом", 1)));
        assert_ne!(a, key_of(&s, &want("Привет", "", 2)), "a new take is a new request");
        assert_ne!(a, key_of(&s, &Want { tag: "s3:abc".into(), ..want("Привет", "", 1) }), "a dub line's own take");
    }

    #[test]
    fn money_and_access_refusals_stop_the_job_and_are_not_retried() {
        let _language = crate::i18n::test_language("en");
        assert!(account_refused("Google 402 Payment Required: prepayment credits are depleted"));
        assert!(account_refused("Google 429 Too Many Requests: monthly spending cap reached"));
        assert!(account_refused("Google 403 Forbidden: permission denied"));
        assert!(!account_refused("Google 429 Too Many Requests: rate limit"));
        assert!(!account_refused("Google TTS unfinished: \"PROHIBITED_CONTENT\""));
        assert!(explain("Google 402 Payment Required: {\"error\":{\"message\":\"Your prepayment credits are depleted\"}}").ends_with("(Google 402)"));
        assert!(!explain("Google 402 Payment Required: {\"error\":{\"message\":\"Your prepayment credits are depleted\"}}").contains("depleted"));
        assert_ne!(explain("Google 429 Too Many Requests: monthly spending cap reached"), explain("Google 429 Too Many Requests: rate limit"));
        assert!(!explain("Google TTS unfinished: \"PROHIBITED_CONTENT\"").contains("PROHIBITED"));
        assert_eq!(explain("Google: connection reset"), "Google: connection reset");
    }

    #[test]
    fn pcm_from_google_reads_as_samples() {
        let bytes: Vec<u8> = [0i16, 16384, -32768].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(samples_of("audio/L16;codec=pcm;rate=24000", &bytes).unwrap(), vec![0.0, 0.5, -1.0]);
    }

    #[test]
    fn a_phrase_the_job_did_not_get_fails_with_its_reason_and_costs_nothing() {
        let _language = crate::i18n::test_language("en");
        let dir = tempfile::tempdir().unwrap();
        let s = setup_with(Mode::Batch);
        let w = want("Привет", "", 1);
        let key = key_of(&s, &w);
        std::fs::create_dir_all(cloud_dir(dir.path())).unwrap();
        save(&failed_path(dir.path()), &json!({ key.clone(): "PROHIBITED_CONTENT" })).unwrap();
        let root = tempfile::tempdir().unwrap();
        let error = phrase(root.path(), root.path(), dir.path(), "p", &s, &w).unwrap_err();
        assert!(error.contains(&explain("PROHIBITED_CONTENT")) && !error.contains("PROHIBITED_CONTENT"), "{error}");
    }

    #[test]
    fn a_cached_phrase_is_sound_already_paid_for() {
        let dir = tempfile::tempdir().unwrap();
        let s = setup_with(Mode::Auto);
        let w = want("Привет", "", 1);
        let path = cache(dir.path(), &key_of(&s, &w));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::wavio::write_mono_f32(&path, &[0.0, 0.25], SR).unwrap();
        let root = tempfile::tempdir().unwrap();
        let (samples, cost) = phrase(root.path(), root.path(), dir.path(), "p", &s, &w).unwrap();
        assert_eq!((samples.len(), cost), (2, None));
        let report = prefetch(root.path(), root.path(), dir.path(), "p", &s, &[w], &|_| {}).unwrap();
        assert_eq!((report["phrases"].as_u64(), report["cached"].as_u64()), (Some(1), Some(1)), "nothing is asked of Google");
    }

    #[test]
    fn the_report_lists_the_pending_batch_the_last_job_and_the_history() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(history_dir(dir.path())).unwrap();
        save(&journal_path(dir.path()), &json!({ "name": "batches/a", "state": "BATCH_STATE_RUNNING" })).unwrap();
        save(&report_path(dir.path()), &json!({ "mode": "batch", "ok": 3 })).unwrap();
        save(&history_dir(dir.path()).join("1-done.json"), &json!({ "created_at": "2026-10-15", "state": "downloaded" })).unwrap();
        let r = report(dir.path());
        assert_eq!(r["pending"]["name"], "batches/a");
        assert_eq!(r["last"]["ok"], 3);
        assert_eq!(r["batches"][0]["state"], "downloaded");
    }
}
