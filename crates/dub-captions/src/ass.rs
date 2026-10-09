//! ASS-эмиттеры: таймкоды, drawing-примитивы (rounded rect / blob), word-spans, плашки, стилизованный
//! субтитр (реверсы karaoke/highlight/word/pop/whole) и локализованный титр. Дословный порт
//! соответствующих функций captions.py (_ts/_round_rect/_blob/_word_spans/_plate_events/_emit_styled/
//! _emit_title). Строки ASS формируются один-в-один (те же теги, порядок, округления).

use crate::font;
use crate::look::{self, ResolvedLook};
use crate::types::{Title, SubStyle};
use std::path::Path;

/// h:mm:ss.cc (центисекунды) — порт _ts.
pub fn ts(seconds: f64) -> String {
    let h = (seconds / 3600.0).floor() as i64;
    let m = ((seconds % 3600.0) / 60.0).floor() as i64;
    let s = seconds % 60.0;
    // {h:d}:{m:02d}:{s:05.2f}
    format!("{}:{:02}:{:05.2}", h, m, s)
}

/// Каталог bundled-шрифтов (fonts/). Путь к файлу шрифта для измерения. Env DUB_STUDIO_FONTS_DIR,
/// иначе установленный set_fonts_dir, иначе ./fonts. Потокобезопасно (Mutex — параллельные тесты/джобы
/// делят процесс; static mut был бы UB).
static FONTS_DIR: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Установить каталог шрифтов (вызывается сервером/примером до build). Идемпотентно.
pub fn set_fonts_dir(dir: impl AsRef<Path>) {
    if let Ok(mut g) = FONTS_DIR.lock() {
        *g = Some(dir.as_ref().to_path_buf());
    }
}

/// Текущий каталог шрифтов (env override, иначе установленный, иначе ./fonts).
pub fn fonts_dir() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("DUB_STUDIO_FONTS_DIR") {
        return std::path::PathBuf::from(d);
    }
    if let Ok(g) = FONTS_DIR.lock() {
        if let Some(d) = g.clone() {
            return d;
        }
    }
    std::path::PathBuf::from("fonts")
}

/// Путь к файлу шрифта для измерения (family -> файл; неизвестный -> Montserrat.ttf, иначе как есть).
pub fn font_path_for(family: &str) -> std::path::PathBuf {
    let dir = fonts_dir();
    if let Some(fname) = font::font_file_for(family) {
        let p = dir.join(fname);
        if p.exists() {
            return p;
        }
    }
    // Дефолт — Montserrat.ttf.
    dir.join("Montserrat.ttf")
}

