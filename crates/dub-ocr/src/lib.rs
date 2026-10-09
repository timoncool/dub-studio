//! dub-ocr — экранный OCR для блюр-боксов вшитого текста. Порт dubengine/text_detect.py (detect_regions:
//! семплинг кадров -> PP-OCR det+(cls)+rec -> merge строк -> IoU-трекинг -> most-frequent текст на трек) +
//! dubengine/compose.py (looks_like_caption, analyze_layout: субтитр-полоса vs титры).
//!
//! Движки det (DBNet+DBPostProcess: min-area-rect, edge-normal unclip) / cls (0/180) / rec (CRNN+CTC,
//! словарь из метадаты ONNX) — PP-OCR ONNX через ort (models/ocr/). Рантайм — свой ort-пайплайн
//! (paddle-ocr-rs использует ort rc.10 + download-binaries, что конфликтует с нашим пинном rc.13
//! load-dynamic api-28 и рискует тем же дедлоком, что чужая system32 DLL — потому свой движок на общей
//! 1.28.2). Внутренности (unclip/словарь/препроцесс/тесты) поглощены из ветки r4-ocr — строго лучше.

mod cls;
mod det;
mod geometry;
mod ort_engine;
mod rec;

/// Правила превращения OCR-детекций в блюр-прямоугольники project.json (порт pipeline.py):
/// centered-straddle гейт + band_blur коалесценция покадровых детекций. Самодостаточный (без ort).
pub mod blur;

pub use det::{DetBox, DetParams};

/// Сбой экранного OCR.
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("ort: {0}")]
    Ort(String),
    #[error("io: {0}")]
    Io(String),
    #[error("ffmpeg frames: {0}")]
    Ffmpeg(String),
    /// ffmpeg завершился с ошибкой: код выхода и хвост stderr.
    #[error("ffmpeg frames exit code {code:?}: {tail}")]
    FfmpegExit { code: Option<i32>, tail: String },
    #[error("the model returned no outputs")]
    NoOutputs,
    #[error("the recognition model has no 'character' metadata")]
    NoDictionary,
    /// Нарушен внутренний инвариант (форма тензора, общий буфер воркеров).
    #[error("internal: {0}")]
    Internal(String),
}

impl OcrError {
    /// Стабильный код ошибки (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            OcrError::Ort(_) => "ocr_ort",
            OcrError::Io(_) => "ocr_io",
            OcrError::Ffmpeg(_) => "ocr_ffmpeg",
            OcrError::FfmpegExit { .. } => "ocr_ffmpeg_exit",
            OcrError::NoOutputs => "ocr_no_outputs",
            OcrError::NoDictionary => "ocr_no_dictionary",
            OcrError::Internal(_) => "ocr_internal",
        }
    }
}
pub use rec::RecDict;

use det::detect;
use image::RgbImage;
use ort_engine::OnnxModel;
use rec::recognize_batch;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Одна per-frame детекция строки: (text, x, y, w, h, t) — сырой поток для frame-accurate блюра.
#[derive(Clone, Debug)]
pub struct RawDet {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub t: f32,
}

/// Регион-трек: (text, x, y, w, h, t0, t1) — стабилизированный трекингом. Порт выхода detect_regions.
#[derive(Clone, Debug)]
pub struct Region {
    pub text: String,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub t0: f32,
    pub t1: f32,
}

/// Пути к моделям OCR.
pub struct OcrPaths {
    pub det: PathBuf,
    pub cls: PathBuf,
    pub rec: PathBuf,
    /// Fallback-словарь .dict.txt. Основной источник — метадата ONNX (ключ "character");
    /// файл используется только если у модели нет метадаты.
    pub rec_dict: PathBuf,
}

impl OcrPaths {
    /// Дефолт: <models>/ocr/{det.onnx, cls.onnx, rec_cyrillic.onnx, rec_cyrillic.dict.txt}.
    pub fn under(models_root: &Path) -> Self {
        let o = models_root.join("ocr");
        OcrPaths {
            det: o.join("det.onnx"),
            cls: o.join("cls.onnx"),
            rec: o.join("rec_cyrillic.onnx"),
            rec_dict: o.join("rec_cyrillic.dict.txt"),
        }
    }
    /// Минимум для работы — det + rec. cls и rec_dict опциональны (cls улучшает ориентацию,
    /// rec_dict — фолбэк словаря, если в ONNX нет метадаты "character").
    pub fn all_exist(&self) -> bool {
        self.det.is_file() && self.rec.is_file()
    }
}

