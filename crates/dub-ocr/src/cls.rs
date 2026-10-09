//! Классификатор ориентации строки (ch_ppocr_mobile_v2.0_cls). Определяет поворот на 180°.
//! Дефолты config.yaml: cls_image_shape=[3,48,192], cls_thresh=0.9, label_list=["0","180"].

use crate::OcrError;
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

const CLS_H: usize = 48;
const CLS_W: usize = 192;
const CLS_THRESH: f32 = 0.9;

/// Вернуть true, если строку надо перевернуть на 180° (метка "180" с уверенностью >= порога).
/// `crop` — RGB-кроп вырезанной строки.
pub fn should_rotate180(model: &mut OnnxModel, crop: &RgbImage) -> Result<bool, OcrError> {
    let (cw, ch) = (crop.width() as usize, crop.height() as usize);
    if cw == 0 || ch == 0 {
        return Ok(false);
    }
    // ресайз до высоты 48 с сохранением аспекта, паддинг до ширины 192 (как PP-OCR ClsPostProcess).
    let ratio = cw as f32 / ch as f32;
    let rw = ((CLS_H as f32 * ratio).ceil() as usize).min(CLS_W).max(1);
    let mut input = Array4::<f32>::zeros((1, 3, CLS_H, CLS_W));
    let sx = cw as f32 / rw as f32;
    let sy = ch as f32 / CLS_H as f32;
    for oy in 0..CLS_H {
        let fy = ((oy as f32 + 0.5) * sy - 0.5).clamp(0.0, ch as f32 - 1.0);
        let y = fy.round() as usize;
        for ox in 0..rw {
            let fx = ((ox as f32 + 0.5) * sx - 0.5).clamp(0.0, cw as f32 - 1.0);
            let x = fx.round() as usize;
            let px = crop.get_pixel(x as u32, y as u32); // один get_pixel на ячейку вместо трёх
            for c in 0..3 {
                let v = px[c] as f32 / 255.0;
                input[[0, c, oy, ox]] = (v - 0.5) / 0.5;
            }
        }
    }
    let (_shape, v) = model.run(input)?;
    // 2 логита softmax-выхода: [p("0"), p("180")].
    if v.len() < 2 {
        return Ok(false);
    }
    Ok(v[1] >= CLS_THRESH && v[1] > v[0])
}
