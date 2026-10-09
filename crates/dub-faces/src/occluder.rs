//! Детектор окклюзии лица (порт FaceFusion `face_masker` xseg): ONNX `[1,3,256,256]` -> маска
//! `[1,1,256,256]`. xseg сегментирует ВИДИМУЮ кожу лица; перекрытие (рука/предмет/микрофон/волосы) даёт
//! низкую маску в этой зоне. Для выбора аватара считаем «видимость» = средняя маска по ЦЕНТРУ кропа лица
//! -> штрафуем закрытые кадры (претензия юзера «лицо закрыто хуйней»).
//!
//! Препроцесс (FaceFusion): кроп лица -> resize 256² -> f32 /255, NCHW. Полярность: у FaceFusion высокая
//! маска = кожа лица (её КЕЕПят при блендинге), поэтому «видимость» = mean центральной маски (1 = чисто).
//! Env DUB_FACES_OCCLUDER_INVERT=1 инвертирует, если конкретный экспорт xseg отдаёт обратную полярность.

use crate::FacesError;
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;
use std::path::Path;

const SIZE: u32 = 256;

/// Путь к xseg-модели окклюдера: `<root>/faces/occluder/xseg_1.onnx`. Env DUB_FACES_OCCLUDER.
pub fn occluder_path(models_root: &Path) -> std::path::PathBuf {
    if let Ok(p) = std::env::var("DUB_FACES_OCCLUDER") {
        return std::path::PathBuf::from(p);
    }
    models_root.join("faces").join("occluder").join("xseg_1.onnx")
}

/// Окклюдер лица: держит ONNX-сессию xseg.
pub struct FaceOccluder {
    model: OnnxModel,
    invert: bool,
}

impl FaceOccluder {
    pub fn load(onnx: &Path) -> Result<Self, FacesError> {
        let invert = std::env::var("DUB_FACES_OCCLUDER_INVERT").ok().as_deref() == Some("1");
        Ok(Self { model: OnnxModel::load(onnx)?, invert })
    }

    /// Видимость лица в кропе [0..1]: доля «кожи лица» (xseg) в ЦЕНТРАЛЬНОЙ зоне (лицо в центре кропа).
    /// 1.0 = открыто/чисто, ~0 = сильно закрыто. `face_crop` — вырезанное квадратное лицо (с полями).
    pub fn visibility(&mut self, face_crop: &RgbImage) -> Result<f32, FacesError> {
        let resized = image::imageops::resize(face_crop, SIZE, SIZE, image::imageops::FilterType::Triangle);
        let mut blob = Array4::<f32>::zeros((1, 3, SIZE as usize, SIZE as usize));
        for y in 0..SIZE {
            for x in 0..SIZE {
                let p = resized.get_pixel(x, y);
                for c in 0..3 {
                    blob[[0, c, y as usize, x as usize]] = p[c] as f32 / 255.0;
                }
            }
        }
        let (shape, data) = self.model.run_single(blob)?;
        // выход [1,1,H,W] или [1,H,W]; берём среднее по центральной трети (лицо в центре кропа).
        // Гард: 4D с C!=1 (мульти-канальный или channel-last [1,H,W,C] экспорт xseg) — раскладка неизвестна,
        // data[yy*w+xx] прочитал бы мусор -> нейтральная видимость 1.0 (без штрафа), а не случайный.
        if shape.len() == 4 && shape[1] != 1 {
            return Ok(1.0);
        }
        let (h, w) = match shape.len() {
            4 => (shape[2], shape[3]),
            3 => (shape[1], shape[2]),
            2 => (shape[0], shape[1]),
            _ => (SIZE as usize, SIZE as usize),
        };
        if data.len() < h * w || h == 0 || w == 0 {
            return Ok(1.0); // не смогли прочитать -> не штрафуем
        }
        let (y0, y1) = (h / 4, h * 3 / 4);
        let (x0, x1) = (w / 4, w * 3 / 4);
        let mut sum = 0.0f64;
        let mut n = 0u64;
        for yy in y0..y1 {
            for xx in x0..x1 {
                sum += data[yy * w + xx].clamp(0.0, 1.0) as f64;
                n += 1;
            }
        }
        if n == 0 {
            return Ok(1.0);
        }
        let mean = (sum / n as f64) as f32;
        Ok(if self.invert { 1.0 - mean } else { mean })
    }
}
