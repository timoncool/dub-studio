//! CCIP — эмбеддер НАРИСОВАННОГО персонажа (deepghs/ccip_onnx, ccip-caformer-24-randaug-pruned, OpenRAIL).
//! Аниме-аналог ArcFace: «тот же ли это персонаж» между кадрами/эпизодами.
//!
//! ВАЖНО: CCIP эмбедит КРОП ВСЕГО ПЕРСОНАЖА (лицо+волосы+одежда), НЕ голое лицо — так обучен (лицо аниме
//! низко-информативно, идентичность несёт весь дизайн). У нас нет person-детектора, поэтому character-crop
//! аппроксимируем расширением bbox лица (вверх — волосы, сильно вниз — тело).
//!
//! Вход `input` [1,3,384,384] f32 RGB NCHW: resize BILINEAR до 384², /255, затем нормализация CLIP
//! mean/std. Выход `output` [1,768] — метрика ЕВКЛИД (L2) на СЫРЫХ векторах (НЕ L2-нормировать!),
//! «тот же персонаж» при dist < ~0.178. Косинус НЕ использовать: пороги CCIP калиброваны под L2.

use crate::FacesError;
use crate::detect::Face;
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

const SIZE: usize = 384;
/// Размерность эмбеддинга CCIP (caformer-24).
pub const CCIP_DIM: usize = 768;
/// CLIP-нормализация (ccip.py imgutils): mean/std.
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

/// Порог L2-расстояния «тот же персонаж» для ccip-caformer-24 (F1-оптимум metrics.json). Env DUB_CCIP_THRESH.
pub fn ccip_threshold() -> f32 {
    std::env::var("DUB_CCIP_THRESH").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.178475)
}

/// Резолв пути к CCIP feat-модели (<root>/faces/ccip/model_feat.onnx). Env-override DUB_CCIP_MODEL.
pub fn ccip_path(models_root: &std::path::Path) -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("DUB_CCIP_MODEL") {
        return std::path::PathBuf::from(p);
    }
    models_root.join("faces").join("ccip").join("model_feat.onnx")
}

/// Character-crop из bbox лица: расширить вверх 0.6·h (волосы), вниз 3.0·h (тело), вбок 1.0·w. Клэмп к кадру.
/// CCIP эмбедит персонажа целиком, а не лицо. (x1,y1,x2,y2) в координатах кадра.
pub fn character_crop_bbox(face: &Face, iw: f32, ih: f32) -> (u32, u32, u32, u32) {
    let (fw, fh) = ((face.x2 - face.x1).max(1.0), (face.y2 - face.y1).max(1.0));
    let x1 = (face.x1 - fw * 1.0).clamp(0.0, iw - 1.0);
    let x2 = (face.x2 + fw * 1.0).clamp(0.0, iw);
    let y1 = (face.y1 - fh * 0.6).clamp(0.0, ih - 1.0);
    let y2 = (face.y2 + fh * 3.0).clamp(0.0, ih);
    (x1 as u32, y1 as u32, (x2 - x1).max(1.0) as u32, (y2 - y1).max(1.0) as u32)
}

/// CCIP-эмбеддер: держит ONNX-сессию.
pub struct CcipEmbedder {
    model: OnnxModel,
}

impl CcipEmbedder {
    pub fn load(onnx: &std::path::Path) -> Result<Self, FacesError> {
        Ok(Self { model: OnnxModel::load(onnx)? })
    }

    /// Эмбеддинг персонажа по КРОПУ (уже вырезанному character-crop). 768-d СЫРОЙ вектор (без L2).
    pub fn embed(&mut self, crop: &RgbImage) -> Result<Vec<f32>, FacesError> {
        let resized = image::imageops::resize(
            crop,
            SIZE as u32,
            SIZE as u32,
            image::imageops::FilterType::Triangle,
        );
        let mut blob = Array4::<f32>::zeros((1, 3, SIZE, SIZE));
        for y in 0..SIZE {
            for x in 0..SIZE {
                let p = resized.get_pixel(x as u32, y as u32);
                for c in 0..3 {
                    blob[[0, c, y, x]] = (p[c] as f32 / 255.0 - MEAN[c]) / STD[c];
                }
            }
        }
        let (shape, data) = self.model.run_single(blob)?;
        let n: usize = shape.iter().product();
        if n != CCIP_DIM {
            return Err(FacesError::OutputShape { model: "CCIP", shape });
        }
        Ok(data) // СЫРОЙ вектор — НЕ нормируем (L2-метрика на сырых)
    }

    /// Удобно: вырезать character-crop из кадра по лицу и сразу эмбеддить.
    pub fn embed_character(&mut self, frame: &RgbImage, face: &Face) -> Result<Vec<f32>, FacesError> {
        let (iw, ih) = (frame.width() as f32, frame.height() as f32);
        let (x, y, w, h) = character_crop_bbox(face, iw, ih);
        let crop = image::imageops::crop_imm(frame, x, y, w, h).to_image();
        self.embed(&crop)
    }
}

/// Евклидово (L2) расстояние между двумя CCIP-векторами (метрика CCIP; меньше = похожее).
pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_zero_for_identical() {
        let v = vec![0.1, -0.2, 0.3];
        assert!(l2_distance(&v, &v).abs() < 1e-6);
    }

    #[test]
    fn l2_mismatch_len_is_max() {
        assert_eq!(l2_distance(&[1.0], &[1.0, 2.0]), f32::MAX);
    }

    #[test]
    fn char_crop_wider_and_taller_than_face() {
        let f = Face { x1: 100.0, y1: 100.0, x2: 150.0, y2: 160.0, score: 0.9, kps: [(0.0, 0.0); 5] };
        let (x, y, w, h) = character_crop_bbox(&f, 640.0, 480.0);
        assert!(w > 50 && h > 60, "кроп персонажа шире/выше лица: {w}x{h}");
        assert!(x < 100 && y < 100, "кроп начинается левее/выше лица");
    }
}
