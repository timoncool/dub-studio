//! SCRFD-10G-KPS (InsightFace buffalo_l det_10g.onnx) — детекция лиц + 5 лендмарок.
//!
//! Вход [1,3,640,640] f32 RGB NCHW, FIXED 640x640 (буфер buffalo_l экспортирован жёстко — letterbox
//! обязателен). Препроцесс = blobFromImage(1/128, mean 127.5, swapRB): (px-127.5)/128 в RGB.
//! 9 выходов: [0..2]=score, [3..5]=bbox, [6..8]=kps по страйдам {8,16,32}, 2 анкера на ячейку.
//! Декод: distance2bbox/kps (дельты * stride от центра анкера), затем /det_scale в исходные координаты.
//! Порог score ~0.5, NMS ~0.4. 5 точек: [0]левый глаз [1]правый глаз [2]нос [3]левый угол рта [4]правый.

use crate::FacesError;
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

/// Одно детектированное лицо: осевой bbox + 5 лендмарок (все в координатах ИСХОДНОГО кадра).
#[derive(Clone, Debug)]
pub struct Face {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub score: f32,
    /// 5 точек (x,y) в порядке InsightFace: левый глаз, правый глаз, нос, левый угол рта, правый угол рта.
    pub kps: [(f32, f32); 5],
}

impl Face {
    pub fn w(&self) -> f32 {
        (self.x2 - self.x1).max(0.0)
    }
    pub fn h(&self) -> f32 {
        (self.y2 - self.y1).max(0.0)
    }
    pub fn area(&self) -> f32 {
        self.w() * self.h()
    }
}

const INPUT: usize = 640;
const STRIDES: [usize; 3] = [8, 16, 32];
const NUM_ANCHORS: usize = 2;

/// Предупреждение о несовместимом билде SCRFD — печатаем ОДИН раз на процесс (иначе спам на каждом кадре).
fn warn_incompatible_scrfd() {
    use std::sync::Once;
    static WARN: Once = Once::new();
    WARN.call_once(|| {
        eprintln!(
            "dub-faces: incompatible SCRFD build (bbox/kps lengths do not match the anchor count), \
             stride(s) skipped; InsightFace det_10g.onnx is expected (bbox=n*4, kps=n*10)"
        );
    });
}

/// Детектор SCRFD: держит ONNX-сессию.
pub struct Scrfd {
    model: OnnxModel,
    score_thresh: f32,
    nms_thresh: f32,
}

impl Scrfd {
    pub fn load(onnx: &std::path::Path) -> Result<Self, FacesError> {
        Ok(Self {
            model: OnnxModel::load(onnx)?,
            score_thresh: 0.5,
            nms_thresh: 0.4,
        })
    }