// ─── text_detect.py порт ─────────────────────────────────────────────────────

#[cfg(windows)]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG: &str = "ffmpeg";

/// Извлечь кадры видео с частотой fps в PNG (порт _frames).
fn extract_frames(video: &Path, out_dir: &Path, fps: i32) -> Result<Vec<PathBuf>, OcrError> {
    std::fs::create_dir_all(out_dir).map_err(|e| OcrError::Io(format!("{}: {e}", out_dir.display())))?;
    let vf = format!("fps={fps}");
    let pat = out_dir.join("f_%05d.png");
    let out = Command::new(FFMPEG)
        .arg("-y")
        .arg("-i")
        .arg(video)
        .args(["-vf", &vf])
        .arg(&pat)
        .output()
        .map_err(|e| OcrError::Ffmpeg(e.to_string()))?;
    if !out.status.success() {
        return Err(OcrError::FfmpegExit {
            code: out.status.code(),
            tail: String::from_utf8_lossy(&out.stderr).chars().rev().take(400).collect::<String>(),
        });
    }
    let mut frames: Vec<PathBuf> = std::fs::read_dir(out_dir)
        .map_err(|e| OcrError::Io(format!("{}: {e}", out_dir.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "png").unwrap_or(false))
        .collect();
    frames.sort();
    Ok(frames)
}

/// Нормализованная схожесть текста — трек продолжается пока это ТОТ ЖЕ текст. Порт _sim:
/// алфанумерик в нижнем регистре; подстрока -> 1.0; иначе точный difflib.SequenceMatcher.ratio().
pub fn sim(a: &str, b: &str) -> f32 {
    let norm = |s: &str| -> String {
        s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
    };
    let na = norm(a);
    let nb = norm(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na.contains(&nb) || nb.contains(&na) {
        return 1.0;
    }
    ratio(&na, &nb)
}

/// difflib.SequenceMatcher.ratio(): 2*M / (len(a)+len(b)), M — суммарный размер совпадающих блоков
/// (жадный longest-matching-block, как в difflib). Работаем по Vec<char> (юникод-корректно).
fn ratio(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let m = match_blocks(&a, &b);
    2.0 * m as f32 / total as f32
}

/// Суммарный размер совпадающих блоков (рекурсивный longest common contiguous block, как difflib).
fn match_blocks(a: &[char], b: &[char]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
    for (j, &c) in b.iter().enumerate() {
        b2j.entry(c).or_default().push(j);
    }
    let (mut best_i, mut best_j, mut best_size) = (0usize, 0usize, 0usize);
    let mut j2len: HashMap<usize, usize> = HashMap::new();
    for (i, &ca) in a.iter().enumerate() {
        let mut newj2len: HashMap<usize, usize> = HashMap::new();
        if let Some(js) = b2j.get(&ca) {
            for &j in js {
                let k = if j == 0 { 1 } else { j2len.get(&(j - 1)).copied().unwrap_or(0) + 1 };
                newj2len.insert(j, k);
                if k > best_size {
                    best_i = i + 1 - k;
                    best_j = j + 1 - k;
                    best_size = k;
                }
            }
        }
        j2len = newj2len;
    }
    if best_size == 0 {
        return 0;
    }
    let left = match_blocks(&a[..best_i], &b[..best_j]);
    let right = match_blocks(&a[best_i + best_size..], &b[best_j + best_size..]);
    best_size + left + right
}

/// IoU двух боксов (x,y,w,h). Порт _iou.
pub fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let x1 = ax.max(bx);
    let y1 = ay.max(by);
    let x2 = (ax + aw).min(bx + bw);
    let y2 = (ay + ah).min(by + bh);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = aw * ah + bw * bh - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Слить боксы слов на одной горизонтальной строке -> одна строка. Порт _merge_rows.
pub fn merge_rows(items: Vec<(String, f32, f32, f32, f32)>, y_tol: f32) -> Vec<(String, f32, f32, f32, f32)> {
    let mut sorted = items;
    sorted.sort_by(|a, b| (a.2, a.1).partial_cmp(&(b.2, b.1)).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(String, f32, f32, f32, f32)> = Vec::new();
    for (txt, x, y, w, h) in sorted {
        let cy = y + h / 2.0;
        let mut merged = false;
        for e in out.iter_mut() {
            let ecy = e.2 + e.4 / 2.0;
            if (ecy - cy).abs() <= y_tol * h.max(e.4) {
                let nx = e.1.min(x);
                let ny = e.2.min(y);
                let nx2 = (e.1 + e.3).max(x + w);
                let ny2 = (e.2 + e.4).max(y + h);
                e.0 = format!("{} {}", e.0, txt);
                e.1 = nx;
                e.2 = ny;
                e.3 = nx2 - nx;
                e.4 = ny2 - ny;
                merged = true;
                break;
            }
        }
        if !merged {
            out.push((txt, x, y, w, h));
        }
    }
    out
}

/// Один трек детекции (внутренний).
struct Track {
    bx: (f32, f32, f32, f32),
    t0: f32,
    t1: f32,
    last_t: f32,
    texts: Vec<String>,
}

/// detect_regions: семплинг кадров -> det+rec -> merge -> IoU-трекинг -> most-frequent текст.
/// Порт text_detect.detect_regions. Возвращает (regions, raw).
#[allow(clippy::too_many_arguments)]
pub fn detect_regions(
    video: &Path,
    work_dir: &Path,
    paths: &OcrPaths,
    fps: i32,
    min_dur: f32,
    iou_thr: f32,
    pad: i64,
    jitter: f32,
    score_thr: f32,
) -> Result<(Vec<Region>, Vec<RawDet>), OcrError> {
    // словарь и параметры грузим один раз (rec-мета читается с временной сессии) и клонируем воркерам.
    let rec_probe = OnnxModel::load(&paths.rec)?;
    let dict = match RecDict::from_session_meta(&rec_probe) {
        Ok(d) => d,
        Err(_) => RecDict::load(&paths.rec_dict)?,
    };
    drop(rec_probe);
    let det_params = DetParams::default();
    let use_cls = paths.cls.is_file() && std::env::var_os("DUB_OCR_NO_CLS").is_none();

    let fdir = work_dir.join("frames");
    let frames = extract_frames(video, &fdir, fps)?;

    // Кадры независимы -> W воркеров, каждый со своим набором сессий (ORT Session не Sync), intra=1
    // чтобы не оверсабскрайбить ядра. Порядок восстанавливаем по индексу кадра.
    let workers = std::env::var("DUB_OCR_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
        .clamp(1, 8)
        .min(frames.len().max(1));

    let frames = std::sync::Arc::new(frames);
    let dict_a = std::sync::Arc::new(dict);
    let next = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let out = std::sync::Arc::new(std::sync::Mutex::new(
        Vec::<(usize, f32, Vec<Line>)>::new(),
    ));
    let err = std::sync::Arc::new(std::sync::Mutex::new(None::<OcrError>));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (frames, dict_a, next, out, err) = (
                frames.clone(),
                dict_a.clone(),
                next.clone(),
                out.clone(),
                err.clone(),
            );
            let (det_path, rec_path, cls_path) =
                (paths.det.clone(), paths.rec.clone(), paths.cls.clone());
            scope.spawn(move || {
                let mut det_model = match OnnxModel::load_with_intra(&det_path, 1) {
                    Ok(m) => m,
                    Err(e) => { *err.lock().unwrap() = Some(e); return; }
                };
                let mut rec_model = match OnnxModel::load_with_intra(&rec_path, 1) {
                    Ok(m) => m,
                    Err(e) => { *err.lock().unwrap() = Some(e); return; }
                };
                let mut cls_model = if use_cls {
                    match OnnxModel::load_with_intra(&cls_path, 1) {
                        Ok(m) => Some(m),
                        Err(e) => { *err.lock().unwrap() = Some(e); return; }
                    }
                } else {
                    None
                };
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= frames.len() {
                        break;
                    }
                    let t = i as f32 / fps as f32;
                    let img = match image::open(&frames[i]) {
                        Ok(im) => im.to_rgb8(),
                        Err(_) => continue,
                    };
                    match process_frame(
                        &mut det_model, &mut rec_model, cls_model.as_mut(),
                        &dict_a, &det_params, &img, score_thr,
                    ) {
                        Ok(lines) => out.lock().unwrap().push((i, t, lines)),
                        Err(e) => { *err.lock().unwrap() = Some(e); return; }
                    }
                }
            });
        }
    });

    if let Some(e) = err.lock().unwrap().take() {
        return Err(e);
    }
    let mut frame_lines_idx = std::sync::Arc::try_unwrap(out)
        .map_err(|_| OcrError::Internal("out arc".into()))?
        .into_inner()
        .map_err(|e| OcrError::Internal(e.to_string()))?;
    frame_lines_idx.sort_by_key(|(i, _, _)| *i);
    let frame_lines: Vec<(f32, Vec<Line>)> =
        frame_lines_idx.into_iter().map(|(_, t, l)| (t, l)).collect();

    // Кадры больше не нужны -> убираем каталог work_dir/frames (иначе сотни PNG оставались на диске,
    // юзеры жаловались на «сохраняет в папку frames»).
    let _ = std::fs::remove_dir_all(&fdir);

    let (regions, raw) = detect_regions_frames(&frame_lines, fps, min_dur, iou_thr, pad, jitter);
    Ok((regions, raw))
}

