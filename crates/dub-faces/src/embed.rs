//! Выравнивание лица (ArcFace 5-точечный similarity-transform -> 112x112) + эмбеддинг LVFace-L.
//!
//! Выравнивание: оценка подобия (масштаб+поворот+сдвиг) по 5 лендмаркам к эталонным точкам ArcFace
//! (стандарт InsightFace, 112x112). Umeyama без масштаба-по-осям (единый scale) — как estimateAffine
//! partial 2D в insightface.utils.face_align. LVFace: вход [1,3,112,112] RGB, (px/255-0.5)/0.5; выход
//! [1,512], L2-нормируем перед косинусом.

use crate::FacesError;
use crate::detect::Face;
use crate::ort_engine::OnnxModel;
use image::RgbImage;
use ndarray::Array4;

/// Эталонные 5 точек ArcFace для 112x112 (insightface arcface_dst).
const ARCFACE_DST: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];
const ALIGN: usize = 112;

/// Аффинное преобразование 2x3 (src -> dst): [[a,b,tx],[c,d,ty]].
type Affine = [[f32; 3]; 2];

/// Оценка similarity-transform (единый масштаб, поворот, сдвиг) по 5 парам точек. src — лендмарки лица
/// в кадре, dst — эталон ArcFace. Возвращает аффинную матрицу src->dst.
///
/// Замкнутая форма 2D-similarity МЕТОДОМ НАИМЕНЬШИХ КВАДРАТОВ (поворот+масштаб+сдвиг), БЕЗ отражения —
/// ровно как InsightFace face_align (cv2.estimateAffinePartial2D / skimage SimilarityTransform). Лицевые
/// 5-точки всегда в фиксированном порядке (Л-глаз, П-глаз, нос, Л-рот, П-рот), поэтому отражение никогда
/// не нужно; допускать его (каноничный Umeyama с D=diag(1,-1)) — источник зеркальных align на вырожденных
/// точках. θ=atan2(Σ(ax·by−ay·bx), Σ(ax·bx+ay·by)); scale=|(cos,sin)-сумма|/var_s; det R=+1 всегда.
fn umeyama(src: &[(f32, f32); 5], dst: &[(f32, f32); 5]) -> Affine {
    let n = 5.0f32;
    // средние
    let (mut sx, mut sy, mut dx, mut dy) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..5 {
        sx += src[i].0;
        sy += src[i].1;
        dx += dst[i].0;
        dy += dst[i].1;
    }
    let (sx, sy, dx, dy) = (sx / n, sy / n, dx / n, dy / n);
    // Скалярное и «векторное» произведения центрированных точек + дисперсия src.
    let (mut s_dot, mut s_cross, mut var_s) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..5 {
        let (ax, ay) = (src[i].0 - sx, src[i].1 - sy);
        let (bx, by) = (dst[i].0 - dx, dst[i].1 - dy);
        s_dot += ax * bx + ay * by;
        s_cross += ax * by - ay * bx;
        var_s += ax * ax + ay * ay;
    }
    let denom = (s_dot * s_dot + s_cross * s_cross).sqrt();
    // Поворот θ: cosθ=s_dot/denom, sinθ=s_cross/denom (при вырождении — тождество).
    let (cos, sin) = if denom > 1e-8 { (s_dot / denom, s_cross / denom) } else { (1.0, 0.0) };
    let scale = if var_s > 1e-8 { denom / var_s } else { 1.0 };
    // scale·R, R=[[c,-s],[s,c]] — поворот (det=+1), без отражения.
    let (r00, r01, r10, r11) = (scale * cos, -scale * sin, scale * sin, scale * cos);
    let tx = dx - (r00 * sx + r01 * sy);
    let ty = dy - (r10 * sx + r11 * sy);
    [[r00, r01, tx], [r10, r11, ty]]
}

