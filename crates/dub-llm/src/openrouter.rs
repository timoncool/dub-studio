//! OpenRouter напрямую из Rust (вместо Go-сайдкара): каталог моделей, проверка ключа, синтез речи и
//! транскрипция. Чат — `ChatClient::openrouter`. Все запросы идут по маршруту прокси (`net::builder`).
//!
//! id моделей здесь не живут никогда: каталог берётся из `GET /models`, модель выбирает пользователь.
//! Типы каталога — порт providers/openrouter.rs студий; возможности — свои, под стадии Dub Studio.

use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const API_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// Все модели разом: без фильтра `/models` отдаёт только текстовые, синтез речи и транскрипция публикуются
/// лишь под своими фильтрами.
const ALL_MODELS_PATH: &str = "/models?output_modalities=all";
const REFERER: &str = "https://github.com/timoncool/dub-studio";
const TITLE: &str = "Dub Studio";

/// Что стадия Dub Studio просит у модели.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Перевод: текст на входе и на выходе.
    Text,
    /// Анализ кадров: картинка на входе, текст на выходе.
    Vision,
    /// Озвучка через /audio/speech.
    Speech,
    /// Транскрипция через /audio/transcriptions.
    Transcription,
}

impl Capability {
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "llm" | "text" => Some(Capability::Text),
            "vision" => Some(Capability::Vision),
            "tts" | "speech" => Some(Capability::Speech),
            "asr" | "stt" | "transcription" => Some(Capability::Transcription),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<RemoteModel>,
    #[serde(default)]
    links: Option<Links>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Links {
    #[serde(default)]
    next: Option<String>,
}

/// Сэмплинг, который модель публикует для себя в `default_parameters`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ModelDefaults {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<i64>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub repetition_penalty: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteModel {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    context_length: Option<u64>,
    #[serde(default)]
    supported_parameters: Option<Vec<String>>,
    #[serde(default)]
    pricing: Option<ModelPricing>,
    #[serde(default)]
    architecture: ModelArchitecture,
    #[serde(default)]
    default_parameters: Option<ModelDefaults>,
    #[serde(default)]
    reasoning: Option<ReasoningSupport>,
    #[serde(default)]
    supported_voices: Option<Vec<String>>,
}

/// Правила модели о рассуждениях — как их публикует OpenRouter.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ReasoningSupport {
    pub default_effort: Option<String>,
    pub default_enabled: Option<bool>,
    /// Думает всегда, выключить нельзя.
    pub mandatory: bool,
    /// Единственные значения, которые она принимает.
    pub supported_efforts: Vec<String>,
}

impl ReasoningSupport {
    /// Усилие для желаемой настройки или ничего. Неизвестное модели усилие становится тем, которое она
    /// предпочитает: запрос с чужим значением отвергается; модели, которая обязана думать, «off» не шлём.
    pub fn effort_for(&self, wanted: Option<&str>) -> Option<String> {
        let wanted = wanted.map(str::trim).filter(|value| !value.is_empty());
        if matches!(wanted, Some("off")) {
            return if self.mandatory { self.default_effort.clone() } else { None };
        }
        let wanted = wanted?;
        if self.supported_efforts.is_empty() || self.supported_efforts.iter().any(|value| value == wanted) {
            return Some(wanted.to_string());
        }
        self.default_effort.clone().or_else(|| self.supported_efforts.first().cloned())
    }
}

/// Цены — десятичные строки, как в API (без округления float). Ключи — как у OpenRouter.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ModelPricing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ModelArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

/// Модель каталога. `capabilities` — «подходит по заявленным модальностям», не гарантия качества.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub context_length: Option<u64>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub supported_parameters: Vec<String>,
    pub pricing: Option<ModelPricing>,
    #[serde(default)]
    pub defaults: ModelDefaults,
    pub reasoning: Option<ReasoningSupport>,
    #[serde(default)]
    pub voices: Vec<String>,
    pub capabilities: Vec<Capability>,
}