    /// Детект лиц на кадре. Возвращает лица с bbox+kps в координатах исходного изображения.
    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Face>, FacesError> {
        let (ow, oh) = (img.width() as f32, img.height() as f32);
        if ow <= 0.0 || oh <= 0.0 {
            return Ok(Vec::new());
        }
        // letterbox: масштаб = min(640/w, 640/h); изображение кладётся в левый-верхний угол чёрного 640².
        let det_scale = (INPUT as f32 / ow).min(INPUT as f32 / oh);
        let nw = (ow * det_scale).round() as u32;
        let nh = (oh * det_scale).round() as u32;
        let resized = image::imageops::resize(
            img,
            nw.max(1),
            nh.max(1),
            image::imageops::FilterType::Triangle,
        );

        // blob NCHW: (px - 127.5)/128, RGB. Канва 640² чёрная (0 -> (0-127.5)/128).
        let mut blob = Array4::<f32>::from_elem((1, 3, INPUT, INPUT), (0.0 - 127.5) / 128.0);
        for y in 0..nh.min(INPUT as u32) {
            for x in 0..nw.min(INPUT as u32) {
                let p = resized.get_pixel(x, y);
                for c in 0..3 {
                    blob[[0, c, y as usize, x as usize]] = (p[c] as f32 - 127.5) / 128.0;
                }
            }
        }

        let outs = self.model.run(blob)?;
        if outs.len() < 9 {
            return Err(FacesError::OutputCount { model: "SCRFD", expected: 9, got: outs.len() });
        }

        // Сгруппировать выходы по последней размерности (1=score, 4=bbox, 10=kps) — устойчиво к
        // переименованию выходов в разных билдах. По страйду сортируем по числу анкеров (больше=меньше stride).
        let mut faces: Vec<Face> = Vec::new();
        for (si, &stride) in STRIDES.iter().enumerate() {
            // Позиционный контракт InsightFace: score[si], bbox[3+si], kps[6+si].
            let (sc_shape, sc) = &outs[si];
            let (_bb_shape, bb) = &outs[3 + si];
            let (_kp_shape, kp) = &outs[6 + si];
            let n = sc_shape.iter().product::<usize>(); // число анкеров этого страйда
            let grid = INPUT / stride; // 80/40/20
            // sanity: n должно быть grid*grid*num_anchors
            if n != grid * grid * NUM_ANCHORS {
                continue;
            }
            // Проверка длин bbox/kps ДО индексации: несовместимый билд SCRFD (иная раскладка выходов/число
            // kps) даёт bb.len()!=n*4 или kp.len()!=n*10 -> без проверки OOB-паника на КАЖДОМ кадре (тихий
            // полный отказ кастинга). Один warn на процесс, дальше молча пропускаем страйд.
            if bb.len() != n * 4 || kp.len() != n * 10 {
                warn_incompatible_scrfd();
                continue;
            }
            for idx in 0..n {
                let s = sc[idx];
                if s < self.score_thresh {
                    continue;
                }
                // центр анкера: (grid_x, grid_y) * stride; 2 анкера на ячейку делят центр.
                let cell = idx / NUM_ANCHORS;
                let gx = (cell % grid) as f32 * stride as f32;
                let gy = (cell / grid) as f32 * stride as f32;
                // distance2bbox: дельты (l,t,r,b) * stride.
                let l = bb[idx * 4] * stride as f32;
                let t = bb[idx * 4 + 1] * stride as f32;
                let r = bb[idx * 4 + 2] * stride as f32;
                let b = bb[idx * 4 + 3] * stride as f32;
                let (x1, y1, x2, y2) = (gx - l, gy - t, gx + r, gy + b);
                // distance2kps: 5 точек (dx,dy) * stride от центра анкера.
                let mut kps = [(0.0f32, 0.0f32); 5];
                for (k, kp_slot) in kps.iter_mut().enumerate() {
                    let dx = kp[idx * 10 + k * 2] * stride as f32;
                    let dy = kp[idx * 10 + k * 2 + 1] * stride as f32;
                    *kp_slot = ((gx + dx) / det_scale, (gy + dy) / det_scale);
                }
                faces.push(Face {
                    x1: x1 / det_scale,
                    y1: y1 / det_scale,
                    x2: x2 / det_scale,
                    y2: y2 / det_scale,
                    score: s,
                    kps,
                });
            }
        }

        Ok(nms(faces, self.nms_thresh))
    }
}

/// IoU двух лиц (по bbox).
pub(crate) fn iou(a: &Face, b: &Face) -> f32 {
    let xx1 = a.x1.max(b.x1);
    let yy1 = a.y1.max(b.y1);
    let xx2 = a.x2.min(b.x2);
    let yy2 = a.y2.min(b.y2);
    let w = (xx2 - xx1).max(0.0);
    let h = (yy2 - yy1).max(0.0);
    let inter = w * h;
    let uni = a.area() + b.area() - inter;
    if uni <= 0.0 {
        0.0
    } else {
        inter / uni
    }
}

/// Greedy NMS по score (по убыванию). Детерминированный tie-break по позиции.
pub(crate) fn nms(mut faces: Vec<Face>, thresh: f32) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<Face> = Vec::new();
    'outer: for f in faces {
        for k in &keep {
            if iou(&f, k) > thresh {
                continue 'outer;
            }
        }
        keep.push(f);
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x1: f32, y1: f32, x2: f32, y2: f32, s: f32) -> Face {
        Face { x1, y1, x2, y2, score: s, kps: [(0.0, 0.0); 5] }
    }

    #[test]
    fn nms_drops_overlapping_lower_score() {
        let a = face(0.0, 0.0, 10.0, 10.0, 0.9);
        let b = face(1.0, 1.0, 11.0, 11.0, 0.5); // сильно пересекается с a
        let c = face(100.0, 100.0, 110.0, 110.0, 0.8); // отдельно
        let out = nms(vec![a, b, c], 0.4);
        assert_eq!(out.len(), 2); // b подавлен
        assert!((out[0].score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn iou_disjoint_is_zero() {
        let a = face(0.0, 0.0, 10.0, 10.0, 1.0);
        let b = face(20.0, 20.0, 30.0, 30.0, 1.0);
        assert_eq!(iou(&a, &b), 0.0);
    }
}
