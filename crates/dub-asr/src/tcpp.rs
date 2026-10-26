//! Распознаватель на transcribe.cpp для `AsrEngine` (`dub_tcpp`): модель берётся из кэша процесса при первом
//! распознавании, слова режутся на реплики как у остальных движков.

pub use dub_tcpp::{
    chosen, cpu_floor, cpu_floor_missing, detect_language, engine_dir, helper_path, installed, knows, model, model_path, shared, unload, vad_probabilities, Build, Download,
    Helper, LangGuess, Model, TcppAsr, TcppStream, LANGID, LANGID_MIN_SAMPLES, LIBRARY, MODELS, VAD,
};

use crate::{segment_words, AsrEngine, AsrError, Segment, SpeakerSegment, Turn, Word, SEG_MAX_DUR, SEG_MAX_GAP};
use std::path::{Path, PathBuf};

fn engine_err(e: String) -> AsrError {
    AsrError::Parakeet(e)
}

fn ours(words: Vec<dub_tcpp::Word>) -> Vec<Word> {
    words.into_iter().map(|w| Word { word: w.word, start: w.start, end: w.end }).collect()
}

/// Распознаватель задачи: берёт модель из кэша процесса при первом распознавании (студия строит его до того, как
/// знает, понадобится ли он).
pub struct TcppEngine {
    engine_dir: PathBuf,
    model: PathBuf,
}

impl TcppEngine {
    pub fn new(models_root: &Path, build: Build, m: &Model) -> TcppEngine {
        TcppEngine { engine_dir: engine_dir(models_root, build), model: model_path(models_root, m) }
    }

    fn words(&self, pcm: &[f32], lang: &str) -> Result<Vec<Word>, AsrError> {
        let asr = shared(&self.engine_dir, &self.model).map_err(engine_err)?;
        let mut asr = asr.lock().unwrap_or_else(|e| e.into_inner());
        asr.words(pcm, lang).map(ours).map_err(engine_err)
    }
}

