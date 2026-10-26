//! Any other task of a model through `POST /v1/tasks/run` (separation, symbolic transcription): the request is the
//! model's own JSON, the answer is the server's result, and its audio outputs and artifacts can be read out of it.

use crate::speech::json_or_error;
use crate::{AcppError, Server};
use base64::Engine;
use serde_json::{json, Map, Value};

/// An artifact of a result: opaque bytes the model returns (a MIDI file, a score, tokens), with its kind as the
/// server names it (`midi`, `custom`, …) and its string metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskArtifact {
    pub id: String,
    pub kind: String,
    pub payload: Vec<u8>,
    pub meta: Map<String, Value>,
}

/// An audio output of a result, interleaved samples in -1..1: the result's `audio` (id None) and each of its
/// `named_audio_outputs` (a separated stem: `vocals`, `drums`, …).
#[derive(Clone, Debug, PartialEq)]
pub struct TaskAudio {
    pub id: Option<String>,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Server {
    /// Runs `request` (the model's request fields: `audio` as a path on this computer, `options`, `artifacts`, …) on
    /// the model `model`; the answer is the server's result.
    pub fn run_task(&self, model: &str, request: Value) -> Result<Value, AcppError> {
        run_task_at(&self.http, &self.base, model, request)
    }
}

fn run_task_at(http: &reqwest::blocking::Client, base: &str, model: &str, request: Value) -> Result<Value, AcppError> {
    let resp = http
        .post(format!("{base}/v1/tasks/run"))
        .json(&json!({ "model": model, "request": request }))
        .send()
        .map_err(|e| AcppError::Io(format!("audiocpp_server task: {e}")))?;
    json_or_error(resp)
}

fn b64(field: &str, text: &str) -> Result<Vec<u8>, AcppError> {
    base64::engine::general_purpose::STANDARD.decode(text).map_err(|e| AcppError::Protocol(format!("{field} is not base64: {e}")))
}

/// The artifacts of a result, in the order the server gives them; none for a result without any.
pub fn task_artifacts(result: &Value) -> Result<Vec<TaskArtifact>, AcppError> {
    let Some(items) = result.get("artifacts").and_then(Value::as_array) else { return Ok(Vec::new()) };
    items
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let text = |k: &str| a.get(k).and_then(Value::as_str).ok_or_else(|| AcppError::Protocol(format!("artifacts[{i}] has no {k}")));
            Ok(TaskArtifact {
                id: text("id")?.to_string(),
                kind: text("kind")?.to_string(),
                payload: b64(&format!("artifacts[{i}].payload"), text("payload")?)?,
                meta: a.get("meta").and_then(Value::as_object).cloned().unwrap_or_default(),
            })
        })
        .collect()
}

/// The first artifact of `kind` (a MIDI file is `midi`), if the result has one.
pub fn task_artifact(result: &Value, kind: &str) -> Result<Option<TaskArtifact>, AcppError> {
    Ok(task_artifacts(result)?.into_iter().find(|a| a.kind == kind))
}

fn wav_samples(field: &str, b64_wav: &str) -> Result<(Vec<f32>, u32, u16), AcppError> {
    let bytes = b64(field, b64_wav)?;
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|e| AcppError::Protocol(format!("{field} is not a WAV: {e}")))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 / max)).collect::<Result<_, _>>()
        }
    }
    .map_err(|e| AcppError::Protocol(format!("{field}: {e}")))?;
    Ok((samples, spec.sample_rate, spec.channels))
}

