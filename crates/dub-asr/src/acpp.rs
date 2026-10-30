//! Recognition in the studio's audiocpp_server (`acpp_server`): Parakeet-TDT, which finds the language itself,
//! Nemotron 3.5, which is told the chosen one as it is, and CrisperWhisper, which is told it by Whisper code
//! (`acpp_server::language`).

use crate::{segment_words, AsrEngine, AsrError, Segment, SpeakerSegment, Turn, Word, SEG_MAX_DUR, SEG_MAX_GAP};
use acpp_server::{Server, TranscribeRequest};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How the model is told the language.
#[derive(Clone, Debug)]
pub enum Language {
    /// The model finds it (Parakeet).
    Own,
    /// Told the chosen code as written, the model resolving it to its own spelling (Nemotron 3.5: `ru` or `ru-RU`);
    /// none or «auto» — the model finds it.
    Told,
    /// Told by Whisper code; «auto» is heard by VoxLingua107 through transcribe.cpp's `engine_dir` and `model`.
    Whisper { engine_dir: PathBuf, langid: PathBuf },
}

/// A recogniser in the server: `model` is its id there.
pub struct AcppAsr {
    server: Arc<Server>,
    model: String,
    language: Language,
    prompt: Option<String>,
}

impl AcppAsr {
    pub fn new(server: Arc<Server>, model: &str, language: Language) -> AcppAsr {
        AcppAsr { server, model: model.to_string(), language, prompt: None }
    }

    /// Words and spellings to expect in the recording; CrisperWhisper takes them as its context text.
    pub fn with_prompt(mut self, prompt: Option<String>) -> Self {
        self.prompt = prompt.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        self
    }

    /// The language to send: none for a model that finds it; the chosen one; or the one heard in the recording.
    fn language_for(&self, wav: &Path, lang: &str) -> Result<Option<String>, AsrError> {
        let chosen = lang.trim();
        let (engine_dir, langid) = match &self.language {
            Language::Own => return Ok(None),
            Language::Told => return Ok(told(chosen)),
            Language::Whisper { engine_dir, langid } => (engine_dir, langid),
        };
        let err = |e: acpp_server::AcppError| AsrError::Parakeet(e.to_string());
        if !chosen.is_empty() && !chosen.eq_ignore_ascii_case("auto") {
            return acpp_server::language::whisper_code(chosen).map(|c| Some(c.to_string())).map_err(err);
        }
        let (pcm, _) = crate::load_wav_16k_mono_head(wav, Some(acpp_server::language::LANGUAGE_ID_SECS))?;
        let heard = acpp_server::language::heard_language(engine_dir, langid, &pcm).map_err(err)?;
        eprintln!("[asr] language heard: {heard}");
        Ok(Some(heard.to_string()))
    }

    fn words(&self, wav: &Path, lang: &str) -> Result<Vec<Word>, AsrError> {
        let language = self.language_for(wav, lang)?;
        let t = self
            .server
            .transcribe_details(&TranscribeRequest {
                model: self.model.clone(),
                wav: wav.to_path_buf(),
                language,
                prompt: self.prompt.clone(),
                options: serde_json::Map::new(),
                within: None,
            })
            .map_err(|e| AsrError::Parakeet(e.to_string()))?;
        Ok(t.words.into_iter().map(|w| Word { word: w.text, start: w.start, end: w.end }).collect())
    }
}

/// The language VoxLingua107 (`langid` on the transcribe.cpp build in `engine_dir`) hears in a 16 kHz mono recording,
/// as Whisper names it; an error names a language Whisper does not know or a recording too short to tell.
pub fn heard_language(engine_dir: &Path, langid: &Path, wav: &Path) -> Result<String, AsrError> {
    let (pcm, _) = crate::load_wav_16k_mono_head(wav, Some(acpp_server::language::LANGUAGE_ID_SECS))?;
    acpp_server::language::heard_language(engine_dir, langid, &pcm)
        .map(str::to_string)
        .map_err(|e| AsrError::Parakeet(e.to_string()))
}