impl CatalogModel {
    fn from_remote(model: RemoteModel) -> Self {
        let input = normalized(model.architecture.input_modalities);
        let output = normalized(model.architecture.output_modalities);
        let capabilities = infer_capabilities(&input, &output);
        Self {
            name: if model.name.trim().is_empty() { model.id.clone() } else { model.name },
            id: model.id,
            context_length: model.context_length,
            input_modalities: input,
            output_modalities: output,
            supported_parameters: normalized(model.supported_parameters.unwrap_or_default()),
            pricing: model.pricing,
            defaults: model.default_parameters.unwrap_or_default(),
            reasoning: model.reasoning,
            voices: model.supported_voices.unwrap_or_default(),
            capabilities,
        }
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// То, что нужно чат-клиенту от записи каталога.
    pub fn profile(&self) -> ModelProfile {
        ModelProfile {
            supported_parameters: self.supported_parameters.clone(),
            reasoning: self.reasoning.clone(),
            defaults: self.defaults.clone(),
        }
    }
}

/// Запись каталога, которую чат-клиент учитывает в теле запроса: какие параметры модель принимает, как она
/// думает и какой сэмплинг публикует для себя.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelProfile {
    pub supported_parameters: Vec<String>,
    pub reasoning: Option<ReasoningSupport>,
    pub defaults: ModelDefaults,
}

impl ModelProfile {
    /// Принимает ли модель параметр. Модель, не опубликовавшая список, — неизвестно: считаем, что принимает.
    pub fn accepts(&self, parameter: &str) -> bool {
        self.supported_parameters.is_empty() || self.supported_parameters.iter().any(|known| known == parameter)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct CapabilityCatalog {
    pub models: Vec<CatalogModel>,
}

impl CapabilityCatalog {
    /// Ответ `GET /models` -> каталог; модели без подходящих Dub Studio возможностей отбрасываются.
    pub fn parse(body: &str) -> Result<Self> {
        let response: ModelsResponse = serde_json::from_str(body).context("invalid OpenRouter models response")?;
        let mut catalog = Self::default();
        catalog.merge(response.data);
        Ok(catalog)
    }

    fn merge(&mut self, data: Vec<RemoteModel>) {
        let mut known: BTreeSet<String> = self.models.iter().map(|model| model.id.clone()).collect();
        for model in data.into_iter().map(CatalogModel::from_remote) {
            if !model.capabilities.is_empty() && known.insert(model.id.clone()) {
                self.models.push(model);
            }
        }
        self.models.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    }

    pub fn models_for(&self, capability: Capability) -> impl Iterator<Item = &CatalogModel> {
        self.models.iter().filter(move |model| model.supports(capability))
    }

    pub fn find(&self, model_id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|model| model.id == model_id)
    }

    /// Выбранная модель, если каталог её знает и она заявляет нужную возможность.
    pub fn selected(&self, capability: Capability, model_id: &str) -> Result<&CatalogModel> {
        let model = self
            .find(model_id)
            .with_context(|| {
            format!("OpenRouter model '{model_id}' is not present in the refreshed catalog")
        })?;
        if !model.supports(capability) {
            bail!("OpenRouter model '{model_id}' does not declare support for {capability:?}");
        }
        Ok(model)
    }
}

fn infer_capabilities(input: &[String], output: &[String]) -> Vec<Capability> {
    let has = |list: &[String], what: &str| list.iter().any(|value| value == what);
    let mut capabilities = Vec::new();
    if has(input, "text") && has(output, "text") {
        capabilities.push(Capability::Text);
    }
    if has(input, "image") && has(output, "text") {
        capabilities.push(Capability::Vision);
    }
    if has(output, "speech") {
        capabilities.push(Capability::Speech);
    }
    if has(output, "transcription") {
        capabilities.push(Capability::Transcription);
    }
    capabilities
}

fn normalized(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

/// Итог проверки ключа.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyCheck {
    /// OpenRouter принял ключ; `data` из `GET /key` (label, limit, usage, …).
    Accepted(Value),
    /// OpenRouter ключ отверг (401/403) — с текстом ответа.
    Rejected(String),
}

/// Результат синтеза речи.
#[derive(Debug, Clone, PartialEq)]
pub enum SpeechAudio {
    /// Готовый WAV (PCM, частота и каналы — из ответа).
    Wav(Vec<u8>),
    /// Сжатое аудио (`mime` из Content-Type) — его нужно раскодировать.
    Encoded { mime: String, bytes: Vec<u8> },
}

/// Сегмент транскрипции.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub duration: Option<f64>,
    pub segments: Vec<TranscriptSegment>,
}

