//! OCR-стадия analyze (раунд 4/5). Порт pipeline.run ocr_detect + compose.analyze_layout + весь
//! caption-композит (pipeline.run:388-643). Детектим вшитый текст (dub-ocr), выделяем субтитр-полосу,
//! коалесцируем её в band_blur (центр-гейт + IoU), затем зовём compose::run — тот собирает финальные
//! ТИТРЫ с bbox и полный набор блюр-боксов из vision-словаря (raw_ctx) + localize/caption_boxes.
//! Спикер-словарь (spoken) из транскрипта отсекает сцен-графику от субтитр-полосы.
//!
//! Fail-safe: любой сбой (нет моделей OCR / нет ORT_DYLIB_PATH / ffmpeg) логируется в SSE и оставляет
//! blur_boxes пустыми — analyze не падает (блюр не блокер, редактор добавит руками).

use dub_core::{BlurBox, Project};
use dub_ocr::{analyze_layout, blur, detect_regions, OcrPaths, RawDet, Region};
use std::collections::HashSet;

use crate::analyze::{AnalyzeArgs, AnalyzePaths, Progress};
use crate::compose::{self, ComposeCtx};

fn emit(progress: &Progress, stage: &str, msg: &str) {
    progress(serde_json::json!({ "stage": stage, "msg": msg }));
}

/// Прогнать OCR-стадию + caption-композит. Заполняет proj.captions.blur_boxes/titles/sub_style/sub_px.
/// vw/vh — размеры кадра (vw нужен центр-гейту band_blur и композиту).
pub fn stage(
    args: &AnalyzeArgs,
    paths: &AnalyzePaths,
    proj: &mut Project,
    vw: i64,
    vh: i64,
    total: f64,
    progress: &Progress,
) {
    match detect(paths, progress) {
        Ok((regions, raw)) => compose_captions(args, paths, proj, &regions, &raw, vw, vh, total, progress),
        Err(e) => emit(progress, "ocr_detect", &t!("ocr-no-blur", error = e)),
    }
}

/// Дорогая часть стадии: кадры + детекция/распознавание вшитого текста. Зависит только от видео и
/// моделей, поэтому analyze кэширует её выход целиком. Err — стадию пропустить (fail-safe у вызывающего).
pub fn detect(paths: &AnalyzePaths, progress: &Progress) -> Result<(Vec<Region>, Vec<RawDet>), String> {
    let ocr_paths = OcrPaths::under(&paths.models_root);
    if !ocr_paths.all_exist() {
        return Err(t!("ocr-models-missing"));
    }
    emit(progress, "ocr_detect", &t!("ocr-detecting-burned-text", model = "PP-OCR DBNet+CRNN"));
    // detect_regions: fps=caption_fps, дефолты как в питоне (min_dur .3, iou .3, pad 8, jitter 20, score .4).
    let fps = paths.caption_fps.max(1);
    detect_regions(&paths.input, &paths.work_dir, &ocr_paths, fps, 0.3, 0.3, 8, 20.0, 0.4)
        .map_err(|e| t!("ocr-detection-failed", error = e.to_string()))
}

/// Раскладка + блюр субтитр-полосы + caption-композит по готовым детекциям (дёшево; зависит от перевода).
#[allow(clippy::too_many_arguments)]
pub fn compose_captions(
    args: &AnalyzeArgs,
    paths: &AnalyzePaths,
    proj: &mut Project,
    regions: &[Region],
    raw: &[RawDet],
    vw: i64,
    vh: i64,
    total: f64,
    progress: &Progress,
) {

    // spoken vocab из исходного транскрипта (для отсечения сцен-графики от субтитр-полосы).
    let spoken: HashSet<String> = proj
        .segments
        .iter()
        .flat_map(|s| {
            s.src_text
                .split(|c: char| !c.is_alphabetic())
                .filter(|w| !w.is_empty())
                .map(|w| w.to_lowercase())
        })
        .collect();

    let fps = paths.caption_fps.max(1);
    let (localize, caption_boxes, sub_y_det) = analyze_layout(regions, vh, raw, &spoken);

    // ── band_blur (порт pipeline.py:416-442) ───────────────────────────────────
    // caption_boxes -> Det (x,y,w,h,t), отфильтровать центр-straddle гейтом (боковые вывески CHIYA/BAKERY
    // выпадают), коалесцировать в бокс-спаны (IoU>=0.5, тайм-гейт 1.6*dt), pad (-6,-4,+12,+8), t0..t1+dt.
    let dets: Vec<blur::Det> = caption_boxes
        .iter()
        .filter(|b| blur::straddles_center(b.0 as f32, b.2 as f32, vw as f32))
        .map(|b| (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32, b.4))
        .collect();
    let band_spans = blur::band_blur(dets, fps as u32);
    let band_blur: Vec<BlurBox> = band_spans
        .iter()
        .map(|s| BlurBox {
            x: s.0 as i64,
            y: s.1 as i64,
            w: s.2 as i64,
            h: s.3 as i64,
            t0: s.4 as f64,
            t1: s.5 as f64,
            hidden: false,
            fill: None,
            extra: Default::default(),
        })
        .collect();

    // sub_y: OCR-детект (усреднён по кадрам) — источник истины позиции; не трогаем при sub_y_locked
    // и не затираем уже выставленный vision-fallback пустым (как в питоне: sub_y = sub_y_det по умолчанию,
    // затем ce.sub_y только если sub_y пуст — тут vision уже положил sub_y в apply_extra при наличии).
    if !proj.captions.sub_y_locked {
        if let Some(y) = sub_y_det {
            proj.captions.sub_y = Some(y);
        }
    }

    // ── caption-композит (pipeline.run:388-643) ────────────────────────────────
    // do_translate = dub || subs==translate; fresh_subs у нас всегда false (нет режима свежих сабов).
    let do_translate = proj.mode == "dub" || matches!(proj.subs.mode.as_str(), "translate" | "bilingual");
    let cctx = ComposeCtx {
        vw,
        vh,
        total,
        do_translate,
        fresh_subs: false,
        src_lang: &args.src_lang,
        paths,
    };
    compose::run(proj, &localize, &caption_boxes, &band_blur, &cctx, progress);

    emit(
        progress,
        "ocr_detect",
        &t!(
            "ocr-summary",
            regions = regions.len(),
            localize = localize.len(),
            bands = band_blur.len(),
            sub_y = format!("{:?}", proj.captions.sub_y)
        ),
    );
}