/// Строка детекции для трекинга: (text, x, y, w, h) в пикселях.
pub type Line = (String, f32, f32, f32, f32);

/// Обработать один кадр: det -> кропы -> (опц. cls) -> rec ПАЧКОЙ -> фильтр порога -> merge_rows.
#[allow(clippy::too_many_arguments)]
fn process_frame(
    det_model: &mut OnnxModel,
    rec_model: &mut OnnxModel,
    mut cls_model: Option<&mut OnnxModel>,
    dict: &RecDict,
    det_params: &DetParams,
    img: &RgbImage,
    score_thr: f32,
) -> Result<Vec<Line>, OcrError> {
    let boxes = detect(det_model, img, det_params)?;
    let mut crops: Vec<RgbImage> = Vec::with_capacity(boxes.len());
    for b in &boxes {
        let mut crop = crop_rgb(img, b.x, b.y, b.w, b.h);
        if let Some(cls) = cls_model.as_deref_mut() {
            if cls::should_rotate180(cls, &crop)? {
                crop = image::imageops::rotate180(&crop);
            }
        }
        crops.push(crop);
    }
    let results = recognize_batch(rec_model, dict, &crops)?;
    let mut lines_raw: Vec<(String, f32, f32, f32, f32)> = Vec::new();
    for (b, (txt, score)) in boxes.iter().zip(results) {
        let tt = txt.trim();
        if tt.is_empty() || score < score_thr {
            continue;
        }
        lines_raw.push((tt.to_string(), b.x, b.y, b.w, b.h));
    }
    Ok(merge_rows(lines_raw, 0.6))
}