/// ASS \p drawing прямоугольника со скруглением (r=радиус; r=h/2 -> pill) — порт _round_rect.
pub fn round_rect(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> String {
    let k = 0.5523 * r;
    format!(
        "m {:.0} {:.0} l {:.0} {:.0} b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} l {:.0} {:.0} \
         b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} l {:.0} {:.0} b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} \
         l {:.0} {:.0} b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0}",
        x0 + r, y0, x1 - r, y0,
        x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r,
        x1, y1 - r,
        x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1,
        x0 + r, y1,
        x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r,
        x0, y0 + r,
        x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0
    )
}

/// ASS \p drawing органической капсулы — порт _blob.
pub fn blob(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    let r = (y1 - y0) / 2.0;
    let k = 0.5523 * r;
    let midy = (y0 + y1) / 2.0;
    let midx = (x0 + x1) / 2.0;
    let bow = (y1 - y0) * 0.16;
    format!(
        "m {:.0} {:.0} b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} \
         b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} \
         b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} \
         b {:.0} {:.0} {:.0} {:.0} {:.0} {:.0}",
        x0 + r, y0,
        midx - r, y0 - bow, midx + r, y0 - bow, x1 - r, y0,
        x1 - r + k, y0, x1, y0 + r - k, x1, midy,
        x1, midy + k, x1 - r + k, y1, x1 - r, y1,
        midx + r, y1 + bow, midx - r, y1 + bow, x0 + r, y1,
        x0 + r - k, y1, x0, midy + k, x0, midy,
        x0, midy - k, x0 + r - k, y0, x0 + r, y0
    )
}

/// [(line_idx, word, ws, we)] — раскладка окна [a,b] по словам, взвешенная по длине — порт _word_spans.
pub fn word_spans(screen: &[String], a: f64, b: f64) -> Vec<(usize, String, f64, f64)> {
    let mut words: Vec<(usize, String)> = Vec::new();
    for (li, ln) in screen.iter().enumerate() {
        for w in ln.split_whitespace() {
            words.push((li, w.to_string()));
        }
    }
    let tot: usize = words.iter().map(|(_, w)| w.chars().count()).sum::<usize>().max(1);
    let dur = (b - a).max(0.001);
    let mut out = Vec::with_capacity(words.len());
    let mut cum = 0usize;
    for (li, w) in words {
        let ws = a + dur * cum as f64 / tot as f64;
        cum += w.chars().count();
        let we = a + dur * cum as f64 / tot as f64;
        out.push((li, w, ws, we));
    }
    out
}

/// [(line_idx, word, ws, we)] по готовым временам слов экрана (word_align). Число времён не совпало со
/// словами экрана — это ошибка сборки вызывающего, поэтому паника с причиной, а не тихая раскладка.
fn timed_spans(screen: &[String], timed: &[(f64, f64)]) -> Vec<(usize, String, f64, f64)> {
    let words: Vec<(usize, String)> = screen
        .iter()
        .enumerate()
        .flat_map(|(li, ln)| ln.split_whitespace().map(move |w| (li, w.to_string())))
        .collect();
    assert_eq!(words.len(), timed.len(), "the word timings do not match the words of the screen: {screen:?}");
    words.into_iter().zip(timed).map(|((li, w), &(ws, we))| (li, w, ws, we)).collect()
}

/// Layer-0 плашка(и) на стиле KP — порт _plate_events. Непрозрачны, чтобы блюр оригинала не просвечивал.
pub fn plate_events(
    plate: &str,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    plate6: &str,
    accent6: &str,
    a: f64,
    b: f64,
) -> Vec<String> {
    let h = y1 - y0;
    let ts0 = ts(a);
    let ts1 = ts(b);
    let ev = |tags: &str, path: &str| -> String {
        format!(
            "Dialogue: 0,{ts0},{ts1},KP,,0,0,0,,{{\\an7\\pos(0,0)\\bord0\\shad0{tags}\\p1}}{path}"
        )
    };
    match plate {
        "none" => vec![],
        "soft" => vec![ev(
            "\\1c&H000000&\\1a&H66&\\blur6",
            &round_rect(x0, y0, x1, y1, (h * 0.30) as i64 as f64),
        )],
        "blob" => vec![ev(&format!("\\1c{plate6}"), &blob(x0, y0, x1, y1))],
        "card" => {
            let dy = ((h * 0.14) as i64).max(4) as f64;
            let blur = ((h * 0.12) as i64).max(4);
            vec![
                ev(
                    &format!("\\1c&H101010&\\1a&H40&\\blur{blur}"),
                    &round_rect(x0, y0 + dy, x1, y1 + dy, (h * 0.28) as i64 as f64),
                ),
                ev(&format!("\\1c{plate6}"), &round_rect(x0, y0, x1, y1, (h * 0.28) as i64 as f64)),
            ]
        }
        "glow" => {
            let g = ((h * 0.20) as i64).max(6) as f64;
            vec![
                ev(
                    &format!("\\1c{accent6}\\1a&H20&\\blur{}", g as i64),
                    &round_rect(x0 - g, y0 - g, x1 + g, y1 + g, ((h + 2.0 * g) / 2.0).floor()),
                ),
                ev(&format!("\\1c{plate6}"), &round_rect(x0, y0, x1, y1, (h / 2.0).floor())),
            ]
        }
        _ => {
            let r = match plate {
                "box" => ((h * 0.10) as i64).max(4),
                "rounded" => (h * 0.30) as i64,
                "pill" => (h / 2.0) as i64,
                _ => (h * 0.30) as i64,
            };
            vec![ev(&format!("\\1c{plate6}"), &round_rect(x0, y0, x1, y1, r as f64))]
        }
    }
}

/// Экранировать { } в тексте (ASS override-скобки) — как .replace в питоне.
pub fn esc(s: &str) -> String {
    s.replace('{', "(").replace('}', ")")
}

/// Кегль экрана в луке и его ink-геометрия (ширина, верх и низ относительно центра строки) — как его
/// нарисует emit_styled: размер нормализуется по шрифту и ужимается, если строка шире кадра.
fn styled_geom(look: &ResolvedLook, screen: &[String], fs: i64, width: i64) -> (i64, f32, f32, f32) {
    let fp = font_path_for(&look.font);
    let mut fs_font = ((fs as f32 * look::font_scale(&look.font)) as i64).max(24);
    let (mut ink_w, mut top_rel, mut bot_rel) = font::text_geom(screen, fs_font, &fp);
    if ink_w > width as f32 * 0.90 {
        fs_font = ((fs_font as f32 * width as f32 * 0.90 / ink_w) as i64).max(20);
        let g = font::text_geom(screen, fs_font, &fp);
        ink_w = g.0;
        top_rel = g.1;
        bot_rel = g.2;
    }
    (fs_font, ink_w, top_rel, bot_rel)
}

/// Верхний и нижний край экрана в луке вместе с плашкой, относительно центра строки.
pub fn styled_extent(look: &ResolvedLook, screen: &[String], fs: i64, width: i64) -> (f32, f32) {
    let (fs_font, _, top_rel, bot_rel) = styled_geom(look, screen, fs, width);
    let pady = (fs_font as f32 * 0.30) as i64 as f32;
    (top_rel - pady, bot_rel + pady)
}

/// Плашка лука под экраном (KP, Layer 0) и ведущие теги его текста: позиция, шрифт, кегль, обводка.
#[allow(clippy::too_many_arguments)]
fn styled_frame(
    look: &ResolvedLook,
    a: f64,
    b: f64,
    screen: &[String],
    cx: i64,
    cy: i64,
    fs: i64,
    width: i64,
    bold: bool,
) -> (Vec<String>, String) {
    let (plate, fontname) = (&look.plate, &look.font);
    let (fs_font, ink_w, top_rel, bot_rel) = styled_geom(look, screen, fs, width);
    let padx = (fs_font as f32 * 0.55) as i64;
    let pady = (fs_font as f32 * 0.30) as i64;
    let x0 = (cx - (ink_w as i64) / 2 - padx).max(6);
    let x1 = (cx + (ink_w as i64) / 2 + padx).min(width - 6);
    let y0 = (cy as f32 + top_rel - pady as f32) as i64;
    let y1 = (cy as f32 + bot_rel + pady as f32) as i64;
    let oc = if look.base_lum < 0.45 { "&HFFFFFF&" } else { "&H101010&" };
    let mut lead = format!(
        "\\an5\\pos({cx},{cy})\\fn{fontname}\\fs{fs_font}{}\\3c{oc}",
        if bold { "\\b1" } else { "\\b0" }
    );
    if plate == "none" || plate == "soft" {
        let bord = ((fs_font as f32 * 0.11) as i64).max(3);
        let shad = ((fs_font as f32 * 0.06) as i64).max(2);
        lead.push_str(&format!("\\bord{bord}\\shad{shad}\\4c&H000000&"));
    }
    let plates = plate_events(plate, x0 as f64, y0 as f64, x1 as f64, y1 as f64, &look.plate6, &look.accent6, a, b);
    (plates, lead)
}

/// Экран в луке целиком, без пословной подсветки, стилем `style` — вторая строка двуязычных субтитров:
/// своя плашка, шрифт и обводка лука, цвет текста `look.base`, `extra` — теги после цвета.
#[allow(clippy::too_many_arguments)]
pub fn emit_styled_line(
    out: &mut Vec<String>,
    look: &ResolvedLook,
    a: f64,
    b: f64,
    screen: &[String],
    cx: i64,
    cy: i64,
    fs: i64,
    width: i64,
    bold: bool,
    style: &str,
    extra: &str,
) {
    let (plates, lead) = styled_frame(look, a, b, screen, cx, cy, fs, width, bold);
    out.extend(plates);
    out.push(format!(
        "Dialogue: 1,{},{},{style},,0,0,0,,{{{lead}\\1c{}{extra}}}{}",
        ts(a),
        ts(b),
        look.base,
        screen.join("\\N")
    ));
}

/// Один субтитр-экран в разрешённом луке (плашка Layer 0 + текст Layer 1) — порт _emit_styled.
/// `timed` — (начало, конец) каждого слова экрана по реальной речи (word_align); None — по длине слов.
#[allow(clippy::too_many_arguments)]
pub fn emit_styled(
    out: &mut Vec<String>,
    look: &ResolvedLook,
    a: f64,
    b: f64,
    screen: &[String],
    timed: Option<&[(f64, f64)]>,
    cx: i64,
    cy: i64,
    fs: i64,
    width: i64,
    bold: bool,
) {
    let (base6, accent6) = (&look.base, &look.accent6);
    let (plates, lead) = styled_frame(look, a, b, screen, cx, cy, fs, width, bold);
    out.extend(plates);
    let reveal0 = &look.reveal;

    let spans = match timed {
        Some(t) => timed_spans(screen, t),
        None => word_spans(screen, a, b),
    };
    // Пауза от появления экрана до первого слова (у равномерной раскладки её нет).
    let lead_in = spans.first().map(|s| (s.2 - a).max(0.0)).unwrap_or(0.0);
    let mut reveal = reveal0.clone();
    if reveal != "whole" && spans.is_empty() {
        reveal = "whole".to_string();
    }
    // Вставка перевода строки при смене line_idx — общий шаг всех reveal-веток.
    let nl = |parts: &mut String, li: usize, cur: &mut usize| {
        if li != *cur {
            parts.push_str("\\N");
            *cur = li;
        }
    };
    match reveal.as_str() {
        "karaoke" => {
            let mut parts = String::new();
            let pre = (lead_in * 100.0).round() as i64;
            if pre > 0 {
                parts.push_str(&format!("{{\\k{pre}}}"));
            }
            let mut cur = 0usize;
            for (li, w, ws, we) in &spans {
                nl(&mut parts, *li, &mut cur);
                let kf = (((we - ws) * 100.0).round() as i64).max(4);
                parts.push_str(&format!("{{\\kf{kf}}}{w} "));
            }
            out.push(format!(
                "Dialogue: 1,{},{},KT,,0,0,0,,{{{lead}\\1c{accent6}\\2c{base6}}}{parts}",
                ts(a),
                ts(b)
            ));
        }
        "highlight" => {
            if lead_in > 0.005 {
                out.push(format!(
                    "Dialogue: 1,{},{},KT,,0,0,0,,{{{lead}\\1c{base6}}}{}",
                    ts(a),
                    ts(spans[0].2),
                    screen.join("\\N")
                ));
            }
            for (k, (_, _, ws, we)) in spans.iter().enumerate() {
                let mut parts = String::new();
                let mut cur = 0usize;
                for (j, (lj, wj, _, _)) in spans.iter().enumerate() {
                    nl(&mut parts, *lj, &mut cur);
                    if j == k {
                        parts.push_str(&format!("{{\\1c{accent6}}}{wj}{{\\1c{base6}}} "));
                    } else {
                        parts.push_str(&format!("{wj} "));
                    }
                }
                out.push(format!(
                    "Dialogue: 1,{},{},KT,,0,0,0,,{{{lead}\\1c{base6}}}{parts}",
                    ts(*ws),
                    ts(*we)
                ));
            }
        }
        "word" | "pop" => {
            let mut parts = String::new();
            let mut cur = 0usize;
            for (li, w, ws, _we) in &spans {
                nl(&mut parts, *li, &mut cur);
                let ms = (((ws - a) * 1000.0) as i64).max(0);
                if reveal == "pop" {
                    parts.push_str(&format!(
                        "{{\\alpha&HFF&\\fscx55\\fscy55\\t({ms},{},\\alpha&H00&\\fscx100\\fscy100)}}{w} ",
                        ms + 130
                    ));
                } else {
                    parts.push_str(&format!(
                        "{{\\alpha&HFF&\\t({ms},{},\\alpha&H00&)}}{w} ",
                        ms + 90
                    ));
                }
            }
            out.push(format!(
                "Dialogue: 1,{},{},KT,,0,0,0,,{{{lead}\\1c{base6}}}{parts}",
                ts(a),
                ts(b)
            ));
        }
        _ => {
            let body = screen.join("\\N");
            out.push(format!(
                "Dialogue: 1,{},{},KT,,0,0,0,,{{{lead}\\1c{base6}}}{body}",
                ts(a),
                ts(b)
            ));
        }
    }
}

/// Один титр как в оригинале (плотная скруглённая плашка, если реальный контрастный фон, иначе
/// outline-only) — порт _emit_title. width/height — кадр.
pub fn emit_title(out: &mut Vec<String>, b: &Title, width: i64, height: i64) {
    // esc() заменяет только {}→() (не пробелы), а b.text уже trim'нут → повторный trim не нужен.
    let text0 = esc(b.text.trim());
    if text0.is_empty() {
        return;
    }
    let text = if b.uppercase { text0.to_uppercase() } else { text0 };
    let bbox = b.bbox.clone().unwrap_or_default();
    if bbox.len() < 4 {
        return;
    }
    let (x, y, w, h) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let fnt = if b.font.as_deref().map(look::is_known_font).unwrap_or(false) {
        b.font.clone().unwrap()
    } else {
        look::FONT_NAME.to_string()
    };
    let fp = font_path_for(&fnt);
    let align = b.align.to_lowercase();
    let cy = (y as f64 + h as f64 / 2.0) as i64;
    let (anc, mut ax): (i64, i64) = match align.as_str() {
        "left" => (4, x),
        "right" => (6, x + w),
        _ => (5, x + w / 2),
    };
    // SIZE: seed от line height оригинала, кап, перенос по ширине бокса.
    let n_src = (b.text.matches('\n').count() as i64 + 1).max(1);
    // lh==0 -> откат на h (питон `int(b.get("lh") or h)`); было Some(0)->1, теперь Some(0)->h.
    let lh0 = b.lh.filter(|&l| l != 0).unwrap_or(h).max(1);
    // КЕГЛЬ — от ВЫСОТЫ СТРОКИ (lh), НЕ от bbox.h. bbox af95837 растянут на всю contiguous-СТОПКУ
    // (для блюра — дыра не возвращается), но h стопки НЕ равно высоте титр-ТЕКСТА: у boris блюр-стопка
    // 169px против строки ~70px: h стопки для кегля непригоден —
    // строки, автошринк не срабатывал и fs=lh=70 раздувало титр втрое против оригинала. Бюджет строк
    // считаем от СОБСТВЕННОГО кол-ва исходных строк (n_src) + 1 слак — оригинальный box держал ровно
    // n_src*lh, round((n_src*lh)/lh)+1 = n_src+1 — стопка блюра на кегль больше не влияет.
    let max_lines = (n_src + 1).max(2);
    let fs_cap = (height as f64 * 0.085) as i64;
    // size_px==0 -> auto-fit (питон `if _szpx:` — 0 falsy), НЕ явные 12.
    let szpx = b.size_px.filter(|&s| s > 0);
    let mut fs = match szpx {
        Some(s) => s.max(12),
        None => fs_cap.min(lh0.max(22)),
    };
    let wrap_w = ((w as f64 * 1.10) as i64).max((width as f64 * 0.45) as i64);

    let wrapnl = |s: &str, size: i64| -> Vec<String> {
        let mut ls = Vec::new();
        for part in s.split('\n') {
            ls.extend(font::wrap(part, size, wrap_w as f32, &fp));
        }
        ls
    };
    let mut wrapped = wrapnl(&text, fs);
    let (mut ink_w, mut top_rel, mut bot_rel) = font::text_geom(&wrapped, fs, &fp);
    let mut guard = 0;
    while (ink_w > wrap_w as f32 * 1.02 || wrapped.len() as i64 > max_lines)
        && fs > 22
        && guard < 24
        && szpx.is_none()
    {
        fs = ((fs as f32 * 0.94) as i64).max(22);
        wrapped = wrapnl(&text, fs);
        let g = font::text_geom(&wrapped, fs, &fp);
        ink_w = g.0;
        top_rel = g.1;
        bot_rel = g.2;
        guard += 1;
    }
    let m = 8i64;
    ax = match anc {
        4 => ax.max(m).min(width - m - ink_w as i64),
        6 => ax.min(width - m).max(m + ink_w as i64),
        _ => ax.max(m + ink_w as i64 / 2).min(width - m - ink_w as i64 / 2),
    };
    let txt_hex = b.color.clone().unwrap_or_else(|| "#FFFFFF".to_string());
    let bg_in = b.bg.clone();
    let has_plate = bg_in
        .as_deref()
        .map(|bg| bg != "none" && (look::lum(bg) - look::lum(&txt_hex)).abs() >= 0.20)
        .unwrap_or(false);
    let bld = if b.bold { "\\b1" } else { "\\b0" };
    let itl = if b.italic { "\\i1" } else { "" };
    let body = wrapped.join("\\N");
    let mut out_tags: String;
    if has_plate {
        let padx = (fs as f32 * 0.34) as i64;
        let pady = (fs as f32 * 0.16) as i64;
        let (px0, px1): (f64, f64) = match anc {
            4 => (ax as f64 - padx as f64, ax as f64 + ink_w as f64 + padx as f64),
            6 => (ax as f64 - ink_w as f64 - padx as f64, ax as f64 + padx as f64),
            _ => (
                ax as f64 - ink_w as f64 / 2.0 - padx as f64,
                ax as f64 + ink_w as f64 / 2.0 + padx as f64,
            ),
        };
        let mut x0 = (px0 as i64).max(6);
        let mut x1 = (px1 as i64).min(width - 6);
        let mut y0 = (cy as f32 + top_rel - pady as f32) as i64;
        let mut y1 = (cy as f32 + bot_rel + pady as f32) as i64;
        x0 = x0.min(x - 4);
        x1 = x1.max(x + w + 4);
        x0 = x0.max(6);
        x1 = x1.min(width - 6);
        y0 = y0.min(y - 4);
        y1 = y1.max(y + h + 4);
        let r = (((y1 - y0) as f64 * 0.12) as i64).max(4);
        out.push(format!(
            "Dialogue: 0,{},{},KP,,0,0,0,,{{\\an7\\pos(0,0)\\1c{}\\bord0\\shad0\\p1}}{}",
            ts(b.start),
            ts(b.end),
            look::c6(&look::hex_ass(bg_in.as_deref().unwrap_or("#101010"))),
            round_rect(x0 as f64, y0 as f64, x1 as f64, y1 as f64, r as f64)
        ));
        out_tags = "\\bord0\\shad0".to_string();
    } else {
        let oc = if look::lum(&txt_hex) > 0.5 { "&H000000&" } else { "&HFFFFFF&" };
        let bord = ((fs as f32 * 0.11) as i64).max(3);
        let shad = ((fs as f32 * 0.05) as i64).max(2);
        out_tags = format!("\\bord{bord}\\shad{shad}\\3c{oc}\\4c{oc}");
    }
    let ow = b.outline_w;
    if b.outline.is_some() || ow.is_some() {
        let oc = look::c6(&look::hex_ass(
            b.outline
                .clone()
                .unwrap_or_else(|| {
                    if look::lum(&txt_hex) > 0.5 {
                        "#000000".to_string()
                    } else {
                        "#FFFFFF".to_string()
                    }
                })
                .as_str(),
        ));
        let bw = match ow {
            Some(v) => v.max(0),
            None => ((fs as f32 * 0.09) as i64).max(2),
        };
        out_tags.push_str(&format!("\\bord{bw}\\3c{oc}\\4c{oc}"));
    }
    // направленная тень (порт captions.py:466 — ВНЕ outline-блока, гейт по shadow_dir): dist=ow|fs*0.10,
    // цвет = outline редактора либо чёрный.
    let sh_dist = ow.unwrap_or_else(|| ((fs as f32 * 0.10) as i64).max(2));
    // питон truthy-guard (captions.py:468): пустая строка outline -> чёрный дефолт, не белый c6(hex_ass("")).
    let sh_col = match &b.outline {
        Some(o) if !o.is_empty() => look::c6(&look::hex_ass(o)),
        _ => "&H000000&".to_string(),
    };
    out_tags.push_str(&look::dir_shadow_tags(b.shadow_dir, sh_dist, &sh_col));
    out.push(format!(
        "Dialogue: 1,{},{},KT,,0,0,0,,{{\\an{}\\pos({},{}) \\fn{}\\fs{}{}{}{}\\1c{}}}{}",
        ts(b.start),
        ts(b.end),
        anc,
        ax,
        cy,
        fnt,
        fs,
        bld,
        itl,
        out_tags,
        look::c6(&look::hex_ass(&txt_hex)),
        body
    )
    // убрать лишний пробел после pos(...) (косметика, libass его игнорирует, но держим тег-строку чистой)
    .replace(") \\fn", ")\\fn"));
}

/// Хелпер: sub_style bold/italic для build (используется вне ass, оставлен для симметрии).
pub fn _touch(_s: &SubStyle, _p: &Path) {}
