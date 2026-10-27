//! Gemini Developer API TTS transport. Callers own retries, cache and batch journals. The same code as Speech
//! Studio's `integrations/google_tts.rs`; the planning of a voicing is [`crate::google_speech`].

use base64::Engine;
use serde_json::{json, Value};
use std::time::Duration;

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

    pub(crate) fn with_base(key: String, base: &str) -> Result<Self, String> {
        let mut sdk = rust_genai::Client::builder()
            .api_key(key.clone())
            .base_url(base.trim_end_matches("/v1beta"))
            .api_version("v1beta")
            .timeout(180)
            .retry_options(rust_genai::types::http::HttpRetryOptions {
                attempts: Some(1),
                ..Default::default()
            });
        if let Some(proxy) = dub_llm::net::proxy_url_for(base) {
            sdk = sdk.proxy(proxy.as_str());
        }
        Ok(Self {
            http: dub_llm::net::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(180))
                .build()
                .map_err(|e| e.to_string())?,
            sdk: sdk.build().map_err(|e| e.to_string().replace(&key, "[redacted]"))?,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?,
            base: format!("{}/v1beta", base.trim_end_matches("/v1beta")),
            key,
        })
    }

    fn call(&self, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header("x-goog-api-key", &self.key);
        if let Some(body) = body {
            req = req.header("Content-Type", "application/json").body(body.to_string());
        }
        let res = req.send().map_err(|e| format!("Google: {}", dub_llm::net::why(&e)))?;
        let status = res.status();
        let text = res.text().map_err(|e| format!("Google response: {}", dub_llm::net::why(&e)))?;
        if !status.is_success() {
            let safe = text.replace(&self.key, "[redacted]");
            return Err(format!("Google {status}: {}", safe.chars().take(1500).collect::<String>()));
        }
        serde_json::from_str(&text).map_err(|e| format!("invalid Google JSON: {e}"))
    }

    pub fn models(&self) -> Result<Value, String> {
        let rows = self
            .runtime
            .block_on(self.sdk.models().all())
            .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
        let mut models = Vec::new();
        for model in rows.into_iter().filter(|m| m.name.as_deref().is_some_and(|n| n.contains("tts"))) {
            let name = model.name.ok_or("Google model has no name")?;
            let id = name.strip_prefix("models/").ok_or("invalid Google model name")?;
            validate_model(id)?;
            // The SDK omits Developer API supportedGenerationMethods from its typed Model.
            let methods = match model.supported_actions {
                Some(actions) => json!(actions),
                None => self.call(reqwest::Method::GET, &format!("/{name}"), None)?["supportedGenerationMethods"].clone(),
            };
            models.push(json!({"name":name,"displayName":model.display_name,"supportedGenerationMethods":methods}));
        }
        Ok(json!({"models":models}))
    }

    pub fn generate(&self, model: &str, request: &Value) -> Result<Value, String> {
        validate_model(model)?;
        self.call(reqwest::Method::POST, &format!("/models/{model}:generateContent"), Some(request))
    }

    pub(crate) fn batch_get(&self, name: &str) -> Result<Value, String> {
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
                // a results file of a whole book is hundreds of MB: the client's 180 s is too short for it
                let long = rust_genai::types::http::HttpOptions {
                    timeout: Some(3_600_000),
                    ..Default::default()
                };
                let config = rust_genai::types::files::DownloadFileConfig {
                    http_options: Some(long),
                    ..Default::default()
                };
                let bytes = self
                    .runtime
                    .block_on(self.sdk.files().download_with_config(&file, config))
                    .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
                let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                let mut rows = Vec::new();
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    let mut row: Value = serde_json::from_str(line).map_err(|e| format!("invalid Google Batch JSONL: {e}"))?;
                    row["metadata"] = json!({"key":row["key"]});
                    rows.push(row);
                }
                op["response"] = json!({"inlinedResponses":{"inlinedResponses":rows}});
            }
        }
        Ok(op)
    }

    pub(crate) fn upload_requests(&self, requests: &Value) -> Result<String, String> {
        let mut jsonl = String::new();
        for row in requests.as_array().ok_or("Batch requests must be an array")? {
            jsonl.push_str(&json!({"key":row["metadata"]["key"],"request":row["request"]}).to_string());
            jsonl.push('\n');
        }
        let file = self
            .runtime
            .block_on(self.sdk.files().upload(jsonl.into_bytes(), "application/jsonl"))
            .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
        file.name.ok_or("Google uploaded JSONL has no name".into())
    }

    /// One attempt only: a retry after a lost answer would create and bill a second batch.
    pub(crate) fn batch_create(&self, model: &str, display: &str, file: &str) -> Result<Created, String> {
        validate_model(model)?;
        let src = rust_genai::types::batches::BatchJobSource {
            file_name: Some(file.into()),
            ..Default::default()
        };
        let once = rust_genai::types::http::HttpOptions {
            retry_options: Some(rust_genai::types::http::HttpRetryOptions {
                attempts: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        };
        let config = rust_genai::types::batches::CreateBatchJobConfig {
            display_name: Some(display.into()),
            http_options: Some(once),
            ..Default::default()
        };
        match self.runtime.block_on(self.sdk.batches().create(model, src, config)) {
            Ok(job) => Ok(Created::Name(job.name.ok_or("Google Batch has no name")?)),
            // a 4xx other than 408 is a definite refusal: nothing was created
            Err(e) if e.status().is_some_and(|s| s.is_client_error() && s.as_u16() != 408) => {
                let code = e.status().map_or(0, |s| s.as_u16());
                Ok(Created::Rejected(code, e.to_string().replace(&self.key, "[redacted]")))
            }
            Err(e) => Err(e.to_string().replace(&self.key, "[redacted]")),
        }
    }

    /// The batches carrying this display name (each submission gets its own).
    pub(crate) fn batch_find(&self, display: &str) -> Result<Vec<String>, String> {
        let mut found = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let config = rust_genai::types::batches::ListBatchJobsConfig {
                page_size: Some(100),
                page_token: token.clone(),
                ..Default::default()
            };
            let page = self
                .runtime
                .block_on(self.sdk.batches().list_with_config(config))
                .map_err(|e| e.to_string().replace(&self.key, "[redacted]"))?;
            for job in page.batch_jobs.unwrap_or_default() {
                if job.display_name.as_deref() == Some(display) {
                    found.extend(job.name);
                }
            }
            match page.next_page_token.filter(|t| !t.is_empty()) {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        Ok(found)
    }
}

/// What creating a batch came to.
pub(crate) enum Created {
    Name(String),
    /// Google refused it (status, message): nothing was created.
    Rejected(u16, String),
}

fn validate_model(model: &str) -> Result<(), String> {
    if is_model_id(model) {
        Ok(())
    } else {
        Err("invalid Gemini model id; use the id without models/".into())
    }
}

fn validate_batch(name: &str) -> Result<(), String> {
    let id = name.strip_prefix("batches/").unwrap_or_default();
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) {
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
    let candidate = response["candidates"].as_array().and_then(|a| a.first()).ok_or_else(|| {
        format!(
            "Google returned no candidate (block reason: {})",
            response["promptFeedback"]["blockReason"]
        )
    })?;
    if candidate["finishReason"].as_str().is_some_and(|r| r != "STOP") {
        return Err(format!("Google TTS unfinished: {}", candidate["finishReason"]));
    }
    let parts = candidate["content"]["parts"].as_array().ok_or("Google returned no audio parts")?;
    let audio: Vec<_> = parts.iter().filter_map(|p| p.get("inlineData")).collect();
    if audio.len() != 1 {
        return Err(format!("Google returned {} audio parts; expected one complete clip", audio.len()));
    }
    let mime = audio[0]["mimeType"].as_str().ok_or("Google audio has no MIME type")?;
    if !mime.starts_with("audio/") {
        return Err("Google inline data is not audio".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio[0]["data"].as_str().ok_or("Google audio has no data")?)
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
        * if chrono::Utc::now().date_naive() > chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap() {
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

/// The Gemini voice model a studio starts with.
pub const DEFAULT_MODEL: &str = "gemini-3.8-flash-lite-tts";

pub(crate) fn is_model_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_llm::test_http::{serve, Reply};

    #[test]
    fn catalog_preserves_developer_api_batch_capabilities() {
        let server = serve(vec![
            Reply::json(
                200,
                r#"{"models":[{"name":"models/gemini-3.8-flash-lite-tts","displayName":"Flash-Lite TTS"}]}"#,
            ),
            Reply::json(200, r#"{"supportedGenerationMethods":["generateContent","batchGenerateContent"]}"#),
        ]);
        let client = Client::with_base("private-key".into(), &server.base()).unwrap();
        let catalog = client.models().unwrap();
        assert!(catalog["models"][0]["supportedGenerationMethods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "batchGenerateContent"));
        assert!(server.request(1).starts_with("GET /v1beta/models/gemini-3.8-flash-lite-tts"));
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
        for m in ["", "models/x", "../other", "https://evil.invalid", "x?key=k"] {
            assert!(validate_model(m).is_err());
        }
        assert!(validate_batch("batches/../other").is_err());
    }

    #[test]
    fn http_errors_redact_the_key_and_cost_uses_audio_usage() {
        let server = serve(vec![Reply::json(403, r#"{"error":"private-key rejected"}"#)]);
        let client = Client::with_base("private-key".into(), &server.base()).unwrap();
        let err = client.models().unwrap_err();
        assert!(!err.contains("private-key"));
        assert!(err.contains("redacted"));
        let usage =
            json!({"promptTokenCount":8,"candidatesTokenCount":85,"candidatesTokensDetails":[{"modality":"AUDIO","tokenCount":85}]});
        let a = estimate("gemini-3.8-flash-lite-tts", false, &usage)["usd"].as_f64().unwrap();
        let b = estimate("gemini-3.8-flash-lite-tts", true, &usage)["usd"].as_f64().unwrap();
        assert_eq!(b, a / 2.0);
        assert!(estimate("unknown", true, &usage).is_null());
    }
}
