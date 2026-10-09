//! DBNet-детекция текста (PP-OCRv5 det mobile). Порт препроцесса и DBPostProcess из RapidOCR/PP-OCR
//! с ДЕФОЛТНЫМИ порогами config.yaml: limit_type=min, limit_side_len=736, mean/std=0.5,
//! thresh=0.3, box_thresh=0.5, unclip_ratio=1.6, use_dilation=true, score_mode=fast.
//!
//! Постпроцесс: порог по карте вероятностей -> дилатация -> связные компоненты -> min-area-rect ->
//! box_score_fast -> unclip (истинный edge-normal offset, geometry::unclip) -> осевой bbox в
//! координатах исходного кадра. Именно edge-normal unclip (а не радиальный от центроида) равномерно
//! растит тонкие широкие боксы сабов по высоте — без него кроп резал глифы и rec шумел.

use crate::OcrError;
use crate::geometry::{box_score_fast, connected_components, min_area_rect, unclip, Point};
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

/// Бокс детекции в координатах исходного кадра.
#[derive(Clone, Copy, Debug)]
pub struct DetBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Пороги DBNet (дефолты RapidOCR config.yaml, секция Det).
#[derive(Clone, Copy, Debug)]
pub struct DetParams {
    pub limit_side_len: u32,
    pub thresh: f32,
    pub box_thresh: f32,
    pub unclip_ratio: f32,
    pub use_dilation: bool,
    pub min_size: f32,
    pub max_candidates: usize,
}

impl Default for DetParams {
    fn default() -> Self {
        Self {
            limit_side_len: 736,
            thresh: 0.3,
            box_thresh: 0.5,
            unclip_ratio: 1.6,
            use_dilation: true,
            min_size: 3.0,
            max_candidates: 1000,
        }
    }
}

/// Изменённый размер, кратный 32, по правилу limit_type=min: короткая сторона >= limit_side_len.
fn resize_shape(w: u32, h: u32, limit_side_len: u32) -> (u32, u32) {
    let (w, h) = (w as f32, h as f32);
    let min_side = w.min(h);
    let ratio = if min_side < limit_side_len as f32 {
        limit_side_len as f32 / min_side
    } else {
        1.0
    };
    let round32 = |v: f32| {
        let r = (v / 32.0).round().max(1.0) * 32.0;
        r as u32
    };
    (round32(w * ratio), round32(h * ratio))
}

/// Дилатация 3x3 бинарной карты (эквивалент cv2.dilate ядром np.ones((2,2)); берём 3x3 как
/// консервативное расширение — PP-OCR применяет dilation чтобы соединить разорванные штрихи).
fn dilate3(bitmap: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut v = 0u8;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                        v |= bitmap[ny as usize * w + nx as usize];
                    }
                }
            }
            out[y * w + x] = v;
        }
    }
    out
}

/// Прогнать детектор на RGB-кадре -> список боксов (x,y,w,h) в координатах исходного кадра.
/// Порт det-части _lines (PP-OCR det + DBPostProcess).
pub fn detect(model: &mut OnnxModel, img: &RgbImage, p: &DetParams) -> Result<Vec<DetBox>, OcrError> {
    let (src_w, src_h) = (img.width(), img.height());
    let (rw, rh) = resize_shape(src_w, src_h, p.limit_side_len);
    // препроцесс: билинейный ресайз в (rw, rh), нормализация (x/255 - mean)/std, mean=std=0.5.
    let mut input = Array4::<f32>::zeros((1, 3, rh as usize, rw as usize));
    let sx = src_w as f32 / rw as f32;
    let sy = src_h as f32 / rh as f32;
    for oy in 0..rh as usize {
        let fy = ((oy as f32 + 0.5) * sy - 0.5).clamp(0.0, src_h as f32 - 1.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(src_h as usize - 1);
        let wy = fy - y0 as f32;
        for ox in 0..rw as usize {
            let fx = ((ox as f32 + 0.5) * sx - 0.5).clamp(0.0, src_w as f32 - 1.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(src_w as usize - 1);
            let wx = fx - x0 as f32;
            // 4 соседних пикселя читаем один раз на выходную ячейку, канал индексируем внутри.
            let p00 = img.get_pixel(x0 as u32, y0 as u32);
            let p10 = img.get_pixel(x1 as u32, y0 as u32);
            let p01 = img.get_pixel(x0 as u32, y1 as u32);
            let p11 = img.get_pixel(x1 as u32, y1 as u32);
            for c in 0..3 {
                let top = p00[c] as f32 * (1.0 - wx) + p10[c] as f32 * wx;
                let bot = p01[c] as f32 * (1.0 - wx) + p11[c] as f32 * wx;
                let val = top * (1.0 - wy) + bot * wy;
                input[[0, c, oy, ox]] = (val / 255.0 - 0.5) / 0.5;
            }
        }
    }

    let (shape, flat) = model.run(input)?;
    // форма (1,1,ph,pw) -> вероятностная карта
    let (ph, pw) = (shape[shape.len() - 2], shape[shape.len() - 1]);

    // порог -> бинарная карта
    let bitmap: Vec<u8> = flat[..pw * ph].iter().map(|&v| (v > p.thresh) as u8).collect();
    let bitmap = if p.use_dilation {
        dilate3(&bitmap, pw, ph)
    } else {
        bitmap
    };

    let comps = connected_components(&bitmap, pw, ph);
    // масштаб карты -> исходный кадр
    let scale_x = src_w as f32 / pw as f32;
    let scale_y = src_h as f32 / ph as f32;

    let mut boxes: Vec<DetBox> = Vec::new();
    for comp in comps.into_iter().take(p.max_candidates.max(1) * 4) {
        if comp.len() < 4 {
            continue;
        }
        let pts: Vec<Point> = comp
            .iter()
            .map(|&idx| ((idx % pw) as f32, (idx / pw) as f32))
            .collect();
        let quad = min_area_rect(&pts);
        if quad.min_side() < p.min_size {
            continue;
        }
        // score по карте вероятностей (в координатах карты)
        let score = box_score_fast(&flat, pw, ph, &quad);
        if score < p.box_thresh {
            continue;
        }
        let expanded = unclip(&quad, p.unclip_ratio);
        if expanded.min_side() < p.min_size + 2.0 {
            continue;
        }
        let (x0, y0, x1, y1) = expanded.aabb();
        let bx = (x0 * scale_x).max(0.0);
        let by = (y0 * scale_y).max(0.0);
        let bw = ((x1 - x0) * scale_x).min(src_w as f32 - bx);
        let bh = ((y1 - y0) * scale_y).min(src_h as f32 - by);
        if bw >= 1.0 && bh >= 1.0 {
            boxes.push(DetBox {
                x: bx,
                y: by,
                w: bw,
                h: bh,
            });
        }
        if boxes.len() >= p.max_candidates {
            break;
        }
    }
    Ok(boxes)
}
