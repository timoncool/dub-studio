//! DPDFNet ONNX execution, cached model ownership and per-recording recurrent state; the same code as Speech Studio's
//! (`engines/denoise.rs` with `audio/denoise.rs` as [`dsp`]).

mod dsp;

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use ort::session::Session;
use ort::value::TensorRef;

pub use dsp::SR;

pub fn model_path(models_root: &Path) -> PathBuf {
    models_root.join("dpdfnet").join("dpdfnet2_48khz_hr.onnx")
}

struct Model {
    session: Session,
    init_state: Vec<f32>,
    win: usize,
}

static MODEL: LazyLock<Mutex<Option<(PathBuf, Model)>>> = LazyLock::new(|| Mutex::new(None));

fn load(path: &Path) -> Result<Model, String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let session = Session::builder()
        .map_err(|e| err(&e))?
        .with_intra_threads(1)
        .map_err(|e| err(&e))?
        .commit_from_file(path)
        .map_err(|e| err(&e))?;
    let meta = session.metadata().map_err(|e| err(&e))?;
    let info = model_info(path, |key| meta.custom(key))?;
    drop(meta);
    Ok(Model {
        session,
        init_state: info.init_state,
        win: info.win,
    })
}

struct ModelInfo {
    init_state: Vec<f32>,
    win: usize,
}

fn model_info(path: &Path, metadata: impl Fn(&str) -> Option<String>) -> Result<ModelInfo, String> {
    let get = |k: &str| metadata(k).ok_or_else(|| format!("{}: no {k} in the model metadata", path.display()));
    let size = |k: &str| get(k)?.trim().parse::<usize>().map_err(|e| format!("{}: {k}: {e}", path.display()));
    let floats = |k: &str| -> Result<Vec<f32>, String> {
        let values = get(k)?
            .split(',')
            .map(|x| x.trim().parse::<f32>().map_err(|e| format!("{}: {k}: {e}", path.display())))
            .collect::<Result<Vec<_>, _>>()?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(format!("{}: {k}: non-finite normalisation state", path.display()));
        }
        Ok(values)
    };
    let (state, erb, spec) = (size("state_size")?, size("erb_norm_state_size")?, size("spec_norm_state_size")?);
    if i64::try_from(state).is_err() || erb.checked_add(spec).is_none_or(|end| end > state) {
        return Err(format!("{}: the normalisation states do not match the metadata", path.display()));
    }
    let bins = size("freq_bins")?;
    let win = bins
        .checked_sub(1)
        .and_then(|value| value.checked_mul(2))
        .filter(|win| *win >= 2)
        .ok_or_else(|| format!("{}: invalid freq_bins: {bins}", path.display()))?;
    let (erb_init, spec_init) = (floats("erb_norm_init")?, floats("spec_norm_init")?);
    if erb_init.len() != erb || spec_init.len() != spec {
        return Err(format!("{}: the normalisation states do not match the metadata", path.display()));
    }
    let mut init_state = vec![0f32; state];
    init_state[..erb].copy_from_slice(&erb_init);
    init_state[erb..erb + spec].copy_from_slice(&spec_init);
    Ok(ModelInfo { init_state, win })
}

/// Clean a 48 kHz mono signal with the cached DPDFNet model and a fresh recurrent state.
pub fn denoise(model: &Path, input: &[f32], limit_db: Option<f32>, progress: &dyn Fn(f32)) -> Result<Vec<f32>, String> {
    let mut slot = MODEL.lock().map_err(|_| "dpdfnet lock".to_string())?;
    if slot.as_ref().is_none_or(|(path, _)| path != model) {
        *slot = Some((model.to_path_buf(), load(model)?));
    }
    let (_, model) = slot.as_mut().expect("dpdfnet model");
    let mut state = model.init_state.clone();
    dsp::process(input, model.win, limit_db, progress, |frame| model.infer(frame, &mut state))
}

impl Model {
    fn infer(&mut self, frame: &mut [f32], state: &mut [f32]) -> Result<(), String> {
        let bins = self.win / 2 + 1;
        let outputs = self.session.run(ort::inputs![
            "spec" => TensorRef::from_array_view((vec![1i64, 1, bins as i64, 2], &*frame)).map_err(|error| format!("dpdfnet: {error}"))?,
            "state_in" => TensorRef::from_array_view((vec![state.len() as i64], &*state)).map_err(|error| format!("dpdfnet: {error}"))?,
        ]).map_err(|error| format!("dpdfnet: {error}"))?;
        let spectrum = outputs.get("spec_e").ok_or("dpdfnet: missing spec_e output")?;
        let (_, spectrum) = spectrum.try_extract_tensor::<f32>().map_err(|error| format!("dpdfnet: {error}"))?;
        let next_state = outputs.get("state_out").ok_or("dpdfnet: missing state_out output")?;
        let (_, next_state) = next_state
            .try_extract_tensor::<f32>()
            .map_err(|error| format!("dpdfnet: {error}"))?;
        copy_outputs(frame, state, spectrum, next_state)
    }
}