/// Кадро-ориентированный трекинг (порт detect_regions без ONNX): на вход отсортированные по времени
/// per-frame строки. Аспект-фильтр (w>=1.2h) и вся IoU/sim/de-jitter логика — здесь. Общий код-путь
/// для видео-`detect_regions` и юнит-тестов. Возвращает (regions, raw).
#[allow(clippy::too_many_arguments)]
pub fn detect_regions_frames(
    frame_lines: &[(f32, Vec<Line>)],
    fps: i32,
    min_dur: f32,
    iou_thr: f32,
    pad: i64,
    jitter: f32,
) -> (Vec<Region>, Vec<RawDet>) {
    let mut tracks: Vec<Track> = Vec::new();
    let mut raw: Vec<RawDet> = Vec::new();
    for (t, lines_in) in frame_lines {
        let t = *t;
        // аспект: строки широкие (w >= 1.2*h) — фильтр ДО raw, как в питоне.
        for ln in lines_in.iter().filter(|l| l.3 >= 1.2 * l.4) {
            let (txt, x, y, w, h) = (ln.0.clone(), ln.1, ln.2, ln.3, ln.4);
            raw.push(RawDet { text: txt.clone(), x, y, w, h, t });
            let bx = (x, y, w, h);
            // найти трек: то же место (IoU) И ~тот же текст (sim>=0.7) в пределах 2 кадров.
            let mut matched: Option<usize> = None;
            for (ti, tr) in tracks.iter().enumerate() {
                if tr.last_t >= t - 2.0 / fps as f32
                    && iou(tr.bx, bx) >= iou_thr
                    && sim(tr.texts.last().map(|s| s.as_str()).unwrap_or(""), &txt) >= 0.7
                {
                    matched = Some(ti);
                    break;
                }
            }
            if let Some(ti) = matched {
                let tr = &mut tracks[ti];
                tr.t1 = t;
                tr.last_t = t;
                tr.texts.push(txt);
                // де-джиттер: держим прежний бокс, если сдвиг <= jitter.
                let moved = (tr.bx.0 - bx.0).abs() > jitter
                    || (tr.bx.1 - bx.1).abs() > jitter
                    || (tr.bx.2 - bx.2).abs() > jitter
                    || (tr.bx.3 - bx.3).abs() > jitter;
                if moved {
                    tr.bx = bx;
                }
            } else {
                tracks.push(Track { bx, t0: t, t1: t, last_t: t, texts: vec![txt] });
            }
        }
    }

    // финализация треков -> Region (most-frequent текст, pad).
    let mut regions = Vec::new();
    for tr in &tracks {
        // +1.0/fps в гейте (как питон text_detect.py:126): трек реально длится t1-t0+1/кадр (см. t1 ниже),
        // без хвоста одиночные/короткие треки выкидывались, хотя питон их держит.
        if tr.t1 - tr.t0 + 1.0 / fps as f32 >= min_dur && !tr.texts.is_empty() {
            let (x, y, w, h) = tr.bx;
            let text = most_common(&tr.texts);
            regions.push(Region {
                text,
                x: ((x - pad as f32).max(0.0)) as i64,
                y: ((y - pad as f32).max(0.0)) as i64,
                w: (w + 2.0 * pad as f32) as i64,
                h: (h + 2.0 * pad as f32) as i64,
                t0: (tr.t0 * 100.0).round() / 100.0,
                t1: ((tr.t1 + 1.0 / fps as f32) * 100.0).round() / 100.0,
            });
        }
    }
    (regions, raw)
}