/// Инверсия аффинного 2x3 (для обратного маппинга dst->src при семплинге).
fn invert(m: &Affine) -> Option<Affine> {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if det.abs() < 1e-8 {
        return None;
    }
    let inv_det = 1.0 / det;
    let a = m[1][1] * inv_det;
    let b = -m[0][1] * inv_det;
    let c = -m[1][0] * inv_det;
    let d = m[0][0] * inv_det;
    let tx = -(a * m[0][2] + b * m[1][2]);
    let ty = -(c * m[0][2] + d * m[1][2]);
    Some([[a, b, tx], [c, d, ty]])
}

/// Выровнять лицо в 112x112 RGB билинейным семплингом (обратный маппинг: для каждого пикселя dst берём
/// источник через inv(affine)). Точки вне кадра -> чёрный.
pub fn align_112(img: &RgbImage, face: &Face) -> RgbImage {
    let m = umeyama(&face.kps, &ARCFACE_DST);
    let inv = match invert(&m) {
        Some(v) => v,
        None => return RgbImage::new(ALIGN as u32, ALIGN as u32),
    };
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let mut out = RgbImage::new(ALIGN as u32, ALIGN as u32);
    for dy in 0..ALIGN {
        for dx in 0..ALIGN {
            let sx = inv[0][0] * dx as f32 + inv[0][1] * dy as f32 + inv[0][2];
            let sy = inv[1][0] * dx as f32 + inv[1][1] * dy as f32 + inv[1][2];
            let px = bilinear(img, sx, sy, iw, ih);
            out.put_pixel(dx as u32, dy as u32, image::Rgb(px));
        }
    }
    out
}

