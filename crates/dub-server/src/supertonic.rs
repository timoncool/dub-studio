//! Supertonic 3 (Supertone), ported from the official Rust runtime (`supertonic/rust/src/helper.rs`): four ONNX graphs —
//! duration, text encoder, flow-matching vector estimator, vocoder — on onnxruntime's CUDA provider, ten preset voices,
//! 31 languages, no cloning. The weights are askurios8/supertonic-3-int8 (weight-only int8, fp32 activations), whose
//! graph inputs and outputs are the original's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ort::session::Session;
use ort::value::TensorRef;
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

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

#[derive(Deserialize)]
struct Config {
    ae: AeConfig,
    ttl: TtlConfig,
}

#[derive(Deserialize)]
struct AeConfig {
    sample_rate: u32,
    base_chunk_size: usize,
}

#[derive(Deserialize)]
struct TtlConfig {
    chunk_compress_factor: usize,
    latent_dim: usize,
}

#[derive(Deserialize)]
struct StyleFile {
    style_ttl: StylePart,
    style_dp: StylePart,
}

#[derive(Deserialize)]
struct StylePart {
    data: Vec<Vec<Vec<f32>>>,
    dims: Vec<usize>,
}

/// A voice: the conditioning of the text encoder and estimator, and of the duration predictor.
struct Style {
    ttl: (Vec<i64>, Vec<f32>),
    dp: (Vec<i64>, Vec<f32>),
}

pub struct Model {
    cfg: Config,
    indexer: Vec<i64>,
    dp: Mutex<Session>,
    enc: Mutex<Session>,
    est: Mutex<Session>,
    voc: Mutex<Session>,
    styles: Mutex<HashMap<String, Arc<Style>>>,
    dir: PathBuf,
}

pub fn dir(models_root: &Path) -> PathBuf {
    models_root.join(DIR)
}

pub fn installed(models_root: &Path) -> bool {
    let d = dir(models_root);
    ["duration_predictor", "text_encoder", "vector_estimator", "vocoder"].iter().all(|g| d.join("onnx").join(format!("{g}.onnx")).is_file())
        && VOICES.iter().all(|v| d.join("voice_styles").join(format!("{v}.json")).is_file())
}

fn session(path: &Path, gpu: bool) -> Result<Session, String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let mut b = Session::builder().map_err(|e| err(&e))?;
    if gpu {
        b = b.with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()]).map_err(|e| err(&e))?;
    }
    let session = b.commit_from_file(path).map_err(|e| err(&e))?;
    tracing::info!("onnxruntime {}: {}", path.display(), if gpu { "CUDA" } else { "CPU" });
    Ok(session)
}

fn json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