fn crop_rgb(img: &RgbImage, x: f32, y: f32, w: f32, h: f32) -> RgbImage {
    let x0 = x.max(0.0) as u32;
    let y0 = y.max(0.0) as u32;
    let x1 = ((x + w) as u32).min(img.width());
    let y1 = ((y + h) as u32).min(img.height());
    if x1 <= x0 || y1 <= y0 {
        return RgbImage::new(1, 1);
    }
    image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image()
}

fn most_common(v: &[String]) -> String {
    if v.is_empty() {
        return String::new();
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for s in v {
        *counts.entry(s.as_str()).or_insert(0) += 1;
    }
    // детерминизм как Counter.most_common: ничья -> первый по появлению (не HashMap-порядок)
    let maxc = counts.values().copied().max().unwrap_or(0);
    v.iter().find(|s| counts[s.as_str()] == maxc).cloned().unwrap_or_default()
}

// ─── compose.py порт (layout: субтитр-полоса vs титры) ───────────────────────

/// Свернуть латинские гомоглифы в кириллицу (визуально одинаковые глифы: A->А, K->К, E->Е, H->Н,
/// C->С, O->О, P->Р, T->Т, X->Х, Y->У, M->М, B->В и строчные). PP-OCR-cyrillic путает их на вшитом
/// тексте — для spoken-матча приводим к тому, что ВИДИТ зритель. Прочие символы без изменений.
pub fn fold_homoglyphs(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a' => 'а', 'c' => 'с', 'e' => 'е', 'o' => 'о', 'p' => 'р', 'x' => 'х', 'y' => 'у',
            'A' => 'А', 'B' => 'В', 'C' => 'С', 'E' => 'Е', 'H' => 'Н', 'K' => 'К', 'M' => 'М',
            'O' => 'О', 'P' => 'Р', 'T' => 'Т', 'X' => 'Х', 'Y' => 'У',
            other => other,
        })
        .collect()
}

/// caption-shape фильтр: реальный оверлей — словоподобный текст (letters/nonspace>=0.6, letters>=3).
/// Порт looks_like_caption.
pub fn looks_like_caption(txt: &str) -> bool {
    let t = txt.trim();
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    let nonspace = t.chars().filter(|c| !c.is_whitespace()).count();
    if letters < 3 || nonspace == 0 {
        return false;
    }
    letters as f32 / nonspace as f32 >= 0.6
}

/// Одна сгруппированная надпись: текст + bbox + время + медианная высота строки. Порт выхода
/// compose.group_captions ({text, bbox:(x,y,w,h), start, end, lh}).
#[derive(Clone, Debug)]
pub struct CaptionGroup {
    pub text: String,
    pub bbox: (f32, f32, f32, f32), // x, y, w, h
    pub start: f32,
    pub end: f32,
    pub lh: f32, // медианная высота строки
}

