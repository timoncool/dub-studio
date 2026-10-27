//! Speech through `POST /v1/audio/speech` as server-sent events: base64 PCM16 deltas, then `speech.audio.done`.

use crate::sse::{next_event, Event};
use crate::{AcppError, Server};
use base64::Engine as _;
use serde_json::{json, Map, Value};
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The voice to clone: a WAV on this computer (no size limit) or WAV bytes sent inline (the server takes up to 5 MiB).
#[derive(Clone, Debug)]
pub enum VoiceRef {
    Path(PathBuf),
    Wav(Vec<u8>),
}

/// One phrase to speak. `options` are the model's request options (Higgs: temperature, top_p, top_k, seed, max_tokens,
/// allow_incomplete_generation, stream_frames_per_event); an option the model does not know is refused by the server.
#[derive(Clone, Debug)]
pub struct SpeechRequest {
    pub model: String,
    pub input: String,
    pub voice_ref: Option<VoiceRef>,
    pub reference_text: Option<String>,
    /// The language of the text, for a model that is told it (Supertonic); `None` for one that finds it.
    pub language: Option<String>,
    pub options: Map<String, Value>,
    /// The longest wait for the next delta; the first one also waits for a lazy model to load.
    pub stall: Duration,
}

/// What the stream's callback tells the client after a delta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Default)]
pub struct SpeechDone {
    pub samples: usize,
    pub ttft_ms: Option<f64>,
}

/// A whole phrase: mono samples in [-1, 1].
#[derive(Clone, Debug)]
pub struct Speech {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// The body of a successful answer, or the server's `{"error":{"message","type"}}` as an error.
pub(crate) fn json_or_error(resp: reqwest::blocking::Response) -> Result<Value, AcppError> {
    let status = resp.status();
    let text = resp.text().map_err(|e| AcppError::Io(format!("audiocpp_server answer: {e}")))?;
    if !status.is_success() {
        return Err(server_error(status.as_u16(), &text));
    }
    serde_json::from_str(&text).map_err(|e| AcppError::Protocol(format!("the answer is not JSON ({e}): {}", text.chars().take(300).collect::<String>())))
}

pub(crate) fn server_error(status: u16, body: &str) -> AcppError {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let err = v.get("error");
    let message = err.and_then(|e| e.get("message")).and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| body.chars().take(500).collect());
    let kind = err.and_then(|e| e.get("type")).and_then(Value::as_str).unwrap_or("error").to_string();
    AcppError::Server { status, kind, message }
}

fn pcm16(b64: &str) -> Result<Vec<f32>, AcppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| AcppError::Protocol(format!("an audio delta is not base64: {e}")))?;
    if bytes.len() % 2 != 0 {
        return Err(AcppError::Protocol(format!("an audio delta of {} bytes is not PCM16", bytes.len())));
    }
    Ok(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
}

/// Feeds the stream's events to `on_delta` until `[DONE]`.
pub(crate) fn read_stream(
    reader: &mut impl std::io::BufRead,
    stall: Duration,
    cancel: &AtomicBool,
    on_delta: &mut dyn FnMut(&[f32]) -> Flow,
) -> Result<SpeechDone, AcppError> {
    let mut done = SpeechDone::default();
    let mut finished = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AcppError::Cancelled);
        }
        let event = next_event(reader).map_err(|e| match e.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => AcppError::Stalled { secs: stall.as_secs() },
            _ => AcppError::Io(format!("audiocpp_server stream: {e}")),
        })?;
        let data = match event {
            Some(Event::Data(d)) => d,
            Some(Event::Done) if finished => return Ok(done),
            Some(Event::Done) => return Err(AcppError::Protocol("the stream ended before speech.audio.done".into())),
            None => return Err(AcppError::Protocol("the stream broke off before [DONE]".into())),
        };
        let v: Value = serde_json::from_str(&data).map_err(|e| AcppError::Protocol(format!("an event is not JSON ({e}): {data}")))?;
        match v.get("type").and_then(Value::as_str) {
            Some("speech.audio.delta") => {
                let audio = v.get("audio").and_then(Value::as_str).ok_or_else(|| AcppError::Protocol("a delta without audio".into()))?;
                let samples = pcm16(audio)?;
                done.samples += samples.len();
                if on_delta(&samples) == Flow::Stop || cancel.load(Ordering::Relaxed) {
                    return Err(AcppError::Cancelled);
                }
            }
            Some("speech.audio.done") => {
                done.ttft_ms = v.pointer("/timing/ttft_ms").and_then(Value::as_f64);
                finished = true;
            }
            Some("error") => {
                let message = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("no message").to_string();
                return Err(AcppError::Server { status: 200, kind: "stream_error".into(), message });
            }
            _ => {}
        }
    }
}