impl TcppEngine {
    fn many(&self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<Word>, AsrError>> {
        let loaded: Vec<Result<Vec<f32>, AsrError>> = files.iter().map(|f| crate::load_wav_16k_mono(f).map(|(pcm, _)| pcm)).collect();
        let ok: Vec<&[f32]> = loaded.iter().filter_map(|r| r.as_ref().ok().map(Vec::as_slice)).collect();
        let heard = shared(&self.engine_dir, &self.model).and_then(|asr| asr.lock().unwrap_or_else(|e| e.into_inner()).words_batch(&ok, lang));
        let mut heard = match heard {
            Ok(h) => h.into_iter(),
            Err(e) => return files.iter().map(|_| Err(engine_err(e.clone()))).collect(),
        };
        loaded
            .into_iter()
            .map(|r| r.and_then(|_| heard.next().expect("one answer per loaded file").map(ours).map_err(engine_err)))
            .collect()
    }
}

impl AsrEngine for TcppEngine {
    fn transcribe_many(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<String, AsrError>> {
        self.many(files, lang).into_iter().map(|r| r.map(|w| crate::join_text(w.iter().map(|w| w.word.as_str())))).collect()
    }

    fn transcribe_many_words(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<Word>, AsrError>> {
        self.many(files, lang)
    }

    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError> {
        let (pcm, _) = crate::load_wav_16k_mono(wav)?;
        Ok(segment_words(&self.words(&pcm, lang)?, SEG_MAX_GAP, SEG_MAX_DUR))
    }

    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError> {
        let (pcm, _) = crate::load_wav_16k_mono(wav)?;
        let mut out = Vec::new();
        for t in turns {
            let a = (t.start * 16_000.0) as usize;
            let b = ((t.end * 16_000.0) as usize).min(pcm.len());
            if b <= a || b - a < 3_200 {
                continue;
            }
            for s in segment_words(&self.words(&pcm[a..b], lang)?, SEG_MAX_GAP, SEG_MAX_DUR) {
                out.push(SpeakerSegment { start: t.start + s.start, end: t.start + s.end, text: s.text, speaker: t.speaker });
            }
        }
        Ok(out)
    }
}

/// Whether `model` must be told the language (Nemotron 3.5 told none loses the first word of a recording): under
/// «auto» it needs the language id ([`LANGID`]) on the transcribe.cpp build it runs on.
pub fn told_the_language(model: &Model) -> bool {
    model.id == "nemotron"
}

/// The language of `model` VoxLingua107 hears in 16 kHz mono speech, chosen among the model's languages: None for
/// speech shorter than the detector takes (the model then finds the language itself).
pub fn heard_language(models_root: &Path, build: Build, model: &Model, pcm16k: &[f32]) -> Result<Option<String>, String> {
    if pcm16k.len() < LANGID_MIN_SAMPLES {
        return Ok(None);
    }
    let head = &pcm16k[..pcm16k.len().min(acpp_server::language::LANGUAGE_ID_SECS * 16_000)];
    // among the model's own languages: two seconds of Russian are heard as Belarusian sooner than as Russian
    let guess = detect_language(&engine_dir(models_root, build), &helper_path(models_root, &LANGID), head, model.languages)?;
    let Some((code, p)) = guess.ranked.into_iter().next() else {
        return Ok(None);
    };
    if knows(model, &code) {
        tracing::info!("recognition: {} is told {code}, heard by VoxLingua107 ({p:.2})", model.name);
        Ok(Some(code))
    } else {
        tracing::info!("recognition: VoxLingua107 hears {code} ({p:.2}), which {} does not know; it finds the language itself", model.name);
        Ok(None)
    }
}

/// The recogniser of `model` for a job: a model that must be told the language is told under «auto» the one heard in
/// the job's first recording long enough to hear it, and that one for the rest of the job; any other is `inner` itself.
pub fn with_heard_language(inner: Box<dyn AsrEngine>, models_root: &Path, build: Build, model: &'static Model) -> Box<dyn AsrEngine> {
    if !told_the_language(model) {
        return inner;
    }
    Box::new(HeardLanguage { inner, models_root: models_root.to_path_buf(), build, model, heard: None })
}

struct HeardLanguage {
    inner: Box<dyn AsrEngine>,
    models_root: PathBuf,
    build: Build,
    model: &'static Model,
    heard: Option<String>,
}

impl HeardLanguage {
    fn language(&mut self, wav: &Path, lang: &str) -> Result<String, AsrError> {
        if !(lang.is_empty() || lang == "auto") {
            return Ok(lang.to_string());
        }
        if self.heard.is_none() {
            let (pcm, _) = crate::load_wav_16k_mono_head(wav, Some(acpp_server::language::LANGUAGE_ID_SECS))?;
            self.heard = heard_language(&self.models_root, self.build, self.model, &pcm).map_err(engine_err)?;
        }
        Ok(self.heard.clone().unwrap_or_else(|| "auto".into()))
    }
}

impl AsrEngine for HeardLanguage {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError> {
        let lang = self.language(wav, lang)?;
        self.inner.transcribe(wav, &lang)
    }

    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError> {
        let lang = self.language(wav, lang)?;
        self.inner.transcribe_turns(wav, turns, &lang)
    }

    fn transcribe_many(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<String, AsrError>> {
        match files.first().map(|f| self.language(f, lang)).transpose() {
            Ok(told) => self.inner.transcribe_many(files, told.as_deref().unwrap_or(lang)),
            Err(e) => files.iter().map(|_| Err(engine_err(e.to_string()))).collect(),
        }
    }

    fn transcribe_many_words(&mut self, files: &[PathBuf], lang: &str) -> Vec<Result<Vec<Word>, AsrError>> {
        match files.first().map(|f| self.language(f, lang)).transpose() {
            Ok(told) => self.inner.transcribe_many_words(files, told.as_deref().unwrap_or(lang)),
            Err(e) => files.iter().map(|_| Err(engine_err(e.to_string()))).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A recogniser that keeps the languages it is told.
    struct Told(Arc<Mutex<Vec<String>>>);

    impl AsrEngine for Told {
        fn transcribe(&mut self, _: &Path, lang: &str) -> Result<Vec<Segment>, AsrError> {
            self.0.lock().unwrap().push(lang.to_string());
            Ok(Vec::new())
        }
        fn transcribe_turns(&mut self, _: &Path, _: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError> {
            self.0.lock().unwrap().push(lang.to_string());
            Ok(Vec::new())
        }
    }

    fn wav(dir: &Path, secs: f32) -> PathBuf {
        let path = dir.join(format!("{secs}.wav"));
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..(secs * 16_000.0) as usize {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        path
    }

    fn nemotron(told: &Arc<Mutex<Vec<String>>>) -> Box<dyn AsrEngine> {
        with_heard_language(Box::new(Told(told.clone())), Path::new("no-models-here"), Build::Cpu, model("nemotron").unwrap())
    }

    #[test]
    fn a_chosen_language_reaches_the_model_as_it_is() {
        let dir = std::env::temp_dir().join("dub-asr-heard-chosen");
        std::fs::create_dir_all(&dir).unwrap();
        let told = Arc::new(Mutex::new(Vec::new()));
        nemotron(&told).transcribe(&wav(&dir, 3.0), "ru").unwrap();
        assert_eq!(*told.lock().unwrap(), ["ru"]);
    }

    #[test]
    fn speech_too_short_to_hear_leaves_the_model_to_find_its_language() {
        let dir = std::env::temp_dir().join("dub-asr-heard-short");
        std::fs::create_dir_all(&dir).unwrap();
        let told = Arc::new(Mutex::new(Vec::new()));
        let mut asr = nemotron(&told);
        asr.transcribe(&wav(&dir, 0.25), "auto").unwrap();
        asr.transcribe_turns(&wav(&dir, 0.3), &[], "").unwrap();
        assert_eq!(*told.lock().unwrap(), ["auto", "auto"]);
    }

    #[test]
    fn a_model_that_finds_its_language_is_not_wrapped() {
        let dir = std::env::temp_dir().join("dub-asr-heard-own");
        std::fs::create_dir_all(&dir).unwrap();
        let told = Arc::new(Mutex::new(Vec::new()));
        let mut asr = with_heard_language(Box::new(Told(told.clone())), Path::new("no-models-here"), Build::Cpu, model("parakeet-v3").unwrap());
        asr.transcribe(&wav(&dir, 3.0), "auto").unwrap();
        assert_eq!(*told.lock().unwrap(), ["auto"]);
    }

    /// DUB_ASR_MODELS: a models folder with the transcribe.cpp build DUB_ASR_BUILD (cpu, cuda or vulkan) and the language
    /// id; DUB_ASR_WAV: speech; DUB_ASR_LANG: its language. Nemotron is told it under «auto».
    #[test]
    #[ignore]
    fn nemotron_under_auto_is_told_the_language_it_hears() {
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} is not set"));
        let build = match env("DUB_ASR_BUILD").as_str() {
            "cpu" => Build::Cpu,
            "cuda" => Build::Cuda,
            "vulkan" => Build::Vulkan,
            other => panic!("DUB_ASR_BUILD={other}"),
        };
        let told = Arc::new(Mutex::new(Vec::new()));
        let mut asr = with_heard_language(Box::new(Told(told.clone())), Path::new(&env("DUB_ASR_MODELS")), build, model("nemotron").unwrap());
        let wav = PathBuf::from(env("DUB_ASR_WAV"));
        asr.transcribe(&wav, "auto").unwrap();
        asr.transcribe_many(&[wav], "auto");
        let lang = env("DUB_ASR_LANG");
        assert_eq!(*told.lock().unwrap(), [lang.clone(), lang], "the language is heard once and kept for the job");
    }
}

