//! Gemini Developer API TTS. Batch journals survive renderer cancellation and restarts.

use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct Client {
    http: reqwest::blocking::Client,
    sdk: rust_genai::Client,
    runtime: tokio::runtime::Runtime,
    base: String,
    key: String,
}

impl Client {
    pub fn new(key: String) -> Result<Self, String> {
        Self::with_base(key, BASE)
    }

    fn with_base(key: String, base: &str) -> Result<Self, String> {
        let mut sdk = rust_genai::Client::builder()
            .api_key(key.clone())
            .base_url(base.trim_end_matches("/v1beta"))
            .api_version("v1beta")
            .timeout(180)
            .retry_options(rust_genai::types::http::HttpRetryOptions {
                attempts: Some(1),
                ..Default::default()
            });
        if let Some(proxy) = dub_llm::net::proxy_url_for(BASE) {
            sdk = sdk.proxy(proxy.as_str());
        }
        Ok(Self {
            http: dub_llm::net::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(180))
                .build()
                .map_err(|e| e.to_string())?,
            sdk: sdk
                .build()
                .map_err(|e| e.to_string().replace(&key, "[redacted]"))?,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?,
            base: format!("{}/v1beta", base.trim_end_matches("/v1beta")),
            key,
        })
    }

    fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header("x-goog-api-key", &self.key);
        if let Some(body) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let res = req
            .send()
            .map_err(|e| format!("Google: {}", dub_llm::net::why(&e)))?;
        let status = res.status();
        let text = res
            .text()
            .map_err(|e| format!("Google response: {}", dub_llm::net::why(&e)))?;
        if !status.is_success() {
            let safe = text.replace(&self.key, "[redacted]");
            return Err(format!(
                "Google {status}: {}",
                safe.chars().take(1500).collect::<String>()
            ));
        }
        serde_json::from_str(&text).map_err(|e| format!("invalid Google JSON: {e}"))
    }

    pub fn models(&self) -> Result<Value, String> {
        let rows = self
            .runtime
            .block_on(self.sdk.models().all())
            .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
        let mut models = Vec::new();
        for model in rows
            .into_iter()
            .filter(|m| m.name.as_deref().is_some_and(|n| n.contains("tts")))
        {
            let name = model.name.ok_or("Google model has no name")?;
            let id = name
                .strip_prefix("models/")
                .ok_or("invalid Google model name")?;
            validate_model(id)?;
            // The SDK omits Developer API supportedGenerationMethods from its typed Model.
            let methods = match model.supported_actions {
                Some(actions) => json!(actions),
                None => self.call(reqwest::Method::GET, &format!("/{name}"), None)?
                    ["supportedGenerationMethods"]
                    .clone(),
            };
            models.push(json!({"name":name,"displayName":model.display_name,"supportedGenerationMethods":methods}));
        }
        Ok(json!({"models":models}))
    }

    pub fn generate(&self, model: &str, request: &Value) -> Result<Value, String> {
        validate_model(model)?;
        self.call(
            reqwest::Method::POST,
            &format!("/models/{model}:generateContent"),
            Some(request),
        )
    }

    fn batch_get(&self, name: &str) -> Result<Value, String> {
        validate_batch(name)?;
        // The SDK's typed BatchJob only recognizes JOB_STATE_*; Operation preserves Google's BATCH_STATE_* and raw usage.
        let operation = self
            .runtime
            .block_on(self.sdk.operations().get(name))
            .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
        let mut op = serde_json::to_value(operation).map_err(|e| e.to_string())?;
        if op["done"] == true {
            if let Some(file) = op
                .pointer("/metadata/output/responsesFile")
                .or_else(|| op.pointer("/response/responsesFile"))
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                let bytes = self
                    .runtime
                    .block_on(self.sdk.files().download(&file))
                    .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
                let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                let mut rows = Vec::new();
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    let mut row: Value = serde_json::from_str(line)
                        .map_err(|e| format!("invalid Google Batch JSONL: {e}"))?;
                    row["metadata"] = json!({"key":row["key"]});
                    rows.push(row);
                }
                op["response"] = json!({"inlinedResponses":{"inlinedResponses":rows}});
            }
        }
        Ok(op)
    }

    fn upload_requests(&self, requests: &Value) -> Result<String, String> {
        let mut jsonl = String::new();
        for row in requests
            .as_array()
            .ok_or("Batch requests must be an array")?
        {
            jsonl.push_str(
                &json!({"key":row["metadata"]["key"],"request":row["request"]}).to_string(),
            );
            jsonl.push('\n');
        }
        let file = self
            .runtime
            .block_on(
                self.sdk
                    .files()
                    .upload(jsonl.into_bytes(), "application/jsonl"),
            )
            .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
        file.name.ok_or("Google uploaded JSONL has no name".into())
    }

    fn batch_create(&self, model: &str, display: &str, file: &str) -> Result<String, String> {
        let src = rust_genai::types::batches::BatchJobSource {
            file_name: Some(file.into()),
            ..Default::default()
        };
        let config = rust_genai::types::batches::CreateBatchJobConfig {
            display_name: Some(display.into()),
            ..Default::default()
        };
        self.runtime
            .block_on(self.sdk.batches().create(model, src, config))
            .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?
            .name
            .ok_or("Google Batch has no name".into())
    }
}