/// Клиент REST OpenRouter (кроме чата). `base` подменяется только в тестах.
pub struct OpenRouter {
    base: String,
    key: Option<String>,
    http: reqwest::blocking::Client,
    retries: u32,
}

impl OpenRouter {
    pub fn new(key: Option<String>) -> Result<Self> {
        Self::with_base(API_BASE_URL, key)
    }

    pub fn with_base(base: &str, key: Option<String>) -> Result<Self> {
        let http = crate::net::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .context("OpenRouter HTTP client")?;
        Ok(OpenRouter {
            base: base.trim_end_matches('/').to_string(),
            key: key.map(|key| key.trim().to_string()).filter(|key| !key.is_empty()),
            http,
            retries: 2,
        })
    }

    fn get(&self, path: &str) -> reqwest::blocking::RequestBuilder {
        let url = if path.starts_with("http://") || path.starts_with("https://") { path.to_string() } else { format!("{}{path}", self.base) };
        self.decorate(self.http.get(url))
    }

    fn decorate(&self, request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        let request = request.header("HTTP-Referer", REFERER).header("X-Title", TITLE);
        match &self.key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }

    fn key(&self) -> Result<&str> {
        self.key.as_deref().ok_or_else(|| anyhow!("no OpenRouter API key is configured"))
    }

    /// Каталог всех моделей (ключ не нужен). Постраничные ответы (`links.next`) собираются целиком.
    pub fn catalog(&self) -> Result<CapabilityCatalog> {
        let mut catalog = CapabilityCatalog::default();
        let mut next = Some(ALL_MODELS_PATH.to_string());
        let mut pages = 0;
        while let Some(path) = next.take() {
            pages += 1;
            if pages > 20 {
                bail!("OpenRouter models listing did not end after 20 pages");
            }
            let response = self.get(&path).timeout(Duration::from_secs(60)).send().map_err(|e| anyhow!("OpenRouter models: {}", crate::net::why(&e)))?;
            let status = response.status();
            let body = response.text().map_err(|e| anyhow!("OpenRouter models: {}", crate::net::why(&e)))?;
            if !status.is_success() {
                bail!("OpenRouter models answered {status}: {}", snippet(&body));
            }
            let page: ModelsResponse = serde_json::from_str(&body).context("invalid OpenRouter models response")?;
            let origin = self.base.trim_end_matches("/api/v1");
            next = match page.links.and_then(|links| links.next).filter(|next| !next.trim().is_empty()) {
                // get() шлёт с запросом ключ: странице на другом хосте его не отдаём.
                Some(next) if next.contains("://") && !next.starts_with(&format!("{origin}/")) => {
                    bail!("OpenRouter models listing names its next page on another host: {next}")
                }
                Some(next) if next.starts_with('/') && !next.starts_with("/models") => {
                    Some(format!("{origin}{next}"))
                }
                other => other,
            };
            catalog.merge(page.data);
        }
        Ok(catalog)
    }

