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

