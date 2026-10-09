//! ONNX-движки кастинга через ort (load-dynamic, та же 1.28.2, что dub-asr/dub-ocr). SCRFD (детект лиц,
//! 9 выходов) + LVFace-L (эмбеддинг лица, [1,512]). ensure_ort_dylib() — копия паттерна dub-ocr: без
//! явного ORT_DYLIB_PATH ort цепляет чужую system32\onnxruntime.dll (1.17) -> ДЕДЛОК при создании сессии.

use crate::FacesError;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Гарантировать правильную onnxruntime.dll (1.28.2). Порт dub_ocr::ensure_ort_dylib (тот же поиск).
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
        if let Some(p) = std::env::var_os("DUB_FACES_ORT_DYLIB") {
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
                cands.push(dir.join(dub_core::runtime::ORT_LIBRARY));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("models"));
        }
        for r in &roots {
            cands.push(r.join("runtime").join("onnxruntime-1.28.dll"));
            cands.extend(dub_core::runtime::ort_candidates(r));
        }
        for c in cands {
            if c.is_file() {
                std::env::set_var("ORT_DYLIB_PATH", &c);
                return;
            }
        }
    });
}

/// Обёртка над ort::Session с одним входом [N,3,H,W] f32. Отдаёт ВСЕ выходы (SCRFD = 9 тензоров).
pub struct OnnxModel {
    session: Session,
}

impl OnnxModel {
    pub fn load(path: &Path) -> Result<Self, FacesError> {
        ensure_ort_dylib();
        let session = Session::builder()
            .map_err(|e| FacesError::Ort(format!("ort builder: {e}")))?
            .commit_from_file(path)
            .map_err(|e| FacesError::Ort(format!("commit_from_file {}: {e}", path.display())))?;
        Ok(Self { session })
    }

    /// Прогнать [N,3,H,W] f32 -> первый выход (shape, данные). Для одно-выходных моделей (LVFace).
    pub fn run_single(&mut self, input: Array4<f32>) -> Result<(Vec<usize>, Vec<f32>), FacesError> {
        let mut outs = self.run(input)?;
        if outs.is_empty() {
            return Err(FacesError::NoOutputs);
        }
        Ok(outs.swap_remove(0))
    }

    /// Прогнать тензор ПРОИЗВОЛЬНОЙ формы (row-major flat) -> первый выход (shape, данные). Нужен для
    /// WeSpeaker: вход [1,T,80] (3-D, не 4-D как у SCRFD/LVFace). shape задаёт вызывающий.
    pub fn run_shape(
        &mut self,
        shape: &[usize],
        data: Vec<f32>,
    ) -> Result<(Vec<usize>, Vec<f32>), FacesError> {
        let dims: Vec<i64> = shape.iter().map(|&d| d as i64).collect();
        let tensor = TensorRef::from_array_view((dims, data.as_slice()))
            .map_err(|e| FacesError::Ort(format!("tensor: {e}")))?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| FacesError::Ort(format!("run: {e}")))?;
        let (_name, v) = outputs.iter().next().ok_or(FacesError::NoOutputs)?;
        let (oshape, odata) = v
            .try_extract_tensor::<f32>()
            .map_err(|e| FacesError::Ort(format!("extract: {e}")))?;
        let oshape: Vec<usize> = oshape.iter().map(|&d| d as usize).collect();
        Ok((oshape, odata.to_vec()))
    }

    /// Удобная обёртка над run_shape для 3-D входа [b,t,mel] (WeSpeaker).
    pub fn run_3d(&mut self, shape: &[usize; 3], data: Vec<f32>) -> Result<(Vec<usize>, Vec<f32>), FacesError> {
        self.run_shape(shape, data)
    }

    /// Прогнать [N,3,H,W] f32 -> ВСЕ выходы в ПОЗИЦИОННОМ порядке модели [(shape, данные), …].
    /// SCRFD: outputs[0..2]=scores, [3..5]=bbox, [6..8]=kps (фиксированный порядок InsightFace).
    pub fn run(&mut self, input: Array4<f32>) -> Result<Vec<(Vec<usize>, Vec<f32>)>, FacesError> {
        // (shape, &data)-форма конструктора тензора (ArrayView-бонд капризен).
        let shape: Vec<i64> = input.shape().iter().map(|&d| d as i64).collect();
        let (data, _) = input.into_raw_vec_and_offset();
        let tensor = TensorRef::from_array_view((shape, data.as_slice()))
            .map_err(|e| FacesError::Ort(format!("tensor: {e}")))?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| FacesError::Ort(format!("run: {e}")))?;
        // Выходы в позиционном порядке модели (SessionOutputs.iter() сохраняет порядок session.outputs) —
        // SCRFD-билды по-разному НАЗЫВАЮТ выходы, но порядок (3 score, 3 bbox, 3 kps) стабилен.
        let mut result = Vec::new();
        for (_name, v) in outputs.iter() {
            let (shape, data) = v
                .try_extract_tensor::<f32>()
                .map_err(|e| FacesError::Ort(format!("extract: {e}")))?;
            let shape: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            result.push((shape, data.to_vec()));
        }
        Ok(result)
    }
}
