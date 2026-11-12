//! Any other task of a model through `POST /v1/tasks/run` (separation, symbolic transcription): the request is the
//! model's own JSON, the answer is the server's result, and its audio outputs and artifacts can be read out of it.
//! A model that streams (MuScriptor, under either mode of its entry) also runs through `POST /v1/tasks/stream` and
//! sends what it has made so far as it works.

use crate::speech::{json_or_error, server_error};
use crate::sse::{next_event, Event};
use crate::{AcppError, Server};
use base64::Engine;
use serde_json::{json, Map, Value};
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
    /// the model `model`; the answer is the server's result. `cancel` set ends the wait at once with
    /// [`AcppError::Cancelled`]; the server still finishes a run it began (audio.cpp: a JSON client that disconnects
    /// leaves its run to finish), so a studio that needs the model or the card free at once stops the server.
    pub fn run_task(&self, model: &str, request: Value, cancel: &AtomicBool) -> Result<Value, AcppError> {
        run_task_at(&self.http, &self.base, model, request, cancel)
    }
}

fn run_task_at(http: &reqwest::blocking::Client, base: &str, model: &str, request: Value, cancel: &AtomicBool) -> Result<Value, AcppError> {
    let call = http.post(format!("{base}/v1/tasks/run")).json(&json!({ "model": model, "request": request }));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let answer = call.send().map_err(|e| AcppError::Io(format!("audiocpp_server task: {e}"))).and_then(json_or_error);
        let _ = tx.send(answer);
    });
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(AcppError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(answer) => return answer,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err(AcppError::Io("audiocpp_server task: the request ended without an answer".into())),
        }
    }
}


impl Server {
    /// Runs `request` on a model that streams through `POST /v1/tasks/stream` as server-sent events; MuScriptor does
    /// under either mode of its entry, a model that does not is refused by the server. The artifacts of each event go to `on_event` as the model produces them (MuScriptor: one note event or
    /// its progress `{"type":"progress","completed","total"}`, JSON with `meta.event_type`); the answer is the result
    /// [`Server::run_task`] gives for the same request. `cancel` set ends the wait at once with
    /// [`AcppError::Cancelled`], and the connection closes at the server's next event.
    pub fn run_task_stream(&self, model: &str, request: Value, cancel: &AtomicBool, on_event: &mut dyn FnMut(&[TaskArtifact])) -> Result<Value, AcppError> {
        run_task_stream_at(&self.http, &self.base, model, request, cancel, on_event)
    }
}

enum Streamed {
    Event(Vec<TaskArtifact>),
    End(Result<Value, AcppError>),
}

fn run_task_stream_at(
    http: &reqwest::blocking::Client,
    base: &str,
    model: &str,
    request: Value,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(&[TaskArtifact]),
) -> Result<Value, AcppError> {
    let call = http
        .post(format!("{base}/v1/tasks/stream"))
        .header("Accept", "text/event-stream")
        .json(&json!({ "model": model, "stream_format": "sse", "include_artifacts": true, "request": request }));
    let (tx, rx) = std::sync::mpsc::channel();
    let left = Arc::new(AtomicBool::new(false));
    let reader_left = left.clone();
    std::thread::spawn(move || {
        let outcome = call.send().map_err(|e| AcppError::Io(format!("audiocpp_server task: {e}"))).and_then(|answer| {
            let status = answer.status();
            if !status.is_success() {
                return Err(server_error(status.as_u16(), &answer.text().unwrap_or_default()));
            }
            read_task_stream(&mut BufReader::with_capacity(1 << 16, answer), &reader_left, &mut |artifacts| {
                let _ = tx.send(Streamed::Event(artifacts));
            })
        });
        let _ = tx.send(Streamed::End(outcome));
    });
    loop {
        if cancel.load(Ordering::SeqCst) {
            left.store(true, Ordering::SeqCst);
            return Err(AcppError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Streamed::Event(artifacts)) => on_event(&artifacts),
            Ok(Streamed::End(outcome)) => return outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err(AcppError::Io("audiocpp_server task: the stream ended without an answer".into())),
        }
    }
}