impl Model {
    /// The four graphs on the GPU when `gpu`, else on the CPU; a GPU that fails is an error, not a quiet CPU run.
    pub fn load(models_root: &Path, gpu: bool) -> Result<Model, String> {
        let d = dir(models_root);
        let onnx = d.join("onnx");
        Ok(Model {
            cfg: json(&onnx.join("tts.json"))?,
            indexer: json(&onnx.join("unicode_indexer.json"))?,
            dp: Mutex::new(session(&onnx.join("duration_predictor.onnx"), gpu)?),
            enc: Mutex::new(session(&onnx.join("text_encoder.onnx"), gpu)?),
            est: Mutex::new(session(&onnx.join("vector_estimator.onnx"), gpu)?),
            voc: Mutex::new(session(&onnx.join("vocoder.onnx"), gpu)?),
            styles: Mutex::new(HashMap::new()),
            dir: d,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.cfg.ae.sample_rate
    }

    fn style(&self, voice: &str) -> Result<Arc<Style>, String> {
        if !VOICES.contains(&voice) {
            return Err(format!("supertonic: no voice {voice}; the voices are {}", VOICES.join(", ")));
        }
        let mut styles = self.styles.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(s) = styles.get(voice) {
            return Ok(s.clone());
        }
        let f: StyleFile = json(&self.dir.join("voice_styles").join(format!("{voice}.json")))?;
        let part = |p: StylePart| (p.dims.iter().map(|d| *d as i64).collect::<Vec<_>>(), p.data.into_iter().flatten().flatten().collect::<Vec<_>>());
        let s = Arc::new(Style { ttl: part(f.style_ttl), dp: part(f.style_dp) });
        styles.insert(voice.to_string(), s.clone());
        Ok(s)
    }

    /// Speaks one phrase (up to `MAX_CHARS`) in a preset voice; the same `seed` gives the same take.
    pub fn speak(&self, text: &str, lang: &str, voice: &str, steps: u32, speed: f64, seed: u64) -> Result<Vec<f32>, String> {
        let lang = if LANGS.contains(&lang) { lang } else { "na" };
        let style = self.style(voice)?;
        let prepared = preprocess(text, lang);
        let ids: Vec<i64> = prepared.chars().map(|c| self.indexer.get(c as usize).copied().unwrap_or(-1)).collect();
        let n = ids.len();
        let ids_shape = vec![1i64, n as i64];
        let text_mask = vec![1f32; n];
        let mask_shape = vec![1i64, 1, n as i64];
        let fail = |what: &str, e: &dyn std::fmt::Display| format!("supertonic {what}: {e}");

        let duration = {
            let mut s = self.dp.lock().unwrap_or_else(|p| p.into_inner());
            let out = s
                .run(ort::inputs![
                    "text_ids" => TensorRef::from_array_view((ids_shape.clone(), ids.as_slice())).map_err(|e| fail("duration", &e))?,
                    "style_dp" => TensorRef::from_array_view((style.dp.0.clone(), style.dp.1.as_slice())).map_err(|e| fail("duration", &e))?,
                    "text_mask" => TensorRef::from_array_view((mask_shape.clone(), text_mask.as_slice())).map_err(|e| fail("duration", &e))?,
                ])
                .map_err(|e| fail("duration", &e))?;
            let (_, d) = out["duration"].try_extract_tensor::<f32>().map_err(|e| fail("duration", &e))?;
            d[0] / speed.max(0.1) as f32
        };

        let (emb_shape, emb) = {
            let mut s = self.enc.lock().unwrap_or_else(|p| p.into_inner());
            let out = s
                .run(ort::inputs![
                    "text_ids" => TensorRef::from_array_view((ids_shape, ids.as_slice())).map_err(|e| fail("text encoder", &e))?,
                    "style_ttl" => TensorRef::from_array_view((style.ttl.0.clone(), style.ttl.1.as_slice())).map_err(|e| fail("text encoder", &e))?,
                    "text_mask" => TensorRef::from_array_view((mask_shape.clone(), text_mask.as_slice())).map_err(|e| fail("text encoder", &e))?,
                ])
                .map_err(|e| fail("text encoder", &e))?;
            let (shape, data) = out["text_emb"].try_extract_tensor::<f32>().map_err(|e| fail("text encoder", &e))?;
            (shape.iter().copied().collect::<Vec<i64>>(), data.to_vec())
        };

        let sr = self.cfg.ae.sample_rate as usize;
        let chunk = self.cfg.ae.base_chunk_size * self.cfg.ttl.chunk_compress_factor;
        let wav_len = (duration.max(0.0) * sr as f32) as usize;
        let latent_len = wav_len.div_ceil(chunk).max(1);
        let latent_dim = self.cfg.ttl.latent_dim * self.cfg.ttl.chunk_compress_factor;
        let latent_shape = vec![1i64, latent_dim as i64, latent_len as i64];
        let mut xt = gaussian(latent_dim * latent_len, seed);
        let latent_mask = vec![1f32; latent_len];
        let latent_mask_shape = vec![1i64, 1, latent_len as i64];
        let total = [steps.max(1) as f32];
        for step in 0..steps.max(1) {
            let current = [step as f32];
            let mut s = self.est.lock().unwrap_or_else(|p| p.into_inner());
            let out = s
                .run(ort::inputs![
                    "noisy_latent" => TensorRef::from_array_view((latent_shape.clone(), xt.as_slice())).map_err(|e| fail("estimator", &e))?,
                    "text_emb" => TensorRef::from_array_view((emb_shape.clone(), emb.as_slice())).map_err(|e| fail("estimator", &e))?,
                    "style_ttl" => TensorRef::from_array_view((style.ttl.0.clone(), style.ttl.1.as_slice())).map_err(|e| fail("estimator", &e))?,
                    "latent_mask" => TensorRef::from_array_view((latent_mask_shape.clone(), latent_mask.as_slice())).map_err(|e| fail("estimator", &e))?,
                    "text_mask" => TensorRef::from_array_view((mask_shape.clone(), text_mask.as_slice())).map_err(|e| fail("estimator", &e))?,
                    "current_step" => TensorRef::from_array_view((vec![1i64], current.as_slice())).map_err(|e| fail("estimator", &e))?,
                    "total_step" => TensorRef::from_array_view((vec![1i64], total.as_slice())).map_err(|e| fail("estimator", &e))?,
                ])
                .map_err(|e| fail("estimator", &e))?;
            let (_, data) = out["denoised_latent"].try_extract_tensor::<f32>().map_err(|e| fail("estimator", &e))?;
            xt = data.to_vec();
        }

        let mut s = self.voc.lock().unwrap_or_else(|p| p.into_inner());
        let out = s
            .run(ort::inputs!["latent" => TensorRef::from_array_view((latent_shape, xt.as_slice())).map_err(|e| fail("vocoder", &e))?])
            .map_err(|e| fail("vocoder", &e))?;
        let (_, wav) = out["wav_tts"].try_extract_tensor::<f32>().map_err(|e| fail("vocoder", &e))?;
        Ok(wav[..wav_len.min(wav.len())].to_vec())
    }
}

/// Standard normal noise from `seed` (splitmix64 and Box-Muller): the starting latent of the flow.
fn gaussian(n: usize, seed: u64) -> Vec<f32> {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut out = Vec::with_capacity(n + 1);
    while out.len() < n {
        let u1 = next().max(f64::MIN_POSITIVE);
        let u2 = next();
        let r = (-2.0 * u1.ln()).sqrt();
        out.push((r * (std::f64::consts::TAU * u2).cos()) as f32);
        out.push((r * (std::f64::consts::TAU * u2).sin()) as f32);
    }
    out.truncate(n);
    out
}

/// The official text clean-up (`preprocess_text`): NFKD, dashes and quotes, no emoji or brackets, a closing full stop,
/// wrapped in the language tags the model reads.
pub fn preprocess(text: &str, lang: &str) -> String {
    let mut t: String = text.nfkd().filter(|c| !is_emoji(*c)).collect();
    for (from, to) in [("–", "-"), ("‑", "-"), ("—", "-"), ("_", " "), ("\u{201C}", "\""), ("\u{201D}", "\""), ("\u{2018}", "'"), ("\u{2019}", "'"), ("´", "'"), ("`", "'"), ("[", " "), ("]", " "), ("|", " "), ("/", " "), ("#", " "), ("→", " "), ("←", " ")] {
        t = t.replace(from, to);
    }
    for symbol in ["♥", "☆", "♡", "©", "\\"] {
        t = t.replace(symbol, "");
    }
    for (from, to) in [("@", " at "), ("e.g.,", "for example, "), ("i.e.,", "that is, ")] {
        t = t.replace(from, to);
    }
    for p in [",", ".", "!", "?", ";", ":", "'"] {
        t = t.replace(&format!(" {p}"), p);
    }
    while t.contains("\"\"") {
        t = t.replace("\"\"", "\"");
    }
    while t.contains("''") {
        t = t.replace("''", "'");
    }
    let mut t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(last) = t.chars().last() {
        if !".!?;:,'\"\u{201C}\u{201D}\u{2018}\u{2019})]}…。」』】〉》›»".contains(last) {
            t.push('.');
        }
    }
    format!("<{lang}>{t}</{lang}>")
}

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F600..=0x1F64F | 0x1F300..=0x1F5FF | 0x1F680..=0x1F6FF | 0x1F700..=0x1F77F | 0x1F780..=0x1F7FF | 0x1F800..=0x1F8FF | 0x1F900..=0x1F9FF | 0x1FA00..=0x1FA6F | 0x1FA70..=0x1FAFF | 0x2600..=0x26FF | 0x2700..=0x27BF | 0x1F1E6..=0x1F1FF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cleaned_and_tagged_as_the_official_runtime_does() {
        assert_eq!(preprocess("Привет — мир", "ru"), "<ru>Привет - мир.</ru>");
        assert_eq!(preprocess("Ok , go !", "en"), "<en>Ok, go!</en>");
        assert!(preprocess("й", "ru").contains('\u{306}'), "NFKD splits й as the model's indexer expects");
    }

    #[test]
    fn the_same_seed_gives_the_same_noise() {
        let a = gaussian(1001, 7);
        assert_eq!(a, gaussian(1001, 7));
        assert_ne!(a, gaussian(1001, 8));
        let mean = a.iter().map(|v| *v as f64).sum::<f64>() / a.len() as f64;
        let var = a.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / a.len() as f64;
        assert!(mean.abs() < 0.1 && (var - 1.0).abs() < 0.15, "{mean} {var}");
    }

    /// The real model: `SUPERTONIC_ROOT` is a models root holding `supertonic3/`, `ORT_DYLIB_PATH` an onnxruntime of the
    /// pinned version (its GPU build).
    #[test]
    #[ignore]
    fn speaks_russian_on_the_gpu() {
        speaks_russian(true);
    }

    /// As `speaks_russian_on_the_gpu`, on the processor: `ORT_DYLIB_PATH` may name the CPU build of onnxruntime.
    #[test]
    #[ignore]
    fn speaks_russian_on_the_processor() {
        speaks_russian(false);
    }

    fn speaks_russian(gpu: bool) {
        let root = PathBuf::from(std::env::var("SUPERTONIC_ROOT").expect("SUPERTONIC_ROOT"));
        let m = Model::load(&root, gpu).unwrap();
        let started = std::time::Instant::now();
        let wav = m.speak("Сегодня в восемь вечера мы встретимся у старого моста.", "ru", "F1", DEFAULT_STEPS, DEFAULT_SPEED, 42).unwrap();
        let secs = wav.len() as f64 / m.sample_rate() as f64;
        eprintln!("[OK] {secs:.2} s of audio in {:.3} s", started.elapsed().as_secs_f64());
        assert!(secs > 1.5 && secs < 8.0, "{secs}");
        assert_eq!(wav, m.speak("Сегодня в восемь вечера мы встретимся у старого моста.", "ru", "F1", DEFAULT_STEPS, DEFAULT_SPEED, 42).unwrap());
        if let Ok(out) = std::env::var("SUPERTONIC_OUT") {
            crate::wavio::write_mono_f32(Path::new(&out), &wav, m.sample_rate()).unwrap();
        }
    }

    /// The same homograph phrases without and with U+0301 after the stressed vowel, one voice and seed:
    /// `SUPERTONIC_OUT_DIR` gets `<n>-plain.wav` and `<n>-acute.wav` to listen to.
    #[test]
    #[ignore]
    fn speaks_marked_stress_beside_plain_text() {
        let root = PathBuf::from(std::env::var("SUPERTONIC_ROOT").expect("SUPERTONIC_ROOT"));
        let out = PathBuf::from(std::env::var("SUPERTONIC_OUT_DIR").expect("SUPERTONIC_OUT_DIR"));
        let m = Model::load(&root, true).unwrap();
        let phrases = [
            "Он запер ста\u{301}рый замо\u{301}к и пошёл к за\u{301}мку на горе\u{301}.",
            "Мука\u{301} была\u{301} в мешке\u{301}, а му\u{301}ка — в душе\u{301}.",
            "В а\u{301}тласе на столе\u{301} лежа\u{301}л кусо\u{301}к атла\u{301}са.",
        ];
        for (n, marked) in phrases.iter().enumerate() {
            for (kind, text) in [("plain", marked.replace('\u{301}', "")), ("acute", marked.to_string())] {
                let wav = m.speak(&text, "ru", "M1", DEFAULT_STEPS, DEFAULT_SPEED, 7).unwrap();
                let secs = wav.len() as f64 / m.sample_rate() as f64;
                eprintln!("[OK] {kind} {n}: {secs:.2} s");
                assert!(secs > 1.0 && secs < 12.0, "{secs}");
                crate::wavio::write_mono_f32(&out.join(format!("{n}-{kind}.wav")), &wav, m.sample_rate()).unwrap();
            }
        }
    }
}
