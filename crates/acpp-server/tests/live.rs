//! A real server on the graphics card. Run by hand, one at a time, with the paths in the environment:
//! ACPP_EXE (audiocpp_server), ACPP_DATA (a folder for its config and log), ACPP_HIGGS (q8_0.gguf), ACPP_PARAKEET
//! (parakeet-tdt-0.6b-v3-Q8_0.gguf), ACPP_CRISPER (the CrisperWhisper folder), ACPP_REF (a reference WAV and
//! ACPP_REF_TEXT its words), ACPP_SPEECH (a 16 kHz mono WAV of speech, ACPP_LANG its Whisper language code).

use acpp_server::{Backend, Flow, ModelEntry, Server, ServerConfig, SpeechRequest, TranscribeRequest, VoiceRef};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn env(k: &str) -> PathBuf {
    PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("{k} is not set")))
}

fn server() -> Server {
    let config = ServerConfig {
        exe: env("ACPP_EXE"),
        backend: Backend::Cuda,
        device: 0,
        threads: std::env::var("ACPP_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(8)),
        models: vec![
            ModelEntry::higgs_tts("higgs", &env("ACPP_HIGGS")),
            ModelEntry::parakeet_tdt("parakeet", &env("ACPP_PARAKEET")),
            ModelEntry::crisperwhisper("crisperwhisper", &env("ACPP_CRISPER")),
        ],
        data_dir: env("ACPP_DATA"),
        log: std::env::var("ACPP_LOG").is_ok_and(|v| v == "1"),
    };
    Server::start(&config, Duration::from_secs(120), &|_| {}).unwrap()
}

#[test]
#[ignore]
fn higgs_streams_its_phrase_and_a_stop_ends_it() {
    let s = server();
    let req = SpeechRequest {
        model: "higgs".into(),
        input: "Hello. This phrase is spoken by Higgs Audio through the audio.cpp server, one piece at a time.".into(),
        voice_ref: Some(VoiceRef::Path(env("ACPP_REF"))),
        reference_text: std::env::var("ACPP_REF_TEXT").ok(),
        language: None,
        options: serde_json::Map::new(),
        stall: Duration::from_secs(120),
    };
    let t0 = Instant::now();
    let mut first = None;
    let mut deltas = 0;
    let done = s
        .speech_stream(&req, &AtomicBool::new(false), &mut |_| {
            first.get_or_insert(t0.elapsed());
            deltas += 1;
            Flow::Continue
        })
        .unwrap();
    let rate = s.speech_rate("higgs").unwrap();
    let secs = done.samples as f64 / rate as f64;
    eprintln!("[OK] {deltas} deltas, {secs:.2} s of audio in {:.2} s, first delta after {:?}", t0.elapsed().as_secs_f64(), first);
    assert!(deltas > 1 && secs > 2.0);
    let mut seen = 0;
    let stopped = s.speech_stream(&req, &AtomicBool::new(false), &mut |_| {
        seen += 1;
        if seen == 2 { Flow::Stop } else { Flow::Continue }
    });
    assert!(matches!(stopped, Err(acpp_server::AcppError::Cancelled)));
    let after = s.speech(&req, &AtomicBool::new(false)).unwrap();
    assert!(after.samples.len() as f64 / after.sample_rate as f64 > 2.0, "the server speaks again after a stop");
    s.unload(&["higgs"]).unwrap();
}

#[test]
#[ignore]
fn parakeet_and_crisperwhisper_give_words_in_seconds() {
    let s = server();
    for (model, language) in [("parakeet", None), ("crisperwhisper", std::env::var("ACPP_LANG").ok())] {
        let t0 = Instant::now();
        let t = s
            .transcribe_details(&TranscribeRequest {
                model: model.into(),
                wav: env("ACPP_SPEECH"),
                language,
                prompt: None,
                options: serde_json::Map::new(),
                within: None,
            })
            .unwrap();
        let last = t.words.last().map(|w| w.end).unwrap_or(0.0);
        eprintln!("[OK] {model}: {} words, the last ends at {last:.2} s, {:.2} s; {}", t.words.len(), t0.elapsed().as_secs_f64(), t.text.chars().take(160).collect::<String>());
        assert!(!t.words.is_empty() && t.words.windows(2).all(|w| w[0].start <= w[1].start));
        s.unload(&[model]).unwrap();
    }
}

/// A stop in the middle of a long phrase frees the model at once: the next phrase does not wait for the stopped one.
#[test]
#[ignore]
fn a_stopped_long_phrase_frees_the_model() {
    let s = server();
    let phrase = |input: &str| SpeechRequest {
        model: "higgs".into(),
        input: input.into(),
        voice_ref: Some(VoiceRef::Path(env("ACPP_REF"))),
        reference_text: std::env::var("ACPP_REF_TEXT").ok(),
        language: None,
        options: serde_json::Map::new(),
        stall: Duration::from_secs(120),
    };
    s.speech(&phrase("Warm up."), &AtomicBool::new(false)).unwrap();
    let long = "This is a long phrase. ".repeat(16);
    let full = Instant::now();
    let whole = s.speech(&phrase(&long), &AtomicBool::new(false)).unwrap();
    let whole_secs = full.elapsed().as_secs_f64();
    let mut seen = 0;
    let t0 = Instant::now();
    let r = s.speech_stream(&phrase(&long), &AtomicBool::new(false), &mut |_| {
        seen += 1;
        if seen == 3 { Flow::Stop } else { Flow::Continue }
    });
    let stopped_at = t0.elapsed().as_secs_f64();
    assert!(matches!(r, Err(acpp_server::AcppError::Cancelled)));
    let t1 = Instant::now();
    let mut first = None;
    s.speech_stream(&phrase("The next phrase."), &AtomicBool::new(false), &mut |_| {
        first.get_or_insert(t1.elapsed().as_secs_f64());
        Flow::Continue
    })
    .unwrap();
    let first = first.unwrap();
    eprintln!(
        "[OK] the long phrase: {:.1} s of audio in {whole_secs:.1} s; stopped after {stopped_at:.1} s; the next phrase's first audio after {first:.2} s",
        whole.samples.len() as f64 / whole.sample_rate as f64
    );
    assert!(first < (whole_secs - stopped_at) / 2.0, "the next phrase waited for the stopped one");
}

/// One Higgs entry (route `clon`) also speaks without a reference: the engine's own voice, no second model in memory.
#[test]
#[ignore]
fn higgs_speaks_without_a_reference() {
    let s = server();
    let req = SpeechRequest {
        model: "higgs".into(),
        input: "This voice is the engine's own, with no reference recording.".into(),
        voice_ref: None,
        reference_text: None,
        language: None,
        options: serde_json::Map::new(),
        stall: Duration::from_secs(120),
    };
    let sp = s.speech(&req, &AtomicBool::new(false)).unwrap();
    let secs = sp.samples.len() as f64 / sp.sample_rate as f64;
    eprintln!("[OK] {secs:.2} s of audio without a reference");
    assert!(secs > 1.5);
}
