//! Supertonic 3 (Supertone) on the audio.cpp server: four graphs — duration, text encoder, flow-matching vector
//! estimator, vocoder — ten preset voices, 31 languages, no cloning. The server reads the installed
//! askurios8/supertonic-3-int8 folder (weight-only int8 ONNX) as it is, on the build of the voicing device.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use acpp_server::{Backend, Server, SpeechRequest};
use serde_json::{json, Map, Value};

/// The folder of the weights inside the models root.
pub const DIR: &str = "supertonic3";
/// The preset voices, as the voice style files are named.
pub const VOICES: &[&str] = &["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"];
pub const LANGS: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu", "id", "it", "lt", "lv", "nl", "pl",
    "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk", "vi",
];
/// Denoising steps and speed of the official runtime's defaults.
pub const DEFAULT_STEPS: u32 = 8;
pub const DEFAULT_SPEED: f64 = 1.05;
/// Characters the model takes at once (the official chunk length outside Korean and Japanese).
pub const MAX_CHARS: usize = 300;
/// The longest wait for the phrase; the first one also waits for the model to load.
const SPEECH_STALL: Duration = Duration::from_secs(120);

/// Supertonic 3 on the audio.cpp server, which reads the installed folder as it is; dropping it frees its memory there.
pub struct Model {
    server: Arc<Server>,
    rate: u32,
    cancel: AtomicBool,
}

pub fn dir(models_root: &Path) -> PathBuf {
    models_root.join(DIR)
}

pub fn installed(models_root: &Path) -> bool {
    let d = dir(models_root);
    ["duration_predictor", "text_encoder", "vector_estimator", "vocoder"].iter().all(|g| d.join("onnx").join(format!("{g}.onnx")).is_file())
        && VOICES.iter().all(|v| d.join("voice_styles").join(format!("{v}.json")).is_file())
}

/// The model's request options: the preset voice, the flow steps, the speech speed and the noise seed (as a string:
/// the server reads a JSON number above 2^53 inexactly).
fn request_options(voice: &str, steps: u32, speed: f64, seed: u64) -> Map<String, Value> {
    let mut options = Map::new();
    options.insert("voice_id".into(), json!(voice));
    options.insert("num_inference_steps".into(), json!(steps.max(1)));
    options.insert("speed".into(), json!(speed));
    options.insert("seed".into(), json!(seed.to_string()));
    options
}

impl Model {
    /// On the build of the voicing device chosen in the settings (`tts_backend`; «auto» — see `acpp::build_for`).
    pub fn load(repo_root: &Path, models_root: &Path) -> Result<Model, String> {
        let build = crate::acpp::build_for(crate::models::server_choice(models_root, "tts_backend"))?;
        let server = crate::acpp::server(repo_root, models_root, build, crate::acpp::SUPERTONIC)?;
        let rate = server.speech_rate(crate::acpp::SUPERTONIC).map_err(|e| crate::acpp::localize(&e))?;
        Ok(Model { server, rate, cancel: AtomicBool::new(false) })
    }

    pub fn backend(&self) -> Backend {
        self.server.config().backend
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Speaks one phrase (up to `MAX_CHARS`) in a preset voice; the same `seed` gives the same take.
    pub fn speak(&self, text: &str, lang: &str, voice: &str, steps: u32, speed: f64, seed: u64) -> Result<Vec<f32>, String> {
        if !VOICES.contains(&voice) {
            return Err(format!("supertonic: no voice {voice}; the voices are {}", VOICES.join(", ")));
        }
        self.cancel.store(false, Ordering::SeqCst);
        let req = SpeechRequest {
            model: crate::acpp::SUPERTONIC.to_string(),
            input: text.chars().filter(|c| !is_emoji(*c)).collect(),
            voice_ref: None,
            reference_text: None,
            language: Some(if LANGS.contains(&lang) { lang } else { "na" }.to_string()),
            options: request_options(voice, steps, speed, seed),
            stall: SPEECH_STALL,
        };
        self.server.speech(&req, &self.cancel).map(|s| s.samples).map_err(|e| crate::acpp::localize(&e))
    }
}

/// The emoji the official text clean-up drops; the server's clean-up keeps them, and the model has no tokens for them.
fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F600..=0x1F64F | 0x1F300..=0x1F5FF | 0x1F680..=0x1F6FF | 0x1F700..=0x1F77F | 0x1F780..=0x1F7FF | 0x1F800..=0x1F8FF | 0x1F900..=0x1F9FF | 0x1FA00..=0x1FA6F | 0x1FA70..=0x1FAFF | 0x2600..=0x26FF | 0x2700..=0x27BF | 0x1F1E6..=0x1F1FF)
}

impl Drop for Model {
    fn drop(&mut self) {
        crate::acpp::unload(&[crate::acpp::SUPERTONIC]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_names_the_voice_steps_speed_and_seed_as_the_model_reads_them() {
        let options = request_options("F1", 0, 1.05, u64::MAX);
        assert_eq!(options["voice_id"], json!("F1"));
        assert_eq!(options["num_inference_steps"], json!(1));
        assert_eq!(options["speed"], json!(1.05));
        assert_eq!(options["seed"], json!("18446744073709551615"));
    }
}
