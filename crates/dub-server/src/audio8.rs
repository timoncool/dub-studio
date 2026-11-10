//! Audio8 TTS (arktts DualAR) on the audio.cpp server, as Speech Studio voices with it: a compact multilingual voice
//! that clones zero-shot from a reference and its transcript, at 44.1 kHz mono. The server reads the standalone GGUF
//! (`Audio8-TTS-Realistic-Air-Q8_0.gguf`) in place, on the build of the voicing device (`tts_backend`); dropping the
//! model frees its memory there.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use acpp_server::{Backend, Flow, Server, SpeechRequest, VoiceRef};
use serde_json::{Map, Value};

/// The folder of the weights inside the models root.
pub const DIR: &str = "audio8";
/// The single GGUF the server reads (the Realistic-Air package, q8_0).
pub const GGUF: &str = "Audio8-TTS-Realistic-Air-Q8_0.gguf";

/// The model ends every phrase with one codec frame (2048 samples at 44.1 kHz) that is not speech: after the voice
/// has died away it holds a burst, heard as a click at the end of the take.
const TERMINAL_FRAME: usize = 2048;
const CODEC_RATE: usize = 44_100;

/// The sampling of the model card (Edge0/Audio8-TTS-Realistic-Air).
pub const TEMPERATURE: f64 = 0.8;
pub const TOP_K: u32 = 50;
pub const TOP_P: f64 = 0.95;

/// How many of `len` samples at `rate` are the phrase itself, without the terminal frame.
fn spoken(len: usize, rate: u32) -> usize {
    len.saturating_sub(TERMINAL_FRAME * rate as usize / CODEC_RATE)
}

/// The longest wait for the next piece of a phrase; the first one also waits for the model to load.
const SPEECH_STALL: Duration = Duration::from_secs(120);

pub fn dir(models_root: &Path) -> PathBuf {
    models_root.join(DIR)
}

pub fn gguf(models_root: &Path) -> PathBuf {
    dir(models_root).join(GGUF)
}

pub fn installed(models_root: &Path) -> bool {
    gguf(models_root).is_file()
}

/// Audio8 TTS on the audio.cpp server; dropping it frees its memory there.
pub struct Model {
    server: Arc<Server>,
    rate: u32,
    cancel: AtomicBool,
}

impl Model {
    /// On the build of the voicing device chosen in the settings (`tts_backend`; «auto» — see [`crate::acpp::build_for`]).
    pub fn load(repo_root: &Path, models_root: &Path) -> Result<Model, String> {
        let gguf = gguf(models_root);
        if !gguf.is_file() {
            return Err(t!("render-audio8-model-missing", path = gguf.display().to_string()));
        }
        let build = crate::acpp::build_for(crate::models::server_choice(models_root, "tts_backend"))?;
        let server = crate::acpp::server(repo_root, models_root, build, crate::acpp::AUDIO8)?;
        let rate = server.speech_rate(crate::acpp::AUDIO8).map_err(|e| crate::acpp::localize(&e))?;
        Ok(Model { server, rate, cancel: AtomicBool::new(false) })
    }

    pub fn backend(&self) -> Backend {
        self.server.config().backend
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Stops the phrase being spoken at its next piece.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// One phrase, the whole of it in the answer. With a reference WAV and its transcript the model clones that voice,
    /// without one it speaks in its own. The model finds the language itself; a `lang` set by hand is its hint (it
    /// tells Cantonese from Mandarin by it), «auto» gives none.
    pub fn speak(&self, text: &str, lang: &str, reference: Option<&Path>, reference_text: Option<&str>, options: Map<String, Value>) -> Result<Vec<f32>, String> {
        self.cancel.store(false, Ordering::SeqCst);
        let req = SpeechRequest {
            model: crate::acpp::AUDIO8.to_string(),
            input: text.to_string(),
            voice_ref: reference.map(|p| VoiceRef::Path(p.to_path_buf())),
            reference_text: reference_text.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string),
            language: Some(lang).filter(|l| !l.is_empty() && *l != "auto").map(str::to_string),
            options,
            stall: SPEECH_STALL,
        };
        let mut all: Vec<f32> = Vec::new();
        self.server
            .speech_stream(&req, &self.cancel, &mut |d| {
                all.extend_from_slice(d);
                Flow::Continue
            })
            .map_err(|e| match e {
                acpp_server::AcppError::Cancelled => crate::jobs::CANCELLED.to_string(),
                other => crate::acpp::localize(&other),
            })?;
        all.truncate(spoken(all.len(), self.rate));
        Ok(all)
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        crate::acpp::unload(&[crate::acpp::AUDIO8]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gguf_lives_under_the_audio8_folder() {
        let root = Path::new("models_root");
        assert_eq!(dir(root), root.join("audio8"));
        assert_eq!(gguf(root), root.join("audio8").join("Audio8-TTS-Realistic-Air-Q8_0.gguf"));
        assert!(!installed(root), "a bogus path holds no model file");
    }

    #[test]
    fn a_phrase_is_given_without_its_terminal_frame() {
        assert_eq!(spoken(2048 * 10, 44_100), 2048 * 9);
        assert_eq!(spoken(1024 * 10, 22_050), 1024 * 9, "the frame is counted at the rate the server answers in");
        assert_eq!(spoken(1000, 44_100), 0, "less than a frame holds no speech");
    }

    #[test]
    fn the_server_entry_is_the_audio8_tts_family_cloning_at_44k() {
        let m = acpp_server::ModelEntry::audio8_tts(crate::acpp::AUDIO8, Path::new("x.gguf"));
        assert_eq!(m.id, crate::acpp::AUDIO8);
        assert_eq!(m.family, "audio8_tts");
        assert_eq!(m.task, "tts", "a reference clones its voice, none speaks the model's own");
        assert_eq!(m.output_rate, Some(44_100), "the neural codec renders 44.1 kHz");
    }
}
