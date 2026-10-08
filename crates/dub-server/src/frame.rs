//! preview_frame / source_frame (порт api.preview_frame / api.source_frame). CPU-дёшево, БЕЗ моделей:
//! preview заново собирает .ass из ТЕКУЩЕГО (отредактированного) Project и композитит ОДИН кадр
//! (blur-боксы + ASS-оверлей) через ffmpeg input-seek — тот же build_ass, что и полный рендер (WYSIWYG-
//! паритет), плюс burn_frame из dub-captions. source_frame — сырой кадр оригинала (для before/after).
//!
//! Обе функции синхронные и тяжеловаты только на ffmpeg — гоняются через GPU-воркер (как в app.py:
//! serialized preview_frame), возвращают PNG-байты.

use dub_core::Project;
use std::path::Path;

use crate::render::build_ass;

/// Собрать ОДИН превью-кадр (JPEG-байты) на времени t. Порт api.preview_frame: build ASS из текущего
/// Project + burn_frame (blur-боксы + ASS). fonts_dir нужен libass/измерению глифов. JPEG (не PNG) —
/// кодируется/качается/декодится в разы быстрее для фотокадра (замер: 0.07с vs 0.10с, 53КБ vs 674КБ).
/// lowres=true (плей) на БОЛЬШИХ видео рендерит кадр в низком разрешении (быстрее ~кратно площади) —
/// геометрия сабов/боксов сохраняется; на паузе/малых видео — полный размер для точных правок.
pub fn preview_frame(
    proj: &Project,
    input: &Path,
    work_dir: &Path,
    fonts_dir: &Path,
    t: f64,
    lowres: bool,
) -> Result<Vec<u8>, String> {
    dub_captions::set_fonts_dir(fonts_dir);
    let vw = proj.meta.width;
    let vh = proj.meta.height;
    let total = proj.meta.duration;
    // Даунскейл только при плее И только для больших видео (длинная сторона > 1280) — цель ~960 по длинной
    // стороне. Малые/паузные кадры идут в полном разрешении (WYSIWYG для правок боксов).
    let long = vw.max(vh);
    let scale_w: Option<i64> = if lowres && long > 1280 {
        let k = 960.0 / long as f64;
        Some(((vw as f64 * k / 2.0).round() as i64 * 2).max(2))
    } else {
        None
    };
    let ass_p = work_dir.join("_preview.ass");
    // WYSIWYG-паритет с render::run: subs.burn=false -> финальный рендер НЕ накладывает ничего (чистое
    // видео), поэтому и превью должно быть чистым. Иначе редактор показывал бы субтитры/блюр, которых в
    // экспорте не будет (баг-репорт code-review). При burn=false пишем пустой ASS + без блюра.
    let mut blur_boxes: Vec<dub_captions::BlurBox> = Vec::new();
    if proj.subs.burn {
        let sub_covers = build_ass(proj, &ass_p, Some(work_dir), vw, vh, total)?;
        // blur-боксы = project.captions.blur_boxes (hidden исключаются) + подложки под нашими субтитрами.
        blur_boxes = proj
            .captions
            .blur_boxes
            .iter()
            .filter(|b| !b.hidden)
            .map(|b| dub_captions::BlurBox {
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
                t0: b.t0,
                t1: b.t1,
                fill: b.fill.clone(),
            })
            .collect();
        blur_boxes.extend(sub_covers.iter().map(crate::render::cover_to_blur));
    } else {
        std::fs::write(&ass_p, "[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\n\n[Events]\n")
            .map_err(|e| t!("frame-empty-ass", error = e.to_string()))?;
    }

    // Разные файлы для полного/низкого разрешения — чтобы параллельные запросы (backpressure сериализует,
    // но подстрахуемся) не перезаписывали кадр друг друга.
    let out = work_dir.join(if scale_w.is_some() { "_preview_lr.jpg" } else { "_preview.jpg" });
    dub_captions::burn_frame(
        input,
        &ass_p,
        &out,
        t,
        &blur_boxes,
        Some((vw, vh)),
        proj.render.blur,
        proj.render.blur_sigma,
        scale_w,
    )?;
    std::fs::read(&out).map_err(|e| t!("frame-read-preview", error = e.to_string()))
}

/// Сырой кадр ОРИГИНАЛА (PNG-байты) на t — без сабов/блюра/дубляжа (для before/after). Порт
/// api.source_frame: ffmpeg input-seek extract одного кадра, без моделей.
pub fn source_frame(input: &Path, work_dir: &Path, t: f64) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    const FFMPEG: &str = "ffmpeg.exe";
    #[cfg(not(windows))]
    const FFMPEG: &str = "ffmpeg";
    let png = work_dir.join("_original.png");
    let out = std::process::Command::new(FFMPEG)
        .arg("-y")
        .arg("-ss")
        .arg(format!("{:.2}", t.max(0.0)))
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1", "-update", "1"])
        .arg(&png)
        .output()
        .map_err(|e| t!("common-ffmpeg-start", error = e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.chars().rev().take(600).collect::<String>().chars().rev().collect();
        return Err(format!("ffmpeg source_frame rc={:?}: {tail}", out.status.code()));
    }
    std::fs::read(&png).map_err(|e| t!("frame-read-source", error = e.to_string()))
}