/// Feeds the artifacts of each `task.stream.event` to `on_event` until `[DONE]`; the answer is the result of
/// `task.stream.done`. `left` set (the caller stopped waiting) ends the reading at the next event.
fn read_task_stream(reader: &mut impl std::io::BufRead, left: &AtomicBool, on_event: &mut dyn FnMut(Vec<TaskArtifact>)) -> Result<Value, AcppError> {
    let mut result = None;
    loop {
        if left.load(Ordering::SeqCst) {
            return Err(AcppError::Cancelled);
        }
        let data = match next_event(reader).map_err(|e| AcppError::Io(format!("audiocpp_server stream: {e}")))? {
            Some(Event::Data(data)) => data,
            Some(Event::Done) => return result.ok_or_else(|| AcppError::Protocol("the stream ended before task.stream.done".into())),
            None => return Err(AcppError::Protocol("the stream broke off before [DONE]".into())),
        };
        let v: Value = serde_json::from_str(&data).map_err(|e| AcppError::Protocol(format!("an event is not JSON ({e}): {}", data.chars().take(300).collect::<String>())))?;
        match v.get("type").and_then(Value::as_str) {
            Some("task.stream.event") => {
                let artifacts = v.get("event").map(task_artifacts).transpose()?.unwrap_or_default();
                if !artifacts.is_empty() {
                    on_event(artifacts);
                }
            }
            Some("task.stream.done") => {
                result = Some(v.get("result").cloned().ok_or_else(|| AcppError::Protocol("task.stream.done without a result".into()))?);
            }
            Some("error") => {
                let message = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("no message").to_string();
                return Err(AcppError::Server { status: 200, kind: "stream_error".into(), message });
            }
            _ => {}
        }
    }
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
        let result = run_task_at(&http, &base, "muscriptor", json!({ "audio": "C:/a.wav", "options": { "output_format": "midi" } }), &AtomicBool::new(false)).unwrap();
        let got = served.join().unwrap();
        assert!(got.starts_with("POST /v1/tasks/run "), "{got}");
        assert!(got.contains(r#""model":"muscriptor""#) && got.contains(r#""request":{"#) && got.contains(r#""output_format":"midi""#), "{got}");
        assert_eq!(task_artifact(&result, "midi").unwrap().unwrap().payload, b"MThd");

        let (base, served) = stand_in(400, r#"{"error":{"message":"artifacts[0] (x): unknown kind 'tokens'","type":"invalid_request_error"}}"#);
        let err = run_task_at(&http, &base, "htdemucs", json!({ "audio": "C:/a.wav" }), &AtomicBool::new(false)).unwrap_err();
        served.join().unwrap();
        assert!(matches!(err, AcppError::Server { status: 400, ref kind, ref message } if kind == "invalid_request_error" && message.contains("unknown kind")), "{err:?}");
    }

    #[test]
    fn a_stop_ends_the_wait_for_a_long_task_at_once() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let held = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(3));
            drop(stream);
        });
        let http = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let stopper = {
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                cancel.store(true, Ordering::SeqCst);
            })
        };
        let t0 = std::time::Instant::now();
        let err = run_task_at(&http, &base, "htdemucs", json!({ "audio": "C:/long.wav" }), &cancel).unwrap_err();
        assert!(matches!(err, AcppError::Cancelled), "{err:?}");
        assert!(t0.elapsed() < Duration::from_secs(2), "the stop waited for the server: {:?}", t0.elapsed());
        stopper.join().unwrap();
        held.join().unwrap();
    }


    /// The events as audio.cpp 31cb1a4a writes them for MuScriptor: a note, an event without artifacts, the progress
    /// of the first chunk, the result.
    const STREAM: &str = concat!(
        "data: {\"type\":\"task.stream.event\",\"event\":{\"artifacts\":[{\"id\":\"event-0\",\"kind\":\"custom\",\"payload\":\"eyJ0eXBlIjoic3RhcnQiLCJwaXRjaCI6NjUsInN0YXJ0X3RpbWUiOjAuMzIwMDAwMDAwMDAwMDAwMDEsImluZGV4IjowLCJpbnN0cnVtZW50IjoiYWNvdXN0aWNfcGlhbm8ifQ==\",\"meta\":{\"mime\":\"application/json\",\"extension\":\"json\",\"format\":\"muscriptor-event-json\",\"event_type\":\"start\"}}],\"is_final\":false}}\n\n",
        "data: {\"type\":\"task.stream.event\",\"event\":{\"is_final\":false}}\n\n",
        "data: {\"type\":\"task.stream.event\",\"event\":{\"artifacts\":[{\"id\":\"progress-1\",\"kind\":\"custom\",\"payload\":\"eyJ0eXBlIjoicHJvZ3Jlc3MiLCJjb21wbGV0ZWQiOjEsInRvdGFsIjo2fQ==\",\"meta\":{\"format\":\"muscriptor-event-json\",\"event_type\":\"progress\"}}],\"is_final\":false}}\n\n",
        "data: {\"type\":\"task.stream.done\",\"result\":{\"text\":\"[]\",\"artifacts\":[{\"id\":\"result\",\"kind\":\"midi\",\"payload\":\"TVRoZA==\",\"meta\":{}}]}}\n\n",
        "data: [DONE]\n\n",
    );

    #[test]
    fn a_streamed_task_gives_each_event_as_it_comes_and_then_the_result() {
        let http = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let (base, served) = stand_in(200, STREAM);
        let mut seen: Vec<(String, String, String)> = Vec::new();
        let result = run_task_stream_at(&http, &base, "muscriptor", json!({ "audio": "C:/a.wav" }), &AtomicBool::new(false), &mut |artifacts| {
            for artifact in artifacts {
                seen.push((artifact.id.clone(), artifact.meta["event_type"].as_str().unwrap().to_string(), String::from_utf8(artifact.payload.clone()).unwrap()));
            }
        })
        .unwrap();
        let got = served.join().unwrap();
        assert!(got.starts_with("POST /v1/tasks/stream "), "{got}");
        assert!(got.contains(r#""stream_format":"sse""#) && got.contains(r#""include_artifacts":true"#) && got.contains(r#""request":{"audio":"C:/a.wav"}"#), "{got}");
        assert_eq!(seen.len(), 2, "an event without artifacts is not handed on: {seen:?}");
        assert_eq!((seen[0].0.as_str(), seen[0].1.as_str()), ("event-0", "start"));
        assert!(seen[0].2.contains(r#""pitch":65"#), "{}", seen[0].2);
        assert_eq!(seen[1], ("progress-1".to_string(), "progress".to_string(), r#"{"type":"progress","completed":1,"total":6}"#.to_string()));
        assert_eq!(task_artifact(&result, "midi").unwrap().unwrap().payload, b"MThd");
    }

    #[test]
    fn a_stream_that_fails_or_breaks_off_is_an_error() {
        let read = |text: &str| read_task_stream(&mut std::io::BufReader::new(text.as_bytes()), &AtomicBool::new(false), &mut |_| {});
        let failed = read("data: {\"type\":\"error\",\"error\":{\"message\":\"configured model does not provide streaming execution: m\"}}\n\n");
        assert!(matches!(failed, Err(AcppError::Server { ref kind, ref message, .. }) if kind == "stream_error" && message.contains("streaming execution")), "{failed:?}");
        let cut = read("data: {\"type\":\"task.stream.event\",\"event\":{\"is_final\":false}}\n\n");
        assert!(matches!(cut, Err(AcppError::Protocol(ref m)) if m.contains("broke off")), "{cut:?}");
        let no_result = read("data: [DONE]\n\n");
        assert!(matches!(no_result, Err(AcppError::Protocol(ref m)) if m.contains("task.stream.done")), "{no_result:?}");
        let left = read_task_stream(&mut std::io::BufReader::new(STREAM.as_bytes()), &AtomicBool::new(true), &mut |_| panic!("no event after the caller left"));
        assert!(matches!(left, Err(AcppError::Cancelled)), "{left:?}");

        let http = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let (base, served) = stand_in(404, r#"{"error":{"message":"unknown model: m","type":"not_found"}}"#);
        let refused = run_task_stream_at(&http, &base, "m", json!({}), &AtomicBool::new(false), &mut |_| {});
        served.join().unwrap();
        assert!(matches!(refused, Err(AcppError::Server { status: 404, ref kind, .. }) if kind == "not_found"), "{refused:?}");
    }

    #[test]
    fn a_stop_ends_the_wait_for_a_streamed_task_at_once() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let held = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(3));
            drop(stream);
        });
        let http = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let stopper = {
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                cancel.store(true, Ordering::SeqCst);
            })
        };
        let t0 = std::time::Instant::now();
        let err = run_task_stream_at(&http, &base, "muscriptor", json!({ "audio": "C:/long.wav" }), &cancel, &mut |_| {}).unwrap_err();
        assert!(matches!(err, AcppError::Cancelled), "{err:?}");
        assert!(t0.elapsed() < Duration::from_secs(2), "the stop waited for the server: {:?}", t0.elapsed());
        stopper.join().unwrap();
        held.join().unwrap();
    }

    #[test]
    fn a_broken_payload_is_an_error_naming_it() {
        let bad = json!({ "artifacts": [{ "id": "x", "kind": "midi", "payload": "not base64!" }] });
        assert!(matches!(task_artifacts(&bad), Err(AcppError::Protocol(m)) if m.contains("artifacts[0].payload")));
        let no_wav = json!({ "audio": base64::engine::general_purpose::STANDARD.encode(b"RIFF....") });
        assert!(matches!(task_audio(&no_wav), Err(AcppError::Protocol(m)) if m.contains("audio")));
    }
}