/// What audio.cpp answers when CrisperWhisper's decoder prompt, the words carried from the window before, reaches the
/// decoder's 448 positions (`whisper_runtime.cpp`). A recording of one window, at most 30 s, carries no words.
const CONTEXT_OVERFLOW: &str = "CrisperWhisper invalid decoder prompt or token limit";

/// Whether recognition failed on CrisperWhisper's carried words outgrowing its decoder.
pub fn context_overflowed(e: &AsrError) -> bool {
    e.to_string().contains(CONTEXT_OVERFLOW)
}

/// A chosen language as the model is told it; none for «auto» or no choice.
fn told(chosen: &str) -> Option<String> {
    let chosen = chosen.trim();
    (!chosen.is_empty() && !chosen.eq_ignore_ascii_case("auto")).then(|| chosen.to_string())
}

/// The turn a word belongs to: the one holding its middle, else the nearest by centre.
fn turn_of(turns: &[Turn], mid: f64) -> usize {
    turns.iter().position(|t| mid >= t.start && mid <= t.end).unwrap_or_else(|| {
        turns
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (mid - (a.start + a.end) / 2.0).abs().total_cmp(&(mid - (b.start + b.end) / 2.0).abs()))
            .map(|(i, _)| i)
            .unwrap_or(0)
    })
}

impl AsrEngine for AcppAsr {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError> {
        Ok(segment_words(&self.words(wav, lang)?, SEG_MAX_GAP, SEG_MAX_DUR))
    }

    /// One pass over the whole recording (the model keeps its context), then each word to its speaker's turn.
    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError> {
        let words = self.words(wav, lang)?;
        if turns.is_empty() {
            return Ok(segment_words(&words, SEG_MAX_GAP, SEG_MAX_DUR)
                .into_iter()
                .map(|s| SpeakerSegment { start: s.start, end: s.end, text: s.text, speaker: 0 })
                .collect());
        }
        let mut buckets: Vec<Vec<Word>> = vec![Vec::new(); turns.len()];
        for w in words {
            buckets[turn_of(turns, (w.start + w.end) / 2.0)].push(w);
        }
        let mut out = Vec::new();
        for (i, ws) in buckets.into_iter().enumerate() {
            for s in segment_words(&ws, SEG_MAX_GAP, SEG_MAX_DUR) {
                out.push(SpeakerSegment { start: s.start, end: s.end, text: s.text, speaker: turns[i].speaker });
            }
        }
        out.sort_by(|a, b| a.start.total_cmp(&b.start));
        Ok(out)
    }

    fn transcribe_many_words(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<Word>, AsrError>> {
        files.iter().map(|f| self.words(f, lang)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_told_language_goes_as_chosen_and_auto_goes_unsaid() {
        assert_eq!(told("ru"), Some("ru".to_string()));
        assert_eq!(told(" th-TH "), Some("th-TH".to_string()));
        assert_eq!(told("auto"), None);
        assert_eq!(told("AUTO"), None);
        assert_eq!(told(""), None);
    }

    #[test]
    fn the_servers_answer_on_outgrown_carried_words_is_told_from_other_failures() {
        let answer = "audiocpp_server answered 500 (server_error): CrisperWhisper invalid decoder prompt or token limit";
        assert!(context_overflowed(&AsrError::Parakeet(answer.to_string())));
        assert!(!context_overflowed(&AsrError::Parakeet("audiocpp_server answered 500 (server_error): CrisperWhisper graph execution failed".into())));
        assert!(!context_overflowed(&AsrError::Io("disk full".into())));
    }

    #[test]
    fn a_word_goes_to_the_turn_holding_its_middle_or_the_nearest() {
        let turns = [Turn { start: 0.0, end: 2.0, speaker: 0 }, Turn { start: 3.0, end: 5.0, speaker: 1 }];
        assert_eq!(turn_of(&turns, 1.0), 0);
        assert_eq!(turn_of(&turns, 4.0), 1);
        assert_eq!(turn_of(&turns, 2.7), 1);
    }
}