fn validate_model(model: &str) -> Result<(), String> {
    if crate::models::is_selection_value("google_tts_model", model) {
        Ok(())
    } else {
        Err("invalid Gemini model id; use the id without models/".into())
    }
}

fn validate_batch(name: &str) -> Result<(), String> {
    let id = name.strip_prefix("batches/").unwrap_or_default();
    if !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        Ok(())
    } else {
        Err("invalid Google batch name".into())
    }
}

pub fn request(text: &str, voice: &str, style: &str) -> Result<Value, String> {
    if text.trim().is_empty() || voice.trim().is_empty() {
        return Err("Google TTS needs dialogue and a voice".into());
    }
    let mut part = json!({"text":text});
    if !style.trim().is_empty() {
        part["speech_metadata"] = json!({"style":style});
    }
    Ok(json!({
        "contents":[{"role":"user","parts":[part]}],
        "generationConfig":{"responseModalities":["AUDIO"],"maxOutputTokens":8192,
            "speechConfig":{"voiceConfig":{"prebuiltVoiceConfig":{"voiceName":voice}}}}
    }))
}

pub fn audio(response: &Value) -> Result<(String, Vec<u8>), String> {
    let candidate = response["candidates"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| {
            format!(
                "Google returned no candidate (block reason: {})",
                response["promptFeedback"]["blockReason"]
            )
        })?;
    if candidate["finishReason"]
        .as_str()
        .is_some_and(|r| r != "STOP")
    {
        return Err(format!(
            "Google TTS unfinished: {}",
            candidate["finishReason"]
        ));
    }
    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or("Google returned no audio parts")?;
    let audio: Vec<_> = parts.iter().filter_map(|p| p.get("inlineData")).collect();
    if audio.len() != 1 {
        return Err(format!(
            "Google returned {} audio parts; expected one complete clip",
            audio.len()
        ));
    }
    let mime = audio[0]["mimeType"]
        .as_str()
        .ok_or("Google audio has no MIME type")?;
    if !mime.starts_with("audio/") {
        return Err("Google inline data is not audio".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            audio[0]["data"]
                .as_str()
                .ok_or("Google audio has no data")?,
        )
        .map_err(|e| format!("invalid Google audio: {e}"))?;
    if bytes.len() < 200 {
        return Err("Google audio is empty or too short".into());
    }
    Ok((mime.into(), bytes))
}

pub fn estimate(model: &str, batch: bool, usage: &Value) -> Value {
    // Published promotional tariff ends 2026-12-31; unknown models have no invented price.
    if model != "gemini-3.8-flash-lite-tts" {
        return Value::Null;
    }
    let factor = if batch { 0.5 } else { 1.0 }
        * if chrono::Utc::now().date_naive()
            > chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()
        {
            2.0
        } else {
            1.0
        };
    let Some(input) = usage["promptTokenCount"].as_u64() else {
        return Value::Null;
    };
    let Some(details) = usage["candidatesTokensDetails"].as_array() else {
        return Value::Null;
    };
    if !details
        .iter()
        .any(|d| d["modality"] == "AUDIO" && d["tokenCount"].as_u64().is_some())
    {
        return Value::Null;
    }
    let output: u64 = details
        .iter()
        .filter(|d| d["modality"] == "AUDIO")
        .filter_map(|d| d["tokenCount"].as_u64())
        .sum();
    json!({"usd":factor * (input as f64 * 0.5 + output as f64 * 6.0) / 1_000_000.0,
        "basis":"usageMetadata × published tariff; not a billing invoice", "input_tokens":input, "audio_tokens":output,
        "input_usd_per_million":0.5*factor,"audio_usd_per_million":6.0*factor,
        "pricing_url":"https://ai.google.dev/gemini-api/docs/pricing#gemini-3.8-flash-lite-tts"})
}

fn save(path: &Path, value: &Value) -> Result<(), String> {
    dub_core::atomic::write(
        path,
        serde_json::to_vec_pretty(value)
            .map_err(|e| e.to_string())?
            .as_slice(),
    )
    .map_err(|e| e.to_string())
}

fn batch_rows(op: &Value) -> Result<HashMap<String, &Value>, String> {
    if !op["error"].is_null() {
        return Err(format!("Google batch failed: {}", op["error"]));
    }
    let rows = op
        .pointer("/response/inlinedResponses/inlinedResponses")
        .or_else(|| op.pointer("/metadata/output/inlinedResponses/inlinedResponses"))
        .and_then(Value::as_array)
        .ok_or("Google batch has no inline results")?;
    let mut out = HashMap::new();
    for row in rows {
        let key = row["metadata"]["key"]
            .as_str()
            .ok_or("Google batch result has no request key")?;
        if out.insert(key.to_string(), row).is_some() {
            return Err(format!("duplicate Google batch result: {key}"));
        }
    }
    Ok(out)
}

fn await_batch(
    client: &Client,
    dir: &Path,
    model: &str,
    requests: &Value,
    progress: &crate::render::Progress,
) -> Result<Value, String> {
    let path = dir.join("batch.json");
    let mut journal = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| format!("invalid Google batch journal: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let j = json!({"state":"submitting","display_name":format!("DubStudio-{}",uuid::Uuid::new_v4()),"started_at":chrono::Utc::now().to_rfc3339()});
            save(&path, &j)?;
            j
        }
        Err(e) => return Err(e.to_string()),
    };
    if journal["name"].is_null() {
        // A marker written before POST prevents a restart from submitting the same paid batch twice.
        let started = journal.get("submitted").is_some();
        if started {
            return Err(format!("Google batch submission outcome is unknown. Find displayName {} in Google before retrying; journal {}", journal["display_name"], path.display()));
        }
        if journal["input_file"].is_null() {
            journal["input_file"] = client.upload_requests(requests)?.into();
            save(&path, &journal)?;
        }
        crate::jobs::check_cancelled()?;
        journal["submitted"] = true.into();
        save(&path, &journal)?;
        let name = client.batch_create(
            model,
            journal["display_name"]
                .as_str()
                .ok_or("Batch has no display name")?,
            journal["input_file"]
                .as_str()
                .ok_or("Batch has no input file")?,
        )?;
        validate_batch(&name)?;
        journal["name"] = name.into();
        journal["state"] = "BATCH_STATE_PENDING".into();
        save(&path, &journal)?;
    }
    let name = journal["name"]
        .as_str()
        .ok_or("batch journal has no name")?
        .to_string();
    loop {
        crate::jobs::check_cancelled()?;
        let op = client.batch_get(&name)?;
        journal["state"] = op["metadata"]["state"].clone();
        journal["stats"] = op["metadata"]["batchStats"].clone();
        journal["google_create_time"] = op["metadata"]["createTime"].clone();
        journal["google_end_time"] = op["metadata"]["endTime"].clone();
        save(&path, &journal)?;
        progress(
            json!({"type":"progress","stage":"tts","msg":format!("Google Batch {name}: {}",op["metadata"]["state"]),"batch_name":name,"batch_stats":op["metadata"]["batchStats"]}),
        );
        if op["done"] == true {
            return Ok(op);
        }
        for _ in 0..10 {
            crate::jobs::check_cancelled()?;
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

pub fn synth_jobs(
    models_root: &Path,
    wd: &Path,
    jobs: &[crate::cloud_tts::Job],
    concurrency: usize,
    progress: &crate::render::Progress,
) -> Result<Vec<bool>, String> {
    let model = crate::models::tts_model(models_root);
    validate_model(&model)?;
    let batch = crate::models::google_tts_batch(models_root);
    let client = Client::new(
        crate::credentials::google_api_key()
            .ok_or("Google TTS selected but GEMINI_API_KEY is not configured")?
            .0,
    )?;
    let reqs: Vec<Value> = jobs
        .iter()
        .map(|j| {
            request(&j.text, &j.voice, &j.style)
                .map(|r| json!({"request":r,"metadata":{"key":j.key}}))
        })
        .collect::<Result<_, _>>()?;
    let requests = json!(reqs);
    let hash = blake3::hash(
        json!({"model":model,"batch":batch,"requests":requests})
            .to_string()
            .as_bytes(),
    )
    .to_hex()
    .to_string();
    let dir = wd.join("google-tts").join(&hash);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    save(&dir.join("requests.json"), &requests)?;
    let wall = Instant::now();
    let mut report = json!({"provider":"google","model":model,"mode":if batch {"batch"} else {"standard"},"request_count":jobs.len(),"started_at":chrono::Utc::now().to_rfc3339(),"lines":[],"concurrency":if batch {1} else {concurrency}});
    save(&wd.join("google-tts-latest.json"), &report)?;
    let results: Vec<Result<Value, String>> = if batch {
        let op = await_batch(&client, &dir, &model, &requests, progress)?;
        let rows = batch_rows(&op)?;
        jobs.iter()
            .map(|j| match rows.get(&j.key) {
                Some(row) if !row["response"].is_null() => Ok(row["response"].clone()),
                Some(row) => Err(format!("{}: {}", j.key, row["error"])),
                None => Err(format!("missing Google batch result: {}", j.key)),
            })
            .collect()
    } else {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        };
        let next = AtomicUsize::new(0);
        let results: Mutex<Vec<Result<Value, String>>> = Mutex::new(
            (0..jobs.len())
                .map(|_| Err("not generated".into()))
                .collect(),
        );
        let ctl = crate::jobs::current();
        std::thread::scope(|scope| {
            for _ in 0..concurrency.max(1).min(jobs.len()) {
                let (next, results, client, dir, reqs, ctl, model) =
                    (&next, &results, &client, &dir, &reqs, ctl.clone(), &model);
                scope.spawn(move || {
                    let _entered = ctl.map(crate::jobs::enter);
                    loop {
                        if crate::jobs::check_cancelled().is_err() {
                            break;
                        }
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= jobs.len() {
                            break;
                        }
                        let cached = dir.join(format!("response-{i}.json"));
                        let response = match std::fs::read(&cached) {
                            Ok(b) => serde_json::from_slice(&b).map_err(|e| e.to_string()),
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                let started = Instant::now();
                                client
                                    .generate(model, &reqs[i]["request"])
                                    .and_then(|mut r| {
                                        r["_dub_request_seconds"] =
                                            started.elapsed().as_secs_f64().into();
                                        save(&cached, &r)?;
                                        Ok(r)
                                    })
                            }
                            Err(e) => Err(e.to_string()),
                        };
                        results.lock().expect("Google results")[i] = response;
                    }
                });
            }
        });
        results.into_inner().expect("Google results")
    };
    let mut failures = Vec::new();
    for (i, (job, result)) in jobs.iter().zip(results).enumerate() {
        let line = result.and_then(|response| {
            let (mime,bytes) = audio(&response)?;
            let wav = crate::cloud_tts::seg_audio(&mime,bytes)?;
            let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).map_err(|e| e.to_string())?;
            let duration = reader.duration() as f64 / reader.spec().sample_rate as f64;
            dub_core::atomic::write(&job.out,&wav).map_err(|e| e.to_string())?;
            dub_core::atomic::write(&dir.join(format!("line-{i}.wav")),&wav).map_err(|e| e.to_string())?;
            Ok(json!({"key":job.key,"voice":job.voice,"duration_seconds":duration,"request_seconds":response["_dub_request_seconds"],"usage":response["usageMetadata"],"cost_estimate":estimate(&model,batch,&response["usageMetadata"])}))
        });
        match line {
            Ok(line) => report["lines"].as_array_mut().unwrap().push(line),
            Err(error) => {
                failures.push(format!("{}: {error}", job.key));
                report["lines"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"key":job.key,"error":error}));
            }
        }
    }
    report["wall_seconds"] = wall.elapsed().as_secs_f64().into();
    report["finished_at"] = chrono::Utc::now().to_rfc3339().into();
    report["generated_audio_seconds"] = report["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["duration_seconds"].as_f64())
        .sum::<f64>()
        .into();
    report["known_cost_estimate_usd"] = report["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["cost_estimate"]["usd"].as_f64())
        .sum::<f64>()
        .into();
    report["unknown_cost_lines"] = report["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["cost_estimate"]["usd"].as_f64().is_none())
        .count()
        .into();
    if let Ok(bytes) = std::fs::read(dir.join("batch.json")) {
        report["batch"] = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if let (Some(start), Some(end)) = (
            report["batch"]["google_create_time"].as_str(),
            report["batch"]["google_end_time"].as_str(),
        ) {
            if let (Ok(start), Ok(end)) = (
                chrono::DateTime::parse_from_rfc3339(start),
                chrono::DateTime::parse_from_rfc3339(end),
            ) {
                report["google_batch_wall_seconds"] =
                    ((end - start).num_milliseconds() as f64 / 1000.0).into();
            }
        }
    }
    save(&dir.join("report.json"), &report)?;
    save(&wd.join("google-tts-latest.json"), &report)?;
    crate::jobs::check_cancelled()?;
    if !failures.is_empty() {
        return Err(format!(
            "Google TTS failed; completed clips remain cached, no standard/provider fallback: {}",
            failures.join("; ")
        ));
    }
    Ok(vec![true; jobs.len()])
}

pub async fn project_report(
    axum::extract::State(st): axum::extract::State<crate::AppState>,
    axum::extract::Path(pid): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let wd = match st.proj_dir(&pid) {
        Ok(wd) => wd,
        Err(r) => return r,
    };
    let latest = std::fs::read(wd.join("google-tts-latest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let mut batches = Vec::new();
    if let Ok(dirs) = std::fs::read_dir(wd.join("google-tts")) {
        for entry in dirs.flatten() {
            if let Ok(bytes) = std::fs::read(entry.path().join("batch.json")) {
                if let Ok(j) = serde_json::from_slice::<Value>(&bytes) {
                    batches.push(j);
                }
            }
        }
    }
    axum::Json(json!({"latest":latest,"batches":batches})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_llm::test_http::{serve, Reply};

    #[test]
    #[ignore = "Read-only smoke test requires GEMINI_API_KEY and DUB_GOOGLE_TEST_BATCH"]
    fn live_sdk_reads_an_existing_batch_and_model_catalog() {
        let client = Client::new(std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY")).unwrap();
        let catalog = client.models().unwrap();
        let lite = catalog["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == "models/gemini-3.8-flash-lite-tts")
            .unwrap();
        assert!(lite["supportedGenerationMethods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "batchGenerateContent"));
        let op = client
            .batch_get(&std::env::var("DUB_GOOGLE_TEST_BATCH").expect("existing batch name"))
            .unwrap();
        assert_eq!(op["done"], true);
        let rows = batch_rows(&op).unwrap();
        assert!(!rows.is_empty());
        for row in rows.values() {
            if row["response"].is_object() {
                audio(&row["response"]).unwrap();
            }
        }
    }

    #[test]
    fn catalog_preserves_developer_api_batch_capabilities() {
        let server = serve(vec![
            Reply::json(
                200,
                r#"{"models":[{"name":"models/gemini-3.8-flash-lite-tts","displayName":"Flash-Lite TTS"}]}"#,
            ),
            Reply::json(
                200,
                r#"{"supportedGenerationMethods":["generateContent","batchGenerateContent"]}"#,
            ),
        ]);
        let client = Client::with_base("private-key".into(), &server.base()).unwrap();
        let catalog = client.models().unwrap();
        assert!(catalog["models"][0]["supportedGenerationMethods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "batchGenerateContent"));
        assert!(server
            .request(1)
            .starts_with("GET /v1beta/models/gemini-3.8-flash-lite-tts"));
        assert_eq!(server.count(), 2);
    }

    #[test]
    fn styles_are_metadata_not_spoken_text_and_paths_cannot_change_the_host() {
        let req = request("Привет.", "Orus", "Quietly, from a distance").unwrap();
        assert_eq!(req["contents"][0]["parts"][0]["text"], "Привет.");
        assert_eq!(
            req["contents"][0]["parts"][0]["speech_metadata"]["style"],
            "Quietly, from a distance"
        );
        for m in [
            "",
            "models/x",
            "../other",
            "https://evil.invalid",
            "x?key=k",
        ] {
            assert!(validate_model(m).is_err());
        }
        assert!(validate_batch("batches/../other").is_err());
    }

    #[test]
    fn out_of_order_results_are_matched_by_key_and_errors_stay_errors() {
        let op = json!({"response":{"inlinedResponses":{"inlinedResponses":[
            {"metadata":{"key":"b"},"error":{"code":3}}, {"metadata":{"key":"a"},"response":{"usageMetadata":{}}}
        ]}}});
        let rows = batch_rows(&op).unwrap();
        assert_eq!(rows["b"]["error"]["code"], 3);
        assert!(rows["a"]["response"].is_object());
        assert!(batch_rows(&json!({"response":{"inlinedResponses":{"inlinedResponses":[{"metadata":{"key":"a"}},{"metadata":{"key":"a"}}]}}})).is_err());
        assert!(audio(&json!({"candidates":[{"finishReason":"MAX_TOKENS"}]})).is_err());
    }

    #[test]
    fn a_saved_batch_is_polled_without_another_paid_post() {
        let dir = tempfile::tempdir().unwrap();
        save(
            &dir.path().join("batch.json"),
            &json!({"name":"batches/existing","submitted":true}),
        )
        .unwrap();
        let server = serve(vec![Reply::json(
            200,
            r#"{"done":true,"metadata":{"state":"BATCH_STATE_SUCCEEDED"},"response":{"inlinedResponses":{"inlinedResponses":[]}}}"#,
        )]);
        let client = Client::with_base("private-key".into(), &server.base()).unwrap();
        await_batch(
            &client,
            dir.path(),
            "gemini-3.8-flash-lite-tts",
            &json!([]),
            &|_| {},
        )
        .unwrap();
        assert!(server
            .request(0)
            .starts_with("GET /v1beta/batches/existing"));
        assert_eq!(server.count(), 1);
        save(
            &dir.path().join("batch.json"),
            &json!({"submitted":true,"display_name":"unknown"}),
        )
        .unwrap();
        assert!(await_batch(
            &client,
            dir.path(),
            "gemini-3.8-flash-lite-tts",
            &json!([]),
            &|_| {}
        )
        .unwrap_err()
        .contains("outcome is unknown"));
        assert_eq!(server.count(), 1);
    }

    #[test]
    fn http_errors_redact_the_key_and_cost_uses_audio_usage() {
        let server = serve(vec![Reply::json(
            403,
            r#"{"error":"private-key rejected"}"#,
        )]);
        let client = Client::with_base("private-key".into(), &server.base()).unwrap();
        let err = client.models().unwrap_err();
        assert!(!err.contains("private-key"));
        assert!(err.contains("redacted"));
        let usage = json!({"promptTokenCount":8,"candidatesTokenCount":85,"candidatesTokensDetails":[{"modality":"AUDIO","tokenCount":85}]});
        let a = estimate("gemini-3.8-flash-lite-tts", false, &usage)["usd"]
            .as_f64()
            .unwrap();
        let b = estimate("gemini-3.8-flash-lite-tts", true, &usage)["usd"]
            .as_f64()
            .unwrap();
        assert_eq!(b, a / 2.0);
        assert!(estimate("unknown", true, &usage).is_null());
    }
}
