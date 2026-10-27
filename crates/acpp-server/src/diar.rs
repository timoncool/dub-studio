//! Diarization through `POST /v1/audio/transcriptions/details`: a diarization model answers no text, only
//! `speaker_turns` in samples, which are turned into seconds by the answer's rate.

use crate::asr::check_16k_mono;
use crate::speech::json_or_error;
use crate::{AcppError, Server};
use serde_json::{json, Value};
use std::path::Path;

/// One speaker's stretch of the recording; speakers are numbered in the order they first speak.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeakerTurn {
    pub start: f64,
    pub end: f64,
    pub speaker: u32,
}

fn speaker_number(id: &str) -> Result<u32, AcppError> {
    let digits = id.trim_start_matches(|c: char| !c.is_ascii_digit());
    digits.parse().map_err(|_| AcppError::Protocol(format!("a speaker turn has the speaker id {id:?}")))
}

pub(crate) fn parse_turns(v: &Value) -> Result<Vec<SpeakerTurn>, AcppError> {
    let Some(items) = v.get("speaker_turns").and_then(Value::as_array) else { return Ok(Vec::new()) };
    let rate = v
        .get("sample_rate")
        .and_then(Value::as_f64)
        .filter(|r| *r > 0.0)
        .ok_or_else(|| AcppError::Protocol("the answer has speaker_turns but no sample_rate".into()))?;
    items
        .iter()
        .map(|t| {
            let at = |k: &str| t.get(k).and_then(Value::as_f64).ok_or_else(|| AcppError::Protocol(format!("speaker_turns: an item without {k}")));
            let id = t.get("speaker_id").and_then(Value::as_str).ok_or_else(|| AcppError::Protocol("speaker_turns: an item without speaker_id".into()))?;
            let (start, end) = (at("start_sample")? / rate, at("end_sample")? / rate);
            Ok(SpeakerTurn { start, end: end.max(start), speaker: speaker_number(id)? })
        })
        .collect()
}

impl Server {
    /// The speaker turns of a 16 kHz mono WAV on this computer.
    pub fn diarize(&self, model: &str, wav: &Path) -> Result<Vec<SpeakerTurn>, AcppError> {
        check_16k_mono(wav)?;
        let body = json!({ "model": model, "audio": wav.to_string_lossy() });
        let resp = self
            .http
            .post(format!("{}/v1/audio/transcriptions/details", self.base))
            .json(&body)
            .send()
            .map_err(|e| AcppError::Io(format!("audiocpp_server diarization: {e}")))?;
        parse_turns(&json_or_error(resp)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_in_samples_become_seconds_with_numbered_speakers() {
        let v: Value = serde_json::from_str(
            r#"{"text":"","speaker_turns":[{"start_sample":160,"end_sample":179840,"speaker_id":"speaker_0","confidence":0.99},
                {"start_sample":203200,"end_sample":219040,"speaker_id":"speaker_1","confidence":0.98}],"sample_rate":16000,"timing":{"wall_ms":1.0}}"#,
        )
        .unwrap();
        let turns = parse_turns(&v).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0], SpeakerTurn { start: 0.01, end: 11.24, speaker: 0 });
        assert_eq!(turns[1].speaker, 1);
    }

    #[test]
    fn silence_is_no_turns_and_a_turn_without_a_rate_is_refused() {
        let silent: Value = serde_json::from_str(r#"{"text":"","timing":{"wall_ms":1.0}}"#).unwrap();
        assert!(parse_turns(&silent).unwrap().is_empty());
        let no_rate: Value = serde_json::from_str(r#"{"text":"","speaker_turns":[{"start_sample":1,"end_sample":2,"speaker_id":"speaker_0"}]}"#).unwrap();
        assert!(matches!(parse_turns(&no_rate), Err(AcppError::Protocol(_))));
    }
}
