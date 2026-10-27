//! Recognition through `POST /v1/audio/transcriptions/details`: the JSON route with a WAV on this computer, so a long
//! recording is not sent over the socket. Spans come in samples and are turned into seconds by the answer's rate.

use crate::speech::json_or_error;
use crate::{AcppError, Server};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What to recognise. The WAV must be 16 kHz mono: a server built without SOXR does not resample, and the studios
/// make such a WAV for recognition anyway.
#[derive(Clone, Debug)]
pub struct TranscribeRequest {
    pub model: String,
    pub wav: PathBuf,
    /// A Whisper language code; `None` for a model that finds the language itself (Parakeet).
    pub language: Option<String>,
    /// Words and spellings to expect (the model's context text).
    pub prompt: Option<String>,
    pub options: Map<String, Value>,
    /// The longest wait for the answer, which comes after the whole recording is recognised.
    pub within: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptSpan {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub words: Vec<TranscriptWord>,
    pub segments: Vec<TranscriptSpan>,
}

pub(crate) fn check_16k_mono(wav: &Path) -> Result<(), AcppError> {
    let spec = hound::WavReader::open(wav).map_err(|e| AcppError::Input { path: wav.display().to_string(), why: e.to_string() })?.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 {
        return Err(AcppError::Input {
            path: wav.display().to_string(),
            why: format!("recognition takes 16 kHz mono, this is {} Hz with {} channels", spec.sample_rate, spec.channels),
        });
    }
    Ok(())
}

pub(crate) fn parse_transcript(v: &Value) -> Result<Transcript, AcppError> {
    let text = v.get("text").and_then(Value::as_str).ok_or_else(|| AcppError::Protocol("the transcript has no text".into()))?.to_string();
    let rate = v.get("sample_rate").and_then(Value::as_f64);
    let spans = |key: &str| -> Result<Vec<(String, f64, f64, f32)>, AcppError> {
        let Some(items) = v.get(key).and_then(Value::as_array) else { return Ok(Vec::new()) };
        let rate = rate.filter(|r| *r > 0.0).ok_or_else(|| AcppError::Protocol(format!("the transcript has {key} but no sample_rate")))?;
        items
            .iter()
            .map(|w| {
                let at = |k: &str| w.get(k).and_then(Value::as_f64).ok_or_else(|| AcppError::Protocol(format!("{key}: an item without {k}")));
                let text = w.get("word").or_else(|| w.get("text")).and_then(Value::as_str).unwrap_or("").to_string();
                let confidence = w.get("confidence").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                Ok((text, at("start_sample")? / rate, at("end_sample")? / rate, confidence))
            })
            .collect()
    };
    let words = spans("words")?
        .into_iter()
        .filter(|(t, ..)| !t.trim().is_empty())
        .map(|(t, start, end, confidence)| TranscriptWord { text: t.trim().to_string(), start, end: end.max(start), confidence })
        .collect();
    let segments = spans("segments")?.into_iter().map(|(text, start, end, confidence)| TranscriptSpan { text, start, end: end.max(start), confidence }).collect();
    Ok(Transcript { text, language: v.get("language").and_then(Value::as_str).map(str::to_string), words, segments })
}

impl Server {
    pub fn transcribe_details(&self, req: &TranscribeRequest) -> Result<Transcript, AcppError> {
        check_16k_mono(&req.wav)?;
        let mut body = json!({ "model": req.model, "audio": req.wav.to_string_lossy() });
        if let Some(l) = &req.language {
            body["language"] = json!(l);
        }
        if let Some(p) = &req.prompt {
            body["text"] = json!(p);
        }
        if !req.options.is_empty() {
            body["options"] = Value::Object(req.options.clone());
        }
        let mut call = self.http.post(format!("{}/v1/audio/transcriptions/details", self.base)).json(&body);
        if let Some(within) = req.within {
            call = call.timeout(within);
        }
        let resp = call
            .send()
            .map_err(|e| AcppError::Io(format!("audiocpp_server transcription: {e}")))?;
        parse_transcript(&json_or_error(resp)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_in_samples_become_seconds() {
        let v: Value = serde_json::from_str(
            r#"{"text":"the task","language":"en","sample_rate":16000,
                "words":[{"word":" the","start_sample":3200,"end_sample":6400,"confidence":0.98},{"word":" ","start_sample":6400,"end_sample":6400},
                         {"word":"task","start_sample":6400,"end_sample":12800,"confidence":0}],
                "timing":{"wall_ms":1.0}}"#,
        )
        .unwrap();
        let t = parse_transcript(&v).unwrap();
        assert_eq!(t.language.as_deref(), Some("en"));
        assert_eq!(t.words.len(), 2);
        assert_eq!(t.words[0], TranscriptWord { text: "the".into(), start: 0.2, end: 0.4, confidence: 0.98 });
        assert_eq!((t.words[1].start, t.words[1].end), (0.4, 0.8));
        assert!(t.segments.is_empty());
    }

    #[test]
    fn spans_without_a_rate_and_a_wrong_wav_are_refused() {
        let v: Value = serde_json::from_str(r#"{"text":"x","words":[{"word":"x","start_sample":1,"end_sample":2}]}"#).unwrap();
        assert!(matches!(parse_transcript(&v), Err(AcppError::Protocol(_))));
        let dir = std::env::temp_dir().join(format!("acpp-asr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("44k.wav");
        let mut w = hound::WavWriter::create(&p, hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
        w.write_sample(0i16).unwrap();
        w.write_sample(0i16).unwrap();
        w.finalize().unwrap();
        assert!(matches!(check_16k_mono(&p), Err(AcppError::Input { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
