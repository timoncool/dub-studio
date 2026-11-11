//! DPDFNet noise reduction of the studio: the shared module of `dub_asr::denoise` (Speech Studio takes the same) over
//! a file the studio's ffmpeg brings to 48 kHz mono.

use std::path::Path;

pub use dub_asr::denoise::{denoise, model_path, SR};

/// Голос или запись без шума: файл в 48 кГц моно через ffmpeg, DPDFNet, запись WAV рядом в `dst`.
pub fn denoise_file(models_root: &Path, src: &Path, dst: &Path, limit_db: Option<f32>, progress: &dyn Fn(f32)) -> Result<(), String> {
    let model = model_path(models_root);
    if !model.is_file() {
        return Err(t!("denoise-missing"));
    }
    let tmp = tempfile::Builder::new().prefix("denoise-").tempdir().map_err(|e| e.to_string())?;
    let wav = tmp.path().join("in48.wav");
    crate::media::extract_audio(src, &wav, SR, 1)?;
    let (samples, _) = crate::wavio::read_mono_f32(&wav)?;
    let clean = denoise(&model, &samples, limit_db, progress)?;
    crate::wavio::write_mono_f32(dst, &clean, SR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_model_does_not_start_file_processing() {
        let _language = crate::i18n::test_language("en");
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("clean.wav");
        let error = denoise_file(root.path(), &root.path().join("missing.wav"), &destination, None, &|_| {}).unwrap_err();
        assert_eq!(error, t!("denoise-missing"));
        assert!(!destination.exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