impl Server {
    fn speech_body(req: &SpeechRequest) -> Value {
        let mut body = json!({
            "model": req.model,
            "input": req.input,
            "response_format": "pcm",
            "stream_format": "sse",
        });
        if let Some(v) = &req.voice_ref {
            body["voice_ref"] = match v {
                VoiceRef::Path(p) => json!({ "type": "path", "path": p.to_string_lossy() }),
                VoiceRef::Wav(bytes) => json!({ "type": "base64", "data": base64::engine::general_purpose::STANDARD.encode(bytes) }),
            };
        }
        if let Some(t) = &req.reference_text {
            body["reference_text"] = json!(t);
        }
        if let Some(l) = &req.language {
            body["language"] = json!(l);
        }
        if !req.options.is_empty() {
            body["options"] = Value::Object(req.options.clone());
        }
        body
    }

    /// The rate of a model's speech deltas, from its entry.
    pub fn speech_rate(&self, model: &str) -> Result<u32, AcppError> {
        self.config
            .model(model)
            .and_then(|m| m.output_rate)
            .ok_or_else(|| AcppError::Protocol(format!("the model «{model}» has no speech rate in its entry")))
    }

    /// Speaks as a stream: every delta goes to `on_delta` as it arrives. `Flow::Stop`, or `cancel` set, closes the
    /// connection, and the server stops at its next delta.
    pub fn speech_stream(&self, req: &SpeechRequest, cancel: &AtomicBool, on_delta: &mut dyn FnMut(&[f32]) -> Flow) -> Result<SpeechDone, AcppError> {
        let resp = self
            .http
            .post(format!("{}/v1/audio/speech", self.base))
            .header("Accept", "text/event-stream")
            .timeout(req.stall)
            .json(&Self::speech_body(req))
            .send()
            .map_err(|e| if e.is_timeout() { AcppError::Stalled { secs: req.stall.as_secs() } } else { AcppError::Io(format!("audiocpp_server speech: {e}")) })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(server_error(status.as_u16(), &text));
        }
        let mut reader = BufReader::with_capacity(1 << 16, resp);
        read_stream(&mut reader, req.stall, cancel, on_delta)
    }

    /// Speaks a whole phrase: the deltas put together.
    pub fn speech(&self, req: &SpeechRequest, cancel: &AtomicBool) -> Result<Speech, AcppError> {
        let sample_rate = self.speech_rate(&req.model)?;
        let mut samples = Vec::new();
        self.speech_stream(req, cancel, &mut |d| {
            samples.extend_from_slice(d);
            Flow::Continue
        })?;
        Ok(Speech { samples, sample_rate })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(samples: &[i16]) -> String {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        format!("data: {{\"type\":\"speech.audio.delta\",\"audio\":\"{}\"}}\n\n", base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    #[test]
    fn deltas_come_in_order_and_done_ends_the_phrase() {
        let text = format!("{}{}data: {{\"type\":\"speech.audio.done\",\"timing\":{{\"ttft_ms\":12.5}}}}\n\ndata: [DONE]\n\n", delta(&[0, 16384]), delta(&[-16384]));
        let mut got = Vec::new();
        let done = read_stream(&mut text.as_bytes(), Duration::from_secs(1), &AtomicBool::new(false), &mut |d| {
            got.extend_from_slice(d);
            Flow::Continue
        })
        .unwrap();
        assert_eq!(got, vec![0.0, 0.5, -0.5]);
        assert_eq!((done.samples, done.ttft_ms), (3, Some(12.5)));
    }

    #[test]
    fn a_stop_an_error_event_and_a_broken_stream_are_errors() {
        let text = format!("{}{}", delta(&[1]), delta(&[2]));
        let mut n = 0;
        let r = read_stream(&mut text.as_bytes(), Duration::from_secs(1), &AtomicBool::new(false), &mut |_| {
            n += 1;
            Flow::Stop
        });
        assert!(matches!(r, Err(AcppError::Cancelled)) && n == 1, "a stop reads no further");
        let err = "data: {\"type\":\"error\",\"error\":{\"message\":\"max_tokens reached before EOC\"}}\n\n";
        match read_stream(&mut err.as_bytes(), Duration::from_secs(1), &AtomicBool::new(false), &mut |_| Flow::Continue) {
            Err(AcppError::Server { message, .. }) => assert_eq!(message, "max_tokens reached before EOC"),
            other => panic!("{other:?}"),
        }
        let cut = delta(&[1]);
        assert!(matches!(read_stream(&mut cut.as_bytes(), Duration::from_secs(1), &AtomicBool::new(false), &mut |_| Flow::Continue), Err(AcppError::Protocol(_))));
    }

    #[test]
    fn the_server_error_body_is_read() {
        match server_error(400, r#"{"error":{"message":"unknown option ras_win_len","type":"invalid_request_error"}}"#) {
            AcppError::Server { status, kind, message } => assert_eq!((status, kind.as_str(), message.as_str()), (400, "invalid_request_error", "unknown option ras_win_len")),
            other => panic!("{other:?}"),
        }
    }
}