/// Сгруппировать боксы вшитого текста, образующие ОДНУ надпись — то же экранное время (time-IoU),
/// вертикально СТЫКУЮЩИЕСЯ и горизонтально ПЕРЕКРЫВАЮЩИЕСЯ (строки одной надписи в одной колонке) —
/// чтобы перевести как одну фразу и перерисовать одним блоком. Дословный порт compose.group_captions.
/// items: (text, x, y, w, h, t0, t1). gap_factor/min_tiou — как в питоне (дефолты 1.0/0.5).
pub fn group_captions(
    items: &[(String, f32, f32, f32, f32, f32, f32)],
    gap_factor: f32,
    min_tiou: f32,
) -> Vec<CaptionGroup> {
    fn tiou(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
        let inter = (a1.min(b1) - a0.max(b0)).max(0.0);
        let union = a1.max(b1) - a0.min(b0);
        if union > 0.0 {
            inter / union
        } else {
            0.0
        }
    }
    // рабочая группа: строки (y,txt), высоты, границы бокса, время.
    struct G {
        lines: Vec<(f32, String)>,
        hs: Vec<f32>,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        t0: f32,
        t1: f32,
    }
    // сортируем по (t0, y) как в питоне.
    let mut sorted: Vec<&(String, f32, f32, f32, f32, f32, f32)> = items.iter().collect();
    sorted.sort_by(|a, b| {
        a.5.partial_cmp(&b.5)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut groups: Vec<G> = Vec::new();
    for (txt, x, y, w, h, t0, t1) in sorted.into_iter().cloned() {
        let mut hit: Option<usize> = None;
        for (i, g) in groups.iter().enumerate() {
            let t_iou = tiou(t0, t1, g.t0, g.t1);
            let v_adjacent = y <= g.y1 + gap_factor * h && y + h >= g.y0 - gap_factor * h;
            let h_overlap = x < g.x1 && x + w > g.x0;
            if t_iou >= min_tiou && v_adjacent && h_overlap {
                hit = Some(i);
                break;
            }
        }
        match hit {
            None => groups.push(G {
                lines: vec![(y, txt)],
                hs: vec![h],
                x0: x,
                y0: y,
                x1: x + w,
                y1: y + h,
                t0,
                t1,
            }),
            Some(i) => {
                let g = &mut groups[i];
                g.lines.push((y, txt));
                g.hs.push(h);
                g.x0 = g.x0.min(x);
                g.y0 = g.y0.min(y);
                g.x1 = g.x1.max(x + w);
                g.y1 = g.y1.max(y + h);
                g.t0 = g.t0.min(t0);
                g.t1 = g.t1.max(t1);
            }
        }
    }
    groups
        .into_iter()
        .map(|mut g| {
            g.lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let text = g.lines.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" ");
            g.hs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let lh = g.hs[g.hs.len() / 2];
            CaptionGroup {
                text,
                bbox: (g.x0, g.y0, g.x1 - g.x0, g.y1 - g.y0),
                start: g.t0,
                end: g.t1,
                lh,
            }
        })
        .collect()
}

/// Бокс субтитр-полосы под блюр: (x,y,w,h,t0,t1).
pub type CaptionBox = (i64, i64, i64, i64, f32, f32);

/// analyze_layout -> (localize, caption_boxes, sub_y). Порт compose.analyze_layout: бакетим детекции по
/// горизонтальным полосам; в нижней части кадра выбираем полосу(ы), где текст ЧАСТО меняется (много
/// distinct строк) И слова СКАЗАНЫ (spoken) — это субтитр-полоса -> caption_boxes (блюр). Остальное ->
/// localize (титры). Возвращаем sub_y = центр самой «богатой» полосы.
pub fn analyze_layout(
    ocr: &[Region],
    frame_h: i64,
    raw: &[RawDet],
    spoken: &std::collections::HashSet<String>,
) -> (Vec<Region>, Vec<CaptionBox>, Option<i64>) {
    let band_frac = 0.10f32;
    let lower_from = 0.45f32;

    // band_src: предпочитаем raw (single-word karaoke дробит трек), фильтр looks_like_caption.
    let band_src: Vec<(&str, f32, f32, f32, f32, f32, f32)> = if !raw.is_empty() {
        raw.iter()
            .filter(|r| looks_like_caption(&r.text))
            .map(|r| (r.text.as_str(), r.x, r.y, r.w, r.h, r.t, r.t))
            .collect()
    } else {
        ocr.iter()
            .filter(|r| looks_like_caption(&r.text))
            .map(|r| (r.text.as_str(), r.x as f32, r.y as f32, r.w as f32, r.h as f32, r.t0, r.t1))
            .collect()
    };
    if band_src.is_empty() {
        return (ocr.to_vec(), Vec::new(), None);
    }

    let band_h = (frame_h as f32 * band_frac).max(1.0);
    // bucket -> детекции.
    let mut bands: HashMap<i64, Vec<(&str, f32, f32, f32, f32, f32, f32)>> = HashMap::new();
    for r in &band_src {
        let cy = r.2 + r.4 / 2.0;
        let b = (cy / band_h) as i64;
        bands.entry(b).or_default().push(*r);
    }

    let band_cy = |b: i64| -> f32 {
        let mut cys: Vec<f32> = bands[&b].iter().map(|r| r.2 + r.4 / 2.0).collect();
        cys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        cys[cys.len() / 2]
    };
    let distinct_texts = |rs: &[(&str, f32, f32, f32, f32, f32, f32)]| -> usize {
        let set: std::collections::HashSet<String> =
            rs.iter().filter(|r| !r.0.trim().is_empty()).map(|r| r.0.trim().to_lowercase()).collect();
        set.len()
    };
    // spoken_frac: доля distinct-строк полосы, которые ГОВОРЯТ сказанное (слово из транскрипта).
    // Питон сравнивает re.findall слова строки со spoken. Наш rec-CRNN, как и питоновский PP-OCR,
    // (1) склеивает слова строки без пробелов («онотправляеТ») и (2) путает визуально одинаковые
    // латиница/кириллица (К↔K, Е↔E, Н↔H, С↔C…) — это свойство модели, одинаковое в питоне. Поэтому
    // сначала фолдим гомоглифы (латиница -> кириллица), затем матчим и по слову, и по ПОДСТРОКЕ
    // (склеенная строка «говорит» слово, если оно в ней содержится). Это делает spoken-сигнал
    // измеримым на реальных reads — иначе frac застревает на ~0.1 даже для честной субтитр-полосы.
    let spoken_frac = |rs: &[(&str, f32, f32, f32, f32, f32, f32)]| -> f32 {
        if spoken.is_empty() {
            return 1.0;
        }
        let distinct: std::collections::HashSet<String> =
            rs.iter().filter(|r| !r.0.trim().is_empty()).map(|r| fold_homoglyphs(&r.0.trim().to_lowercase())).collect();
        if distinct.is_empty() {
            return 0.0;
        }
        let mut hit = 0;
        for t in &distinct {
            let ws: Vec<&str> = t.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).collect();
            let word_hit = !ws.is_empty()
                && ws.iter().filter(|w| spoken.contains(&w.to_lowercase())).count() as f32 >= 0.5 * ws.len() as f32;
            // ПОДСТРОКА: склеенная строка содержит сказанное слово (>=4 симв., чтобы не ловить шум).
            let substr_hit = spoken.iter().any(|sp| sp.chars().count() >= 4 && t.contains(sp.as_str()));
            if word_hit || substr_hit {
                hit += 1;
            }
        }
        hit as f32 / distinct.len() as f32
    };
    // Гейт субтитр-полосы. Геометрия — ДОСЛОВНЫЙ питон: nt>=3 distinct-строк (compose._is_sub_line;
    // A-шный доп. ratio nt>=0.3*len снят — питон его убрал как fps-хрупкий). Питоновский порог
    // spoken_frac>=0.5 недостижим на СКЛЕЕННЫХ read'ах (даже фолд+подстрока даёт ~0.44 для честной
    // полосы, проверено живым прогоном питоновского analyze_layout — он сам отдаёт 0 боксов). Поэтому
    // spoken служит РАЗЛИЧИТЕЛЕМ: сцен-графика (снек-паки, лейблы, вывески) НЕ говорит сказанного ->
    // frac==0; субтитр-полоса говорит -> frac>0. Отбрасываем полосу лишь если spoken есть И frac==0.
    // Это питоновский СМЫСЛ (блюрить только реальные субтитры), устойчивый к склейке rec.
    let is_sub_line = |b: i64| -> bool {
        let rs = &bands[&b];
        distinct_texts(rs) >= 3 && (spoken.is_empty() || spoken_frac(rs) > 0.0)
    };

    // Порядок бэндов = ПЕРВОЕ ПОЯВЛЕНИЕ при скане band_src (как питон-defaultdict `[b for b in bands]`),
    // НЕ случайный HashMap-порядок и НЕ cy. Так и детерминизм, и tie-break richest-полосы совпадает с
    // питоновским `max(lines, key=...)` (первый по порядку на ничьей distinct_texts).
    let mut band_order: Vec<i64> = Vec::new();
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for r in &band_src {
        let b = ((r.2 + r.4 / 2.0) / band_h) as i64;
        if seen.insert(b) {
            band_order.push(b);
        }
    }
    let lines: Vec<i64> = band_order
        .iter()
        .copied()
        .filter(|&b| band_cy(b) >= lower_from * frame_h as f32 && is_sub_line(b))
        .collect();
    if lines.is_empty() {
        return (ocr.to_vec(), Vec::new(), None);
    }
    let centers: Vec<f32> = lines.iter().map(|&b| band_cy(b)).collect();
    // richest = ПЕРВЫЙ по порядку появления с макс. distinct_texts (питон max() -> первый на ничьей; reduce
    // меняет best лишь на СТРОГО больший, поэтому ранний выигрывает при равенстве).
    let richest = *lines
        .iter()
        .reduce(|best, b| if distinct_texts(&bands[b]) > distinct_texts(&bands[best]) { b } else { best })
        .unwrap();
    let sub_y = band_cy(richest) as i64;

    let on_any_line = |cy: f32| -> bool { centers.iter().any(|c| (cy - c).abs() <= 0.7 * band_h) };

    // caption_boxes: каждая lettered строка на субтитр-линии (raw если есть, иначе ocr).
    let caption_boxes: Vec<CaptionBox> = if !raw.is_empty() {
        raw.iter()
            .filter(|r| on_any_line(r.y + r.h / 2.0) && r.text.chars().any(|c| c.is_alphabetic()))
            .map(|r| (r.x as i64, r.y as i64, r.w as i64, r.h as i64, r.t, r.t))
            .collect()
    } else {
        ocr.iter()
            .filter(|r| on_any_line(r.y as f32 + r.h as f32 / 2.0) && r.text.chars().any(|c| c.is_alphabetic()))
            .map(|r| (r.x, r.y, r.w, r.h, r.t0, r.t1))
            .collect()
    };
    let localize: Vec<Region> = ocr
        .iter()
        .filter(|r| !on_any_line(r.y as f32 + r.h as f32 / 2.0))
        .cloned()
        .collect();
    (localize, caption_boxes, Some(sub_y))
}