    /// `GET /key`: принят ли ключ и что о нём известно (в т.ч. `usage` — потрачено ключом, USD).
    pub fn check_key(&self) -> Result<KeyCheck> {
        self.key()?;
        let response = self.get("/key").timeout(Duration::from_secs(20)).send().map_err(|e| anyhow!("OpenRouter key check: {}", crate::net::why(&e)))?;
        let status = response.status();
        let body = response.text().map_err(|e| anyhow!("OpenRouter key check: {}", crate::net::why(&e)))?;
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Ok(KeyCheck::Rejected(format!("{status}: {}", snippet(&body))));
        }
        if !status.is_success() {
            bail!("OpenRouter key check answered {status}: {}", snippet(&body));
        }
        let value: Value = serde_json::from_str(&body).context("OpenRouter key check did not return JSON")?;
        Ok(KeyCheck::Accepted(value.get("data").cloned().unwrap_or(value),
        ))
    }

    /// Потрачено этим ключом, USD (поле `usage` из `GET /key`).
    pub fn key_usage_usd(&self) -> Result<f64> {
        match self.check_key()? {
            KeyCheck::Accepted(data) => data.get("usage").and_then(Value::as_f64).ok_or_else(|| anyhow!("OpenRouter key info has no usage")),
            KeyCheck::Rejected(detail) => bail!("OpenRouter rejected the key: {detail}"),
        }
    }

    /// POST с ретраями на сетевых ошибках, 429 и 5xx; 4xx — сразу ошибка с телом ответа.
    fn post(&self, path: &str, body: &Value, timeout: Duration,
    ) -> Result<reqwest::blocking::Response> {
        self.key()?;
        let url = format!("{}{path}", self.base);
        let mut last = String::new();
        for attempt in 0..=self.retries {
            match self.decorate(self.http.post(&url)).json(body).timeout(timeout).send() {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().unwrap_or_default();
                    if status.as_u16() != 429 && status.as_u16() < 500 {
                        bail!("OpenRouter {path} answered {status}: {}", snippet(&text));
                    }
                    last = format!("{status}: {}", snippet(&text));
                }
                Err(error) => last = crate::net::why(&error),
            }
            if attempt < self.retries {
                std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
            }
        }
        bail!("OpenRouter {path} failed after {} attempts: {last}", self.retries + 1)
    }

    /// Синтез речи (`/audio/speech`, формат pcm). PCM оборачивается в WAV с частотой и числом каналов из
    /// Content-Type (`audio/pcm;rate=24000;channels=1`); WAV отдаётся как есть; сжатый формат — как есть с mime.
    pub fn speech(&self, model: &str, input: &str, voice: &str) -> Result<SpeechAudio> {
        self.speech_with_style(model, input, voice, "")
    }

    pub fn speech_with_style(
        &self,
        model: &str,
        input: &str,
        voice: &str,
        style: &str,
    ) -> Result<SpeechAudio> {
        if model.trim().is_empty() {
            bail!("OpenRouter speech needs a model id");
        }
        let mut body = json!({ "model": model, "input": input, "voice": voice, "response_format": "pcm" });
        if !style.trim().is_empty() {
            if !model.starts_with("google/gemini-") {
                bail!("speech style metadata is only supported for Gemini TTS");
            }
            body["provider"] =
                json!({"options":{"google-ai-studio":{"speech_metadata":{"style":style}}}});
        }
        let response = self.post("/audio/speech", &body, Duration::from_secs(180))?;
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("audio/pcm")
            .to_ascii_lowercase();
        let bytes = response.bytes().map_err(|e| anyhow!("OpenRouter speech body: {}", crate::net::why(&e)))?.to_vec();
        speech_audio(&mime, bytes)
    }

    /// Транскрипция (`/audio/transcriptions`, verbose_json с сегментами). `language` пусто или "auto" —
    /// автоопределение.
    pub fn transcribe(&self, model: &str, audio: &[u8], format: &str, language: &str,
    ) -> Result<Transcript> {
        if model.trim().is_empty() {
            bail!("OpenRouter transcription needs a model id");
        }
        if audio.is_empty() {
            bail!("no audio to transcribe");
        }
        let mut body = json!({
            "model": model,
            "input_audio": { "data": base64::engine::general_purpose::STANDARD.encode(audio), "format": format },
            "response_format": "verbose_json",
            "timestamp_granularities": ["segment"],
        });
        let language = language.trim();
        if !language.is_empty() && !language.eq_ignore_ascii_case("auto") {
            body["language"] = Value::String(language.to_string());
        }
        let response = self.post("/audio/transcriptions", &body, Duration::from_secs(900))?;
        let value: Value = response.json().map_err(|e| {
            anyhow!("OpenRouter transcription did not return JSON: {}", crate::net::why(&e))
        })?;
        Ok(parse_transcript(&value))
    }
}