/// The audio outputs of a result: its `audio`, then its `named_audio_outputs`; none for a result without audio.
pub fn task_audio(result: &Value) -> Result<Vec<TaskAudio>, AcppError> {
    let mut out = Vec::new();
    if let Some(wav) = result.get("audio").and_then(Value::as_str) {
        let (samples, sample_rate, channels) = wav_samples("audio", wav)?;
        out.push(TaskAudio { id: None, samples, sample_rate, channels });
    }
    for (i, named) in result.get("named_audio_outputs").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let id = named.get("id").and_then(Value::as_str).ok_or_else(|| AcppError::Protocol(format!("named_audio_outputs[{i}] has no id")))?;
        let wav = named.get("audio").and_then(Value::as_str).ok_or_else(|| AcppError::Protocol(format!("named_audio_outputs[{i}] has no audio")))?;
        let (samples, sample_rate, channels) = wav_samples(&format!("named_audio_outputs[{i}].audio"), wav)?;
        out.push(TaskAudio { id: Some(id.to_string()), samples, sample_rate, channels });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_b64(samples: &[i16], rate: u32, channels: u16) -> String {
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            for s in samples {
                w.write_sample(*s).unwrap();
            }
            w.finalize().unwrap();
        }
        base64::engine::general_purpose::STANDARD.encode(cur.into_inner())
    }

    #[test]
    fn stems_and_a_midi_artifact_are_read_out_of_a_result() {
        let midi = b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x01\xe0";
        let result = json!({
            "named_audio_outputs": [
                { "id": "vocals", "audio": wav_b64(&[16384, -16384], 44_100, 2), "sample_rate": 44_100, "channels": 2 },
                { "id": "drums", "audio": wav_b64(&[0, 0], 44_100, 2), "sample_rate": 44_100, "channels": 2 }
            ],
            "artifacts": [
                { "id": "muscriptor.notes", "kind": "custom", "payload": base64::engine::general_purpose::STANDARD.encode(b"{}"), "meta": {} },
                { "id": "muscriptor.midi", "kind": "midi", "payload": base64::engine::general_purpose::STANDARD.encode(midi), "meta": { "format": "smf" } }
            ],
            "timing": { "wall_ms": 1.0 }
        });
        let audio = task_audio(&result).unwrap();
        assert_eq!(audio.iter().map(|a| a.id.as_deref()).collect::<Vec<_>>(), [Some("vocals"), Some("drums")]);
        assert_eq!((audio[0].sample_rate, audio[0].channels), (44_100, 2));
        assert_eq!(audio[0].samples, vec![0.5, -0.5]);
        let found = task_artifact(&result, "midi").unwrap().unwrap();
        assert_eq!(found.payload, midi);
        assert_eq!(found.meta["format"], "smf");
        assert_eq!(task_artifacts(&result).unwrap().len(), 2);
        assert!(task_audio(&json!({ "text": "" })).unwrap().is_empty());
        assert!(task_artifact(&json!({ "text": "" }), "midi").unwrap().is_none());
    }

    /// A server on a local port answering one request with `status` and `body`; the request it got comes back.
    fn stand_in(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let served = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut got = vec![0u8; length];
            reader.read_exact(&mut got).unwrap();
            let answer = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(answer.as_bytes()).unwrap();
            format!("{}{}", head.lines().next().unwrap_or_default(), String::from_utf8(got).unwrap())
        });
        (base, served)
    }

    #[test]
    fn a_task_goes_to_tasks_run_and_its_result_or_error_comes_back() {
        let http = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let (base, served) = stand_in(200, r#"{"artifacts":[{"id":"m","kind":"midi","payload":"TVRoZA==","meta":{}}],"timing":{"wall_ms":2.0}}"#);
        let result = run_task_at(&http, &base, "muscriptor", json!({ "audio": "C:/a.wav", "options": { "output_format": "midi" } })).unwrap();
        let got = served.join().unwrap();
        assert!(got.starts_with("POST /v1/tasks/run "), "{got}");
        assert!(got.contains(r#""model":"muscriptor""#) && got.contains(r#""request":{"#) && got.contains(r#""output_format":"midi""#), "{got}");
        assert_eq!(task_artifact(&result, "midi").unwrap().unwrap().payload, b"MThd");

        let (base, served) = stand_in(400, r#"{"error":{"message":"artifacts[0] (x): unknown kind 'tokens'","type":"invalid_request_error"}}"#);
        let err = run_task_at(&http, &base, "htdemucs", json!({ "audio": "C:/a.wav" })).unwrap_err();
        served.join().unwrap();
        assert!(matches!(err, AcppError::Server { status: 400, ref kind, ref message } if kind == "invalid_request_error" && message.contains("unknown kind")), "{err:?}");
    }

    #[test]
    fn a_broken_payload_is_an_error_naming_it() {
        let bad = json!({ "artifacts": [{ "id": "x", "kind": "midi", "payload": "not base64!" }] });
        assert!(matches!(task_artifacts(&bad), Err(AcppError::Protocol(m)) if m.contains("artifacts[0].payload")));
        let no_wav = json!({ "audio": base64::engine::general_purpose::STANDARD.encode(b"RIFF....") });
        assert!(matches!(task_audio(&no_wav), Err(AcppError::Protocol(m)) if m.contains("audio")));
    }
}
