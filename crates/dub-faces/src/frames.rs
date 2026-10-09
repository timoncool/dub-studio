//! Семплинг кадров видео через ffmpeg (~1.5 fps по умолчанию) в PNG. Свой ffmpeg-вызов (media-хелперы
//! dub-server сюда не тянем — крейт не должен зависеть от сервера). Кадры кладутся в out_dir как
//! frame_%06d.png; время i-го кадра = i / fps.

use crate::FacesError;
use image::RgbImage;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG: &str = "ffmpeg";

/// Частота семплинга кадров кастинга по умолчанию (кадр/с). Лиц достаточно и на 1.5 fps; выше — лишний
/// счёт SCRFD/LVFace. Env DUB_FACES_FPS.
pub const DEFAULT_FPS: f64 = 1.5;

/// Один семплированный кадр: время + загруженное RGB-изображение.
pub struct SampledFrame {
    pub t: f64,
    pub img: RgbImage,
    pub path: PathBuf,
}

/// Что делать с PNG кадра после обработки в потоковом режиме: удалить (кадр не нужен) или оставить на
/// диске (кадр — кандидат в аватарку выбранного лица).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameDisposition {
    Delete,
    Keep,
}

/// Запустить ffmpeg-экстракцию кадров (PNG frame_%06d.png @ fps) в out_dir. Возвращает fps сетки.
/// Отдельно от чтения, чтобы потоковый обход мог декодировать кадры по одному.
fn extract_frames(video: &Path, out_dir: &Path, fps: f64) -> Result<f64, FacesError> {
    std::fs::create_dir_all(out_dir).map_err(|e| FacesError::Io(format!("frames dir {}: {e}", out_dir.display())))?;
    let fps = if fps > 0.0 { fps } else { DEFAULT_FPS };
    let pattern = out_dir.join("frame_%06d.png");
    let out = Command::new(FFMPEG)
        .arg("-y")
        .arg("-i")
        .arg(video)
        .args(["-vf", &format!("fps={fps}"), "-vsync", "0"])
        .arg(&pattern)
        .output()
        .map_err(|e| FacesError::Ffmpeg(e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.chars().rev().take(1200).collect::<String>().chars().rev().collect();
        return Err(FacesError::FfmpegExit { code: out.status.code(), tail });
    }
    Ok(fps)
}

/// ПОТОКОВЫЙ обход кадров (#115 перф/OOM): декодирует PNG ПО ОДНОМУ и сразу отдаёт в `on_frame`,
/// НЕ накапливая Vec<SampledFrame> (на 90мин@1.5fps ~8100 кадров = гигабайты RAM/диска). Каждый PNG
/// удаляется сразу после обработки, КРОМЕ тех, для которых `on_frame` вернул Keep (кандидаты в аватарки —
/// перечитываются позже по пути). `on_frame` возвращает Ok(Keep/Delete) или Err (фатально — прерываем).
/// Возвращает число обработанных кадров.
pub fn stream_frames<F>(video: &Path, out_dir: &Path, fps: f64, mut on_frame: F) -> Result<usize, FacesError>
where
    F: FnMut(&SampledFrame) -> Result<FrameDisposition, FacesError>,
{
    let fps = extract_frames(video, out_dir, fps)?;
    let mut idx = 1usize;
    let mut processed = 0usize;
    loop {
        let p = out_dir.join(format!("frame_{idx:06}.png"));
        if !p.is_file() {
            break;
        }
        let im = match image::open(&p) {
            Ok(im) => im.to_rgb8(),
            Err(_) => break,
        };
        let t = (idx - 1) as f64 / fps;
        let frame = SampledFrame { t, img: im, path: p.clone() };
        let disp = on_frame(&frame)?;
        // Освобождаем RAM кадра до удаления файла.
        drop(frame);
        if disp == FrameDisposition::Delete {
            let _ = std::fs::remove_file(&p);
        }
        processed += 1;
        idx += 1;
    }
    Ok(processed)
}

/// Извлечь кадры видео с частотой fps в out_dir (PNG), вернуть список (время, изображение, путь).
/// Детерминированно: fps фиксирует сетку, время i-го кадра = i/fps. Пустой результат = нет видео/кадров.
/// ВНИМАНИЕ: держит ВСЕ кадры в RAM — для длинного видео используйте stream_frames (потоковый обход).
pub fn sample_frames(video: &Path, out_dir: &Path, fps: f64) -> Result<Vec<SampledFrame>, FacesError> {
    let fps = extract_frames(video, out_dir, fps)?;
    // Собрать по порядку frame_000001.png … время = (i-1)/fps.
    let mut frames: Vec<SampledFrame> = Vec::new();
    let mut idx = 1usize;
    loop {
        let p = out_dir.join(format!("frame_{idx:06}.png"));
        if !p.is_file() {
            break;
        }
        match image::open(&p) {
            Ok(im) => {
                let t = (idx - 1) as f64 / fps;
                frames.push(SampledFrame { t, img: im.to_rgb8(), path: p });
            }
            Err(_) => break,
        }
        idx += 1;
    }
    Ok(frames)
}

/// Резкость кропа лица (variance of Laplacian по grayscale) — метрика ФОКУСА для выбора аватарки.
/// bbox — в координатах кадра; клэмпится к границам. Пустой/вырожденный кроп -> 0.
///
/// ВАЖНО: кроп НОРМАЛИЗУЕТСЯ к фиксированному 128×128 ДО Лапласа. Иначе variance растёт с размером лица
/// (у крупного расфокус-лица было бы больше, чем у мелкого резкого) — и большое мыльное лицо побеждало
/// резкое поменьше (баг мыльных аватаров). После нормализации это истинная мера фокуса, сравнимая между
/// лицами любого размера.
pub fn crop_sharpness(img: &RgbImage, bbox: (f32, f32, f32, f32)) -> f32 {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let x1 = bbox.0.max(0.0) as i32;
    let y1 = bbox.1.max(0.0) as i32;
    let x2 = (bbox.2 as i32).min(iw);
    let y2 = (bbox.3 as i32).min(ih);
    if x2 - x1 < 3 || y2 - y1 < 3 {
        return 0.0;
    }
    // Кроп лица -> ресайз к NORM² (Triangle). Focus-мера считается на нормализованном кропе.
    const NORM: u32 = 128;
    let sub = image::imageops::crop_imm(img, x1 as u32, y1 as u32, (x2 - x1) as u32, (y2 - y1) as u32).to_image();
    let g = image::imageops::resize(&sub, NORM, NORM, image::imageops::FilterType::Triangle);
    let (w, h) = (NORM as usize, NORM as usize);
    let mut gray = vec![0.0f32; w * h];
    for yy in 0..h {
        for xx in 0..w {
            let p = g.get_pixel(xx as u32, yy as u32);
            gray[yy * w + xx] = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
        }
    }
    // Лаплас 4-соседей, дисперсия отклика.
    let mut vals: Vec<f32> = Vec::with_capacity((w - 2) * (h - 2));
    for yy in 1..h - 1 {
        for xx in 1..w - 1 {
            let c = gray[yy * w + xx];
            let lap = gray[(yy - 1) * w + xx] + gray[(yy + 1) * w + xx] + gray[yy * w + xx - 1]
                + gray[yy * w + xx + 1]
                - 4.0 * c;
            vals.push(lap);
        }
    }
    if vals.is_empty() {
        return 0.0;
    }
    let mean = vals.iter().sum::<f32>() / vals.len() as f32;
    vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / vals.len() as f32
}

/// Вырезать аватарку ЛИЦА из кадра по bbox и сохранить PNG. `pad_frac` — доля расширения вокруг лица
/// (0.35 = +35% с каждой стороны: захватить лоб/волосы/подбородок). Кроп КВАДРАТНЫЙ (сторона = бОльшая
/// сторона лица × (1+2·pad)), центрирован на лице, клэмпится к границам кадра — карточка-аватарка ровная.
/// Без кропа аватар был бы целым кадром (руки/фон), а не лицом (#115-баг).
pub fn save_face_crop(frame_path: &Path, bbox: (f32, f32, f32, f32), pad_frac: f32, out: &Path) -> Result<(), FacesError> {
    let img = image::open(frame_path).map_err(|e| FacesError::Io(format!("open frame {}: {e}", frame_path.display())))?.to_rgb8();
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let (x1, y1, x2, y2) = bbox;
    let (bw, bh) = ((x2 - x1).max(1.0), (y2 - y1).max(1.0));
    let (cx, cy) = (x1 + bw / 2.0, y1 + bh / 2.0);
    // квадратная сторона с полями, но не больше кадра.
    let side = (bw.max(bh) * (1.0 + 2.0 * pad_frac)).min(iw).min(ih);
    // левый/верхний угол, зажатый так, чтобы квадрат целиком влезал в кадр.
    let sx = (cx - side / 2.0).clamp(0.0, iw - side).max(0.0) as u32;
    let sy = (cy - side / 2.0).clamp(0.0, ih - side).max(0.0) as u32;
    let sw = (side as u32).max(1).min(img.width() - sx);
    let sh = (side as u32).max(1).min(img.height() - sy);
    let crop = image::imageops::crop_imm(&img, sx, sy, sw, sh).to_image();
    crop.save(out).map_err(|e| FacesError::Io(format!("save avatar {}: {e}", out.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharpness_uniform_is_zero() {
        // Плоское серое изображение -> лаплас нулевой -> дисперсия 0.
        let img = RgbImage::from_pixel(20, 20, image::Rgb([128, 128, 128]));
        let s = crop_sharpness(&img, (0.0, 0.0, 20.0, 20.0));
        assert!(s < 1e-3, "плоское -> резкость ~0, got {s}");
    }

    #[test]
    fn sharpness_checker_is_positive() {
        // Шахматка -> высокий лаплас -> резкость > 0.
        let mut img = RgbImage::new(20, 20);
        for y in 0..20u32 {
            for x in 0..20u32 {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                img.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }
        let s = crop_sharpness(&img, (0.0, 0.0, 20.0, 20.0));
        assert!(s > 100.0, "шахматка -> высокая резкость, got {s}");
    }

    #[test]
    fn degenerate_crop_zero() {
        let img = RgbImage::new(20, 20);
        assert_eq!(crop_sharpness(&img, (0.0, 0.0, 1.0, 1.0)), 0.0);
    }
}