fn parse_transcript(value: &Value) -> Transcript {
    let segments = value
        .get("segments")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|segment| {
                    let start = segment.get("start").and_then(Value::as_f64)?;
                    let end = segment.get("end").and_then(Value::as_f64)?;
                    let text = segment.get("text").and_then(Value::as_str)?.trim().to_string();
                    (!text.is_empty()).then_some(TranscriptSegment { start, end, text })
                })
                .collect()
        })
        .unwrap_or_default();
    Transcript {
        text: value.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
        duration: value.get("duration").and_then(Value::as_f64),
        segments,
    }
}

/// Параметр `name=value` из Content-Type.
fn mime_param(mime: &str, name: &str) -> Option<u32> {
    mime.split(';').skip(1).find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key.trim() == name).then(|| value.trim().trim_matches('"').parse().ok()).flatten()
    })
}

fn speech_audio(mime: &str, bytes: Vec<u8>) -> Result<SpeechAudio> {
    if bytes.is_empty() {
        bail!("OpenRouter speech returned no audio");
    }
    let base = mime.split(';').next().unwrap_or_default().trim();
    match base {
        "audio/pcm" | "audio/l16" | "audio/raw" | "application/octet-stream" => {
            let rate = mime_param(mime, "rate").unwrap_or(24_000);
            let channels = mime_param(mime, "channels").unwrap_or(1);
            Ok(SpeechAudio::Wav(pcm16_to_wav(&bytes, rate, channels as u16,
            )?))
        }
        "audio/wav" | "audio/wave" | "audio/x-wav" => Ok(SpeechAudio::Wav(bytes)),
        _ if base.starts_with("audio/") => Ok(SpeechAudio::Encoded { mime: base.to_string(), bytes,
        }),
        other => bail!("OpenRouter speech returned {other}, not audio"),
    }
}

/// Сырой PCM s16le -> WAV (WAVE_FORMAT_PCM, 16 бит): такой заголовок принимают и hound, и ffmpeg.
/// Незаконченный последний кадр (обрыв потока посреди сэмпла) отбрасывается.
pub fn pcm16_to_wav(pcm: &[u8], rate: u32, channels: u16) -> Result<Vec<u8>> {
    if channels == 0 || rate == 0 {
        bail!("PCM with {channels} channels at {rate} Hz");
    }
    let frame = 2 * channels as usize;
    let pcm = &pcm[..pcm.len() - pcm.len() % frame];
    let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int,
    };
    let mut out = std::io::Cursor::new(Vec::with_capacity(pcm.len() + 44));
    {
        let mut writer = hound::WavWriter::new(&mut out, spec).context("WAV header")?;
        let mut samples = writer.get_i16_writer((pcm.len() / 2) as u32);
        for pair in pcm.as_chunks::<2>().0 {
            samples.write_sample(i16::from_le_bytes(*pair));
        }
        samples.flush().context("WAV samples")?;
        writer.finalize().context("WAV finalize")?;
    }
    Ok(out.into_inner())
}