#[cfg(test)]
mod group_tests {
    use super::*;

    fn it(t: &str, x: f32, y: f32, w: f32, h: f32, t0: f32, t1: f32) -> (String, f32, f32, f32, f32, f32, f32) {
        (t.to_string(), x, y, w, h, t0, t1)
    }

    // Эталон — прогон питоновского compose.group_captions на тех же данных (см. отчёт):
    //  min_tiou=0.5: 'Autistic Boyfriend' (100,50,140,62) 0..2 lh30; 'LEUCO' (600,55,80,28); 'flash' 3..4.
    #[test]
    fn group_captions_tiou_05() {
        let items = vec![
            it("Autistic", 100.0, 50.0, 120.0, 30.0, 0.0, 2.0),
            it("Boyfriend", 100.0, 82.0, 140.0, 30.0, 0.0, 2.0),
            it("LEUCO", 600.0, 55.0, 80.0, 28.0, 0.0, 2.0),
            it("flash", 110.0, 50.0, 100.0, 30.0, 3.0, 4.0),
        ];
        let g = group_captions(&items, 1.0, 0.5);
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].text, "Autistic Boyfriend");
        assert_eq!(g[0].bbox, (100.0, 50.0, 140.0, 62.0));
        assert_eq!(g[0].lh, 30.0);
        assert_eq!(g[1].text, "LEUCO");
        assert_eq!(g[1].bbox, (600.0, 55.0, 80.0, 28.0));
        assert_eq!(g[2].text, "flash");
    }

    // min_tiou=0 (титр-группировка по позиции): 'flash' сливается в тот же вертикальный столбец, но LEUCO
    // (сбоку, нет h_overlap) остаётся отдельным. Эталон питона: 'Autistic flash Boyfriend' (100,50,140,62).
    #[test]
    fn group_captions_tiou_0_position() {
        let items = vec![
            it("Autistic", 100.0, 50.0, 120.0, 30.0, 0.0, 2.0),
            it("Boyfriend", 100.0, 82.0, 140.0, 30.0, 0.0, 2.0),
            it("LEUCO", 600.0, 55.0, 80.0, 28.0, 0.0, 2.0),
            it("flash", 110.0, 50.0, 100.0, 30.0, 3.0, 4.0),
        ];
        let g = group_captions(&items, 1.0, 0.0);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].text, "Autistic flash Boyfriend");
        assert_eq!(g[0].bbox, (100.0, 50.0, 140.0, 62.0));
        assert_eq!(g[0].start, 0.0);
        assert_eq!(g[0].end, 4.0);
        assert_eq!(g[1].text, "LEUCO");
    }
}
