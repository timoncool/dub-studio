//! ONNX-движки PP-OCR через ort (load-dynamic, та же 1.28.2, что dub-asr). Det (DBNet) + Rec (CRNN).
//! ensure_ort_dylib() — копия паттерна dub-asr: без явного ORT_DYLIB_PATH ort цепляет чужую
//! system32\onnxruntime.dll (1.17) -> ДЕДЛОК при создании сессии. Выставляем на встроенную 1.28.2.

use crate::OcrError;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Гарантировать правильную onnxruntime.dll (1.28.2). Порт dub_asr::ensure_ort_dylib (тот же поиск).
pub fn ensure_ort_dylib() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("ORT_DYLIB_PATH").is_some() {
            return;
        }
        let mut cands: Vec<PathBuf> = Vec::new();
        if let Some(p) = std::env::var_os("DUB_ASR_ORT_DYLIB") {
            cands.push(PathBuf::from(p));
        }
        if let Some(p) = std::env::var_os("DUB_OCR_ORT_DYLIB") {
            cands.push(PathBuf::from(p));
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(m) = std::env::var_os("DUBENGINE_MODELS_ROOT") {
            roots.push(PathBuf::from(m));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.join("models"));
                if let Some(p2) = dir.parent().and_then(|d| d.parent()).and_then(|d| d.parent()) {
                    roots.push(p2.join("models"));
                }
                if let Some(p1) = dir.parent().and_then(|d| d.parent()) {
                    roots.push(p1.join("models"));
                }
                cands.push(dir.join("onnxruntime.dll"));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("models"));
        }
        for r in &roots {
            cands.push(r.join("runtime").join("onnxruntime-1.28.dll"));
            cands.push(
                r.join("runtime")
                    .join("onnxruntime-win-x64-1.28.2")
                    .join("lib")
                    .join("onnxruntime.dll"),
            );
        }
        for c in cands {
            if c.is_file() {
                std::env::set_var("ORT_DYLIB_PATH", &c);
                return;
            }
        }
    });
}

/// Обёртка над ort::Session для одного входа [N,3,H,W] -> один выход f32.
pub struct OnnxModel {
    session: Session,
}

impl OnnxModel {
    pub fn load(path: &Path) -> Result<Self, OcrError> {
        Self::load_with_intra(path, 0)
    }

    /// Загрузить сессию с заданным числом intra-op потоков (0 = дефолт ORT = все ядра). При кадровой
    /// параллельности ставим 1, чтобы W воркеров не оверсабскрайбили ядра.
    pub fn load_with_intra(path: &Path, intra: usize) -> Result<Self, OcrError> {
        ensure_ort_dylib();
        let mut b = Session::builder().map_err(|e| OcrError::Ort(format!("ort builder: {e}")))?;
        if intra > 0 {
            b = b.with_intra_threads(intra).map_err(|e| OcrError::Ort(format!("intra_threads: {e}")))?;
        }
        let session = b
            .commit_from_file(path)
            .map_err(|e| OcrError::Ort(format!("commit_from_file {}: {e}", path.display())))?;
        Ok(Self { session })
    }

    /// Кастомная метадата модели по ключу "character" (словарь rec, как RapidOCR v3). None если ключа нет.
    pub fn metadata_character(&self) -> Result<Option<String>, OcrError> {
        let meta = self.session.metadata().map_err(|e| OcrError::Ort(format!("metadata: {e}")))?;
        Ok(meta.custom("character").map(|s| s.to_string()))
    }

    /// Прогнать [N,3,H,W] f32 -> (shape, данные) первого выхода.
    pub fn run(&mut self, input: Array4<f32>) -> Result<(Vec<usize>, Vec<f32>), OcrError> {
        // (shape, &data)-форма конструктора тензора (ArrayView-бонд капризен).
        let shape: Vec<i64> = input.shape().iter().map(|&d| d as i64).collect();
        let (data, _) = input.into_raw_vec_and_offset();
        let tensor = TensorRef::from_array_view((shape, data.as_slice()))
            .map_err(|e| OcrError::Ort(format!("tensor: {e}")))?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| OcrError::Ort(format!("run: {e}")))?;
        // первый выход по индексу.
        let (_, out) = outputs
            .iter()
            .next()
            .ok_or(OcrError::NoOutputs)?;
        let (shape, data) = out
            .try_extract_tensor::<f32>()
            .map_err(|e| OcrError::Ort(format!("extract: {e}")))?;
        let shape: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        Ok((shape, data.to_vec()))
    }
}