/// Билинейный семпл RGB из img по дробным координатам; вне границ -> чёрный.
fn bilinear(img: &RgbImage, x: f32, y: f32, iw: i32, ih: i32) -> [u8; 3] {
    if x < 0.0 || y < 0.0 || x > (iw - 1) as f32 || y > (ih - 1) as f32 {
        return [0, 0, 0];
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = (x0 + 1).min(iw - 1);
    let y1 = (y0 + 1).min(ih - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let mut out = [0u8; 3];
    for c in 0..3 {
        let p00 = img.get_pixel(x0 as u32, y0 as u32)[c] as f32;
        let p10 = img.get_pixel(x1 as u32, y0 as u32)[c] as f32;
        let p01 = img.get_pixel(x0 as u32, y1 as u32)[c] as f32;
        let p11 = img.get_pixel(x1 as u32, y1 as u32)[c] as f32;
        let top = p00 + (p10 - p00) * fx;
        let bot = p01 + (p11 - p01) * fx;
        out[c] = (top + (bot - top) * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// LVFace-L эмбеддер: ONNX-сессия [1,3,112,112] -> [1,512].
pub struct LvFace {
    model: OnnxModel,
}

impl LvFace {
    pub fn load(onnx: &std::path::Path) -> Result<Self, FacesError> {
        Ok(Self { model: OnnxModel::load(onnx)? })
    }

    /// Эмбеддинг для УЖЕ выровненного 112x112 RGB. Нормализация (px/255-0.5)/0.5. L2-норм на выходе.
    pub fn embed_aligned(&mut self, aligned: &RgbImage) -> Result<Vec<f32>, FacesError> {
        if aligned.width() != ALIGN as u32 || aligned.height() != ALIGN as u32 {
            return Err(FacesError::CropSize { width: aligned.width(), height: aligned.height() });
        }
        let mut blob = Array4::<f32>::zeros((1, 3, ALIGN, ALIGN));
        for y in 0..ALIGN {
            for x in 0..ALIGN {
                let p = aligned.get_pixel(x as u32, y as u32);
                for c in 0..3 {
                    blob[[0, c, y, x]] = (p[c] as f32 / 255.0 - 0.5) / 0.5;
                }
            }
        }
        let (_shape, data) = self.model.run_single(blob)?;
        Ok(l2_normalize(data))
    }

    /// Детект+выравнивание уже сделаны — эмбеддинг лица по кадру (align_112 + embed_aligned).
    pub fn embed_face(&mut self, img: &RgbImage, face: &Face) -> Result<Vec<f32>, FacesError> {
        let aligned = align_112(img, face);
        self.embed_aligned(&aligned)
    }
}

/// L2-нормализация вектора (нулевой -> как есть).
pub fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umeyama_identity_on_dst() {
        // src = dst -> преобразование ~единичное (scale~1, поворот~0, сдвиг~0).
        let m = umeyama(&ARCFACE_DST, &ARCFACE_DST);
        assert!((m[0][0] - 1.0).abs() < 1e-3, "scale x ~1, got {}", m[0][0]);
        assert!((m[1][1] - 1.0).abs() < 1e-3, "scale y ~1");
        assert!(m[0][2].abs() < 1e-2, "tx ~0");
        assert!(m[1][2].abs() < 1e-2, "ty ~0");
    }

    #[test]
    fn umeyama_scaled_translated() {
        // src = dst * 2 + (10, 20) -> восстановление: обратный transform масштабирует на 0.5.
        let mut src = ARCFACE_DST;
        for p in &mut src {
            p.0 = p.0 * 2.0 + 10.0;
            p.1 = p.1 * 2.0 + 20.0;
        }
        let m = umeyama(&src, &ARCFACE_DST);
        // применяем к первой точке src -> должна лечь на dst[0].
        let (x, y) = src[0];
        let tx = m[0][0] * x + m[0][1] * y + m[0][2];
        let ty = m[1][0] * x + m[1][1] * y + m[1][2];
        assert!((tx - ARCFACE_DST[0].0).abs() < 0.5, "aligned x, got {tx}");
        assert!((ty - ARCFACE_DST[0].1).abs() < 0.5, "aligned y, got {ty}");
    }

    #[test]
    fn umeyama_no_reflection_on_frontal() {
        // Нормальные лицевые точки (без зеркала) -> det аффинного > 0 (собственно поворот+масштаб,
        // не отражение). Проверяем, что выравнивание не «переворачивает» лицо.
        let m = umeyama(&ARCFACE_DST, &ARCFACE_DST);
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        assert!(det > 0.0, "фронтальные точки -> без отражения, det={det}");
    }

    #[test]
    fn umeyama_no_mirror_flip() {
        // Зеркально отражённый по X набор точек НЕ должен «чиниться» отражением: выравнивание лица —
        // строго similarity без reflection (стандарт ArcFace), det аффинного остаётся >0. Иначе крон
        // лица оказался бы зеркальным. Пара точек при этом НЕ ляжет идеально на dst — это ожидаемо.
        let mut src = ARCFACE_DST;
        for p in &mut src {
            p.0 = 112.0 - p.0; // отражение по вертикальной оси
        }
        let m = umeyama(&src, &ARCFACE_DST);
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        assert!(det > 0.0, "выравнивание без отражения, det={det}");
    }

    #[test]
    fn l2_norm_unit_length() {
        let v = l2_normalize(vec![3.0, 4.0]);
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-6);
    }

    #[test]
    fn invert_roundtrip() {
        let m: Affine = [[2.0, 0.0, 5.0], [0.0, 2.0, 7.0]];
        let inv = invert(&m).unwrap();
        // m(inv(p)) == p
        let (px, py) = (13.0f32, 21.0f32);
        let ix = inv[0][0] * px + inv[0][1] * py + inv[0][2];
        let iy = inv[1][0] * px + inv[1][1] * py + inv[1][2];
        let bx = m[0][0] * ix + m[0][1] * iy + m[0][2];
        let by = m[1][0] * ix + m[1][1] * iy + m[1][2];
        assert!((bx - px).abs() < 1e-4 && (by - py).abs() < 1e-4);
    }
}
