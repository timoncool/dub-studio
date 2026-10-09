//! Детектор НАРИСОВАННЫХ/аниме/мульт лиц — deepghs/anime_face_detection v1.4_s (YOLOv8, MIT).
//!
//! Вход [1,3,640,640] f32 RGB NCHW, letterbox (как SCRFD, чтобы просто размасштабировать боксы обратно),
//! препроцесс: px/255.0 (БЕЗ mean/std). Выход `output0` — YOLOv8 raw: [1, 5, N] ИЛИ [1, N, 5]
//! (5 = cx,cy,w,h,score; N ~ 8400 анкеров). Раскладку определяем по факту (какая ось == 5).
//! Декод: фильтр score >= conf, xywh(center)->xyxy, class-agnostic NMS, /det_scale в исходные координаты.
//! Ландмарок НЕТ (для аниме нет ONNX-модели точек) — kps в Face зануляем; выравнивание для CCIP не нужно
//! (CCIP эмбедит кроп всего персонажа целиком). Возвращает те же Face, что SCRFD, для единого пайплайна.

use crate::FacesError;
use crate::detect::{nms, Face};
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

const INPUT: usize = 640;

/// Порог уверенности детекции аниме-лица. Env DUB_ANIME_CONF (модель-карта v1.4_s: F1-оптимум ~0.30).
fn conf_threshold() -> f32 {
    std::env::var("DUB_ANIME_CONF").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.30)
}

/// IoU-порог NMS. Env DUB_ANIME_IOU (imgutils дефолт 0.7).
fn iou_threshold() -> f32 {
    std::env::var("DUB_ANIME_IOU").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.7)
}

/// Детектор аниме-лиц: держит ONNX-сессию YOLOv8.
pub struct AnimeFaceDetector {
    model: OnnxModel,
    conf: f32,
    iou: f32,
}

impl AnimeFaceDetector {
    pub fn load(onnx: &std::path::Path) -> Result<Self, FacesError> {
        Ok(Self { model: OnnxModel::load(onnx)?, conf: conf_threshold(), iou: iou_threshold() })
    }

    /// Детект нарисованных лиц. bbox в координатах исходного кадра; kps занулены (у аниме их нет).
    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Face>, FacesError> {
        let (ow, oh) = (img.width() as f32, img.height() as f32);
        if ow <= 0.0 || oh <= 0.0 {
            return Ok(Vec::new());
        }
        // letterbox в 640² (чёрный фон), масштаб = min(640/w, 640/h) — как SCRFD, простой обратный пересчёт.
        let det_scale = (INPUT as f32 / ow).min(INPUT as f32 / oh);
        let nw = ((ow * det_scale).round() as u32).max(1);
        let nh = ((oh * det_scale).round() as u32).max(1);
        let resized =
            image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle);
        // blob NCHW, px/255, RGB; паддинг 0 (чёрный).
        let mut blob = Array4::<f32>::zeros((1, 3, INPUT, INPUT));
        for y in 0..nh.min(INPUT as u32) {
            for x in 0..nw.min(INPUT as u32) {
                let p = resized.get_pixel(x, y);
                for c in 0..3 {
                    blob[[0, c, y as usize, x as usize]] = p[c] as f32 / 255.0;
                }
            }
        }
        let (shape, data) = self.model.run_single(blob)?;
        // shape = [1, d1, d2]. Ось «5» = каналы (cx,cy,w,h,score), другая = N анкеров.
        if shape.len() != 3 {
            return Err(FacesError::OutputShape { model: "anime face detector", shape });
        }
        let (d1, d2) = (shape[1], shape[2]);
        let (n, channels_first) = if d1 == 5 {
            (d2, true) // [1,5,N]
        } else if d2 == 5 {
            (d1, false) // [1,N,5]
        } else {
            return Err(FacesError::OutputShape { model: "anime face detector", shape });
        };
        // Доступ к каналу c анкера i с учётом раскладки.
        let at = |c: usize, i: usize| -> f32 {
            if channels_first {
                data[c * n + i]
            } else {
                data[i * 5 + c]
            }
        };
        let mut faces: Vec<Face> = Vec::with_capacity(64);
        for i in 0..n {
            let score = at(4, i);
            if score < self.conf {
                continue;
            }
            let (cx, cy, w, h) = (at(0, i), at(1, i), at(2, i), at(3, i));
            // xywh-center -> xyxy в 640-пространстве, затем /det_scale в исходные координаты.
            let x1 = (cx - w / 2.0) / det_scale;
            let y1 = (cy - h / 2.0) / det_scale;
            let x2 = (cx + w / 2.0) / det_scale;
            let y2 = (cy + h / 2.0) / det_scale;
            faces.push(Face { x1, y1, x2, y2, score, kps: [(0.0, 0.0); 5] });
        }
        Ok(nms(faces, self.iou))
    }
}