fn snippet(text: &str) -> String {
    text.trim().chars().take(600).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_http::{serve, Reply};

    const LISTING: &str = r#"{"data":[
        {"id":"a/text","name":"Text","context_length":131072,"pricing":{"prompt":"0.0000003","completion":"0.0000025"},
         "supported_parameters":["temperature","top_p","top_k","reasoning"],
         "reasoning":{"mandatory":false,"default_enabled":true,"supported_efforts":["high","low"],"default_effort":"high"},
         "architecture":{"input_modalities":["text","image"],"output_modalities":["text"]}},
        {"id":"b/tts","name":"Voice","supported_voices":["alloy"],"architecture":{"input_modalities":["text"],"output_modalities":["speech"]}},
        {"id":"c/stt","name":"Ears","architecture":{"input_modalities":["audio"],"output_modalities":["transcription"]}},
        {"id":"d/embed","name":"Embed","architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}}
    ],"links":{"next":null}}"#;

    #[test]
    fn the_catalog_knows_what_each_model_can_do_for_a_stage() {
        let catalog = CapabilityCatalog::parse(LISTING).unwrap();
        assert_eq!(catalog.models.len(), 3, "a model with no stage to serve is dropped");
        let text = catalog.selected(Capability::Text, "a/text").unwrap();
        assert!(text.supports(Capability::Vision));
        assert_eq!(text.context_length, Some(131072));
        assert_eq!(text.pricing.as_ref().and_then(|p| p.prompt.as_deref()), Some("0.0000003"));
        assert!(catalog.selected(Capability::Speech, "a/text").is_err());
        assert_eq!(catalog.models_for(Capability::Speech).map(|m| m.id.as_str()).collect::<Vec<_>>(), ["b/tts"]);
        assert_eq!(catalog.find("b/tts").unwrap().voices, ["alloy"]);
        assert_eq!(catalog.models_for(Capability::Transcription).count(), 1);
        assert!(catalog.selected(Capability::Text, "gone/model").is_err());
        assert!(text.profile().accepts("top_k") && !text.profile().accepts("repetition_penalty"));
    }

    #[test]
    fn an_effort_a_model_does_not_take_becomes_the_one_it_named() {
        let deepseek = ReasoningSupport {
            default_effort: Some("high".into()),
            default_enabled: None,
            mandatory: false,
            supported_efforts: vec!["max".into(), "high".into(), "low".into()],
        };
        assert_eq!(deepseek.effort_for(Some("max")).as_deref(), Some("max"));
        assert_eq!(deepseek.effort_for(Some("medium")).as_deref(), Some("high"));
        assert_eq!(deepseek.effort_for(Some("off")), None);
        let glm = ReasoningSupport { mandatory: true, default_effort: Some("max".into()), ..deepseek.clone() };
        assert_eq!(glm.effort_for(Some("off")).as_deref(), Some("max"));
        assert_eq!(glm.effort_for(Some("low")).as_deref(), Some("low"));
    }

    #[test]
    fn raw_pcm_becomes_a_wav_at_the_rate_the_server_named() {
        let pcm: Vec<u8> = [0i16, 1000, -1000, 32767].iter().flat_map(|s| s.to_le_bytes()).collect();
        let SpeechAudio::Wav(wav) = speech_audio("audio/pcm;rate=44100;channels=1", pcm.clone()).unwrap() else { panic!("wav") };
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap();
        assert_eq!(reader.spec().sample_rate, 44100);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.into_samples::<i16>().map(Result::unwrap).collect::<Vec<_>>(), [0, 1000, -1000, 32767]);
        let SpeechAudio::Wav(default) = speech_audio("audio/pcm", pcm).unwrap() else { panic!("wav") };
        assert_eq!(hound::WavReader::new(std::io::Cursor::new(&default)).unwrap().spec().sample_rate, 24000);
        assert!(matches!(speech_audio("audio/mpeg", vec![1, 2]).unwrap(), SpeechAudio::Encoded { .. }));
        assert!(speech_audio("text/html", vec![1]).is_err());
        assert!(speech_audio("audio/pcm", vec![]).is_err());
        let torn = pcm16_to_wav(&[1, 0, 2, 0, 3, 0, 9], 24_000, 2).unwrap();
        assert_eq!(hound::WavReader::new(std::io::Cursor::new(&torn)).unwrap().len(), 2, "a torn last frame is dropped");
    }

    #[test]
    fn the_catalog_is_read_from_the_api_without_a_key() {
        let server = serve(vec![Reply::json(200, LISTING)]);
        let catalog = OpenRouter::with_base(&server.base(), None).unwrap().catalog().unwrap();
        let request = server.request(0);
        assert!(request.starts_with("GET /models?output_modalities=all "), "{request}");
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert_eq!(catalog.models.len(), 3);
    }

    #[test]
    fn a_key_is_accepted_or_rejected_by_openrouter_itself() {
        let server = serve(vec![
            Reply::json(200, r#"{"data":{"label":"sk-or-v1-abc...","usage":1.25,"limit":null}}"#,
            ),
            Reply::json(401, r#"{"error":{"message":"User not found.","code":401}}"#),
        ]);
        let accepted = OpenRouter::with_base(&server.base(), Some("sk-good".into())).unwrap();
        assert_eq!(accepted.key_usage_usd().unwrap(), 1.25);
        assert!(server.request(0).to_ascii_lowercase().contains("authorization: bearer sk-good"));
        let rejected = OpenRouter::with_base(&server.base(), Some("sk-bad".into())).unwrap().check_key().unwrap();
        assert!(matches!(rejected, KeyCheck::Rejected(detail) if detail.contains("User not found")));
        assert!(OpenRouter::with_base(&server.base(), None).unwrap().check_key().is_err());
    }

    #[test]
    fn speech_asks_for_pcm_and_wraps_it() {
        let pcm: Vec<u8> = [5i16, -5].iter().flat_map(|s| s.to_le_bytes()).collect();
        let server = serve(vec![Reply::bytes(200, "audio/pcm;rate=24000;channels=1", pcm,
        )]);
        let audio = OpenRouter::with_base(&server.base(), Some("k".into())).unwrap().speech("b/tts", "Привет", "alloy").unwrap();
        let SpeechAudio::Wav(wav) = audio else { panic!("wav") };
        assert_eq!(hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap().len(), 2);
        let body = crate::test_http::body_json(&server.request(0));
        assert_eq!(body, json!({ "model": "b/tts", "input": "Привет", "voice": "alloy", "response_format": "pcm" }));
    }

    #[test]
    fn transcription_sends_base64_audio_and_reads_segments() {
        let server = serve(vec![
            Reply::json(503, r#"{"error":"busy"}"#),
            Reply::json(200, r#"{"text":"hello there","duration":2.5,"segments":[{"start":0.0,"end":1.2,"text":" hello "},{"start":1.2,"end":2.5,"text":"there"},{"start":2.5,"end":2.6,"text":"  "}]}"#,
            ),
        ]);
        let transcript = OpenRouter::with_base(&server.base(), Some("k".into())).unwrap().transcribe("c/stt", b"RIFF", "wav", "en").unwrap();
        assert_eq!(transcript.segments, vec![
            TranscriptSegment { start: 0.0, end: 1.2, text: "hello".into() },
            TranscriptSegment { start: 1.2, end: 2.5, text: "there".into() },
        ]);
        assert_eq!(transcript.duration, Some(2.5));
        let body = crate::test_http::body_json(&server.request(1));
        assert_eq!(body["input_audio"], json!({ "data": "UklGRg==", "format": "wav" }));
        assert_eq!(body["language"], "en");
        assert_eq!(body["response_format"], "verbose_json");
        let auto = serve(vec![Reply::json(200, r#"{"text":"x"}"#)]);
        OpenRouter::with_base(&auto.base(), Some("k".into())).unwrap().transcribe("c/stt", b"RIFF", "wav", "auto").unwrap();
        assert!(crate::test_http::body_json(&auto.request(0)).get("language").is_none());
    }

    #[test]
    fn a_request_error_carries_the_answer() {
        let server = serve(vec![Reply::json(400, r#"{"error":{"message":"voice not supported"}}"#,
        )]);
        let error = OpenRouter::with_base(&server.base(), Some("k".into())).unwrap().speech("b/tts", "x", "nope").unwrap_err();
        assert!(error.to_string().contains("voice not supported"), "{error}");
    }
}