fn copy_outputs(frame: &mut [f32], state: &mut [f32], spectrum: &[f32], next_state: &[f32]) -> Result<(), String> {
    if spectrum.len() != frame.len() {
        return Err(format!("dpdfnet: spec_e has {} values, expected {}", spectrum.len(), frame.len()));
    }
    if next_state.len() != state.len() {
        return Err(format!(
            "dpdfnet: state_out has {} values, expected {}",
            next_state.len(),
            state.len()
        ));
    }
    frame.copy_from_slice(spectrum);
    state.copy_from_slice(next_state);
    Ok(())
}

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

    fn metadata() -> std::collections::HashMap<&'static str, String> {
        [
            ("state_size", "5"),
            ("erb_norm_state_size", "2"),
            ("spec_norm_state_size", "1"),
            ("erb_norm_init", "0.1, 0.2"),
            ("spec_norm_init", "0.3"),
            ("freq_bins", "5"),
        ]
        .into_iter()
        .map(|(key, value)| (key, value.to_string()))
        .collect()
    }

    #[test]
    fn metadata_initialises_normalisation_and_zeroes_the_remaining_state() {
        let metadata = metadata();
        let info = model_info(Path::new("model.onnx"), |key| metadata.get(key).cloned()).unwrap();
        assert_eq!(info.win, 8);
        assert_eq!(info.init_state, [0.1, 0.2, 0.3, 0.0, 0.0]);
    }

    #[test]
    fn missing_and_invalid_metadata_name_the_key_and_model() {
        for (key, value) in [
            ("freq_bins", None),
            ("state_size", Some("no size")),
            ("erb_norm_init", Some("not a float")),
            ("spec_norm_init", Some("NaN")),
        ] {
            let mut metadata = metadata();
            match value {
                Some(value) => {
                    metadata.insert(key, value.to_string());
                }
                None => {
                    metadata.remove(key);
                }
            }
            let error = model_info(Path::new("model.onnx"), |key| metadata.get(key).cloned())
                .err()
                .expect("invalid metadata");
            assert!(error.contains("model.onnx") && error.contains(key), "{error}");
        }
    }

    #[test]
    fn inconsistent_and_overflowing_state_sizes_are_rejected() {
        for (key, value) in [
            ("state_size", "2".to_string()),
            ("erb_norm_state_size", usize::MAX.to_string()),
            ("spec_norm_init", "0.3, 0.4".to_string()),
        ] {
            let mut metadata = metadata();
            metadata.insert(key, value);
            assert!(model_info(Path::new("model.onnx"), |key| metadata.get(key).cloned()).is_err());
        }
    }

    #[test]
    fn invalid_frequency_bins_cannot_make_a_zero_or_overflowing_window() {
        for bins in [0, 1, usize::MAX] {
            let mut metadata = metadata();
            metadata.insert("freq_bins", bins.to_string());
            let error = model_info(Path::new("model.onnx"), |key| metadata.get(key).cloned())
                .err()
                .expect("invalid frequency bins");
            assert!(error.contains("freq_bins"), "{error}");
        }
    }

    #[test]
    fn valid_outputs_replace_the_spectrum_and_recurrent_state() {
        let mut frame = [1.0; 4];
        let mut state = [2.0; 3];
        copy_outputs(&mut frame, &mut state, &[3.0, 4.0, 5.0, 6.0], &[7.0, 8.0, 9.0]).unwrap();
        assert_eq!(frame, [3.0, 4.0, 5.0, 6.0]);
        assert_eq!(state, [7.0, 8.0, 9.0]);
    }

    #[test]
    fn bad_outputs_do_not_partially_replace_a_frame_or_state() {
        for (spectrum, next_state, name) in [
            (&[3.0][..], &[4.0, 5.0][..], "spec_e"),
            (&[3.0; 5][..], &[4.0, 5.0][..], "spec_e"),
            (&[3.0; 4][..], &[4.0][..], "state_out"),
            (&[3.0; 4][..], &[4.0; 3][..], "state_out"),
        ] {
            let mut frame = [1.0; 4];
            let mut state = [2.0; 2];
            let error = copy_outputs(&mut frame, &mut state, spectrum, next_state).unwrap_err();
            assert!(error.contains(name), "{error}");
            assert_eq!(frame, [1.0; 4]);
            assert_eq!(state, [2.0; 2]);
        }
    }

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

    #[test]
    #[ignore = "needs the DPDFNet model and a 48 kHz mono WAV: DUB_DENOISE_MODEL, DUB_DENOISE_IN, DUB_DENOISE_OUT"]
    fn denoise_a_file_for_comparison_with_the_python_package() {
        let var = |k: &str| PathBuf::from(std::env::var(k).unwrap());
        let (x, sr) = crate::wavio::read_mono_f32(&var("DUB_DENOISE_IN")).unwrap();
        assert_eq!(sr, SR);
        let limit = std::env::var("DUB_DENOISE_DB").ok().map(|v| v.parse::<f32>().unwrap());
        let y = denoise(&var("DUB_DENOISE_MODEL"), &x, limit, &|_| {}).unwrap();
        let repeated = denoise(&var("DUB_DENOISE_MODEL"), &x, limit, &|_| {}).unwrap();
        assert_eq!(y, repeated, "each recording starts with a fresh recurrent state");
        crate::wavio::write_mono_f32(&var("DUB_DENOISE_OUT"), &y, SR).unwrap();
    }
}
