//! Silero VAD of the studio: the shared module of `dub_asr::vad` (Speech Studio takes the same) on the transcribe.cpp
//! build of the recognition device chosen in the settings.

use std::path::Path;

pub use dub_asr::vad::{speech, windows, VadConfig, MAX_WINDOW_SECS, SR};

/// The speech-marking components missing on disk: the Silero VAD model and the transcribe.cpp build of the
/// recognition device.
pub fn missing(models_root: &Path) -> Vec<String> {
    dub_asr::vad::missing(models_root, crate::models::tcpp_build(models_root))
}

/// The probability of speech in every 32 ms frame (16 kHz mono).
pub fn probabilities(models_root: &Path, samples: &[f32]) -> Result<Vec<f32>, String> {
    dub_asr::vad::probabilities(models_root, crate::models::tcpp_build(models_root), samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs a models folder with transcribe.cpp and Silero VAD and a 16 kHz mono WAV: DUB_VAD_MODELS, DUB_VAD_IN"]
    fn probabilities_of_a_file_for_comparison_with_the_python_wrapper() {
        let var = |k: &str| std::path::PathBuf::from(std::env::var(k).unwrap());
        let (x, _) = crate::wavio::read_mono_f32(&var("DUB_VAD_IN")).unwrap();
        let p = probabilities(&var("DUB_VAD_MODELS"), &x).unwrap();
        println!("PROBS {}", p.iter().map(|v| format!("{v:.5}")).collect::<Vec<_>>().join(","));
        println!("SPANS {:?}", speech(&p, x.len(), &VadConfig::default()));
    }
}
