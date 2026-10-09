//! Порт vision-части dubengine/ctx_translate.py — анализ кадров через mmproj в llama-server:
//! стиль субтитров (цвет/шрифт/y/размер/n_lines/italic/uppercase), тайтлы-карточки, бренды, вторичные
//! капшены. Промпты VP/SP перенесены ДОСЛОВНО (выверены на стенде). Кадры дёргаем ffmpeg'ом (square-pad).

use std::path::Path;
use std::process::Command;

use base64::Engine;
use regex::Regex;
use serde_json::{Map, Value};

use dub_llm::{strip_think, ChatClient, Message, Part, Sampling};

use crate::{Note, TranslateError};

pub const FONTS: &str = "Montserrat, Oswald, Roboto, Russo One, Pacifico, Playfair Display, Caveat";

#[cfg(windows)]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG: &str = "ffmpeg";

/// _hex — вытащить #RRGGBB (upper). None если нет.
pub fn hex(s: &str) -> Option<String> {
    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"#([0-9a-fA-F]{6})").unwrap());
    RE.captures(s).map(|c| format!("#{}", c[1].to_uppercase()))
}

fn hex_opt(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| x.as_str()).and_then(hex)
}

/// _COUNTER_RE — слайд/страничные счётчики ("2/7", "3 of 7", "стр. 4") — это навигация, НЕ тайтл.
pub fn is_counter(txt: &str) -> bool {
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*(?:(?:page\s*|стр\.?\s*)?\d{1,3}\s*(?:/|of|из)\s*\d{1,3}|(?:page|стр\.?|слайд)\s*\d{1,3})\s*$",
        )
        .unwrap()
    });
    RE.is_match(txt)
}

/// Добавить style_block(src, txt) в аккумулятор, если такого текста (без учёта регистра) там ещё нет.
/// Общий дедуп для titles и вторичных captions.
fn push_unique(acc: &mut Vec<Value>, src: &Value, txt: &str) {
    let lower = txt.to_lowercase();
    let dup = acc
        .iter()
        .any(|e| e.get("text").and_then(|x| x.as_str()).map(str::to_lowercase) == Some(lower.clone()));
    if !dup {
        acc.push(style_block(src, txt));
    }
}

/// _vis_json — вытащить первый {...} JSON-объект из ответа модели; {} при ошибке.
fn vis_json(s: &str) -> Value {
    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?s)\{.*\}").unwrap());
    if let Some(m) = RE.find(s) {
        serde_json::from_str(m.as_str()).unwrap_or(Value::Object(Map::new()))
    } else {
        Value::Object(Map::new())
    }
}

/// _frame_b64 — square-pad кадр (SigLIP резайзит всё в 896x896; квадрат сохраняет геометрию букв) и base64.
fn frame_b64(video: &Path, t: f64, tmp: &Path) -> Result<String, TranslateError> {
    let _ = std::fs::remove_file(tmp); // сбросить прошлый кадр, чтобы не прочитать несвежий при сбое
    let out = Command::new(FFMPEG)
        .args(["-y", "-ss"])
        .arg(format!("{t:.1}"))
        .arg("-i")
        .arg(video)
        .args(["-frames:v", "1", "-vf",
            "pad='max(iw,ih)':'max(iw,ih)':'(ow-iw)/2':'(oh-ih)/2':color=black"])
        .arg(tmp)
        .output()
        .map_err(|e| TranslateError::Frame(format!("ffmpeg start: {e}")))?;
    if !out.status.success() || !tmp.exists() {
        return Err(TranslateError::Frame(format!(
            "ffmpeg frame extract failed at {t:.1}s (rc={:?})",
            out.status.code()
        )));
    }
    let bytes = std::fs::read(tmp).map_err(|e| TranslateError::Frame(e.to_string()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Коэрсинг числа: Gemma иногда шлёт число СТРОКОЙ ("0.5"). Питон float()/int() это принимает; голый
/// as_f64/as_i64 у serde роняет Value::String в None. Порт неявной коэрсии float()/int().
fn num_f64(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}
fn num_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => {
            let t = s.trim();
            t.parse::<i64>().ok().or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
        }
        _ => None,
    }
}

/// Стилевой блок для одного текстового элемента (title/caption) в dict рендерера. Порт _style_block.
fn style_block(d: &Value, txt: &str) -> Value {
    let bg_solid = d.get("background").and_then(|x| x.as_str()) == Some("solid");
    let align = d.get("align").and_then(|x| x.as_str()).unwrap_or("");
    let align = if ["left", "center", "right"].contains(&align) { align } else { "center" };
    serde_json::json!({
        "text": txt,
        "y_frac": num_f64(d.get("y_frac")),   // коэрсинг строки-числа -> f64 (иначе compose.yfrac дропнет)
        "color": hex_opt(d.get("color")),
        "bg": if bg_solid { hex_opt(d.get("background_color")) } else { None },
        "solid": bg_solid,
        "align": align,
        "outline": hex_opt(d.get("outline")).unwrap_or_else(|| "none".into()),
        "bold": d.get("bold").and_then(|x| x.as_bool()).unwrap_or(true),
        "italic": d.get("italic").and_then(|x| x.as_bool()).unwrap_or(false),
        "font": d.get("font"),
    })
}

/// Результат vision-анализа макета. sub_style/sub_y/titles/brands/captions — как extra в питоне.
#[derive(Default)]
pub struct Layout {
    pub sub_style: Option<Value>,
    pub sub_y: Option<i64>,
    pub titles: Vec<Value>,   // каждый — _style_block + позже tgt
    pub brands: Vec<Value>,   // {text, y_frac}
    pub captions: Vec<Value>, // вторичные капшены (blur-only)
}

fn most_common<T: Clone + Eq + std::hash::Hash>(items: &[T]) -> Option<T> {
    // Детерминизм как Counter.most_common(1): при ничьей — ПЕРВЫЙ по порядку появления (не HashMap-порядок,
    // который у Rust недетерминирован и давал бы «последнего»). Считаем -> берём первый item с макс. счётом.
    if items.is_empty() {
        return None;
    }
    let mut counts: std::collections::HashMap<&T, usize> = std::collections::HashMap::new();
    for it in items {
        *counts.entry(it).or_insert(0) += 1;
    }
    let maxc = counts.values().copied().max().unwrap_or(0);
    items.iter().find(|it| counts[*it] == maxc).cloned()
}

/// Одно image-сообщение к Gemma (кадр + текстовый промпт). Порт imsg.
fn imsg(
    llm: &ChatClient,
    video: &Path,
    tmp: &Path,
    t: f64,
    prompt: &str,
    mt: u32,
    temp: f32,
) -> Result<String, TranslateError> {
    let b64 = frame_b64(video, t, tmp)?;
    let parts = vec![Part::ImagePngB64(b64), Part::Text(prompt.to_string())];
    // Gemma-recommended sampling: top_k=64, top_p=0.95.
    let s = Sampling::new(temp, 0.95, mt).top_k(64);
    Ok(strip_think(&llm.chat(&[Message::user_parts(parts)], &s)?))
}

/// Авто-детект типа контента для кастинга (#115): live-action (реальные лица -> SCRFD+LVFace) vs
/// animation/cartoon/anime (рисованные -> anime_face+CCIP). Сэмплируем несколько кадров по телу видео,
/// спрашиваем Gemma-vision по каждому и голосуем (most_common, детерминированный tie-break). Возвращает
/// "real" | "anime". При неоднозначности/сбое — "real" (самый частый тип контента).
pub fn classify_content_type(
    llm: &ChatClient,
    video: &Path,
    tmp: &Path,
    total: f64,
    mut log: impl FnMut(&Note),
) -> String {
    const N: usize = 4;
    let prompt = "Look at this single video frame. Is it LIVE-ACTION (filmed real people and places \
        with a camera, photographic) or ANIMATION (hand-drawn or CG cartoon / anime, illustrated \
        characters)? Reply with exactly one word: LIVE or ANIMATION.";
    let mut votes: Vec<String> = Vec::new();
    for i in 0..N {
        // кадры по телу (8%..92%), избегаем заставок/финальных титров.
        let frac = 0.08 + 0.84 * (i as f64) / ((N - 1).max(1) as f64);
        let t = (total * frac).max(0.0);
        if let Ok(ans) = imsg(llm, video, tmp, t, prompt, 8, 0.1) {
            let a = ans.to_lowercase();
            // "animation"/"anime"/"cartoon"/"drawn" ловим ПЕРВЫМ (в "live-action" нет "anim").
            let v = if a.contains("anim") || a.contains("cartoon") || a.contains("drawn") {
                Some("anime")
            } else if a.contains("live") || a.contains("real") || a.contains("photo") {
                Some("real")
            } else {
                None // непонятный ответ — не голосуем
            };
            if let Some(v) = v {
                votes.push(v.to_string());
            }
        }
    }
    let decided = most_common(&votes).unwrap_or_else(|| "real".to_string());
    log(&Note::ContentType { decided: &decided, votes: votes.len(), frames: N });
    decided
}

/// VISION layout (фаза 1 ctx_translate.run) — сэмплируем кадры, читаем sub_style/sub_y/titles/brands/captions.
/// total — длительность (с), vh — высота кадра (px). video — исходное видео. tmp — путь под кадр.
pub fn analyze_layout(
    llm: &ChatClient,
    video: &Path,
    tmp: &Path,
    total: f64,
    vh: f64,
) -> Result<Layout, TranslateError> {
    // Мега-промпт VP (тайтлы + вторичные капшены). ДОСЛОВНО из ctx_translate.py.
    let vp = format!(
        "You are an on-screen-text style analyst. Each text will be re-drawn in another language and must \
look NATIVE. In \"sub\" describe the MAIN running SUBTITLE / spoken-word caption style — the \
prominent recurring on-screen WORDS that present the narration/dialogue (a bottom dialogue line, OR \
styled centred punchline captions, e.g. big italic outlined words). ALWAYS fill \"sub\" whenever ANY \
such words appear — they ARE the subtitles even if styled/centred. Report its size_frac = the letter \
HEIGHT as a fraction of the frame height (how big the text is).\n\
TITLES — list ONLY the opening/heading TITLE CARD (the video's name/topic, usually large, near the \
TOP, in the first seconds). Do NOT list the running per-scene caption that changes scene-to-scene \
(that IS the subtitle — already covered by \"sub\", never repeat it as a title). Text printed ON an \
object / product / packaging / food label / storefront sign, or a channel logo / watermark, is a \
BRAND (kind:\"brand\") — LEAVE it, never translate it (e.g. a chip-bag flavour, a shop name).\n\
IGNORE emoji, reaction icons, like/heart badges, app UI, and decorative coloured \
dots/pills that contain NO words — never report their colour as a background.\n\
BACKGROUND — for EACH element answer exactly one:\n\
  \"solid\" = ONLY if the area right behind the words is a UNIFORM flat colour edge-to-edge — a \
designed box / pill / bar, or a plain solid-colour card. Put the flat hex in background_color.\n\
  \"scene\" = the text sits over the video itself — trees, sky, a room, a person, a blurred photo, a \
gradient, ANY real-world imagery (it may have an outline/shadow — still scene). NO box. This is the \
DEFAULT; background_color = null. If the area behind the text is NOT a perfectly flat single colour, \
it is scene, even if it looks light or bright.\n\
A small badge NEXT TO the text is NOT its background -> scene. If unsure -> scene. Never give a \
background_color equal to the text colour.\n\
COLOR = the colour of the LETTER FILL itself, not the area around them. Most short-video \
captions are WHITE (or bright yellow); report a dark/black text colour ONLY when the letters are \
genuinely dark (e.g. black text on a white card). If the letters are a LIGHT fill with a DARK \
OUTLINE around them (very common: white words with a black edge), color = the LIGHT fill (#FFFFFF) \
and outline = the dark edge — NEVER report the outline colour as the text colour.\n\
SCENE_COLOR = the dominant colour of the video DIRECTLY BEHIND these words, so a same-colour cover \
can hide the old text seamlessly (e.g. white duck body -> #FFFFFF, dark room -> its dark hex).\n\
ITALIC — look CAREFULLY at the vertical strokes: are the letters SLANTED / leaning to the right \
(oblique)? Many short-video caption fonts ARE italic. Report italic:true whenever the text clearly \
slants, false only when the letters stand perfectly upright.\n\
ALIGN — how each block is justified by its text edges: left, center, or right. Do NOT default to \
center; only say center if it is truly centred.\n\
N_LINES — how many lines the running caption occupies on screen: 1 for a single-line caption, 2 (or \
more) for a stacked multi-line one. Match the original so our caption wraps the same way.\n\
UPPERCASE — true if the caption letters are ALL CAPS (every letter capital), false otherwise.\n\
JSON only: \
{{\"sub\":{{\"lettering\":\"<FIRST, in a few words, describe these subtitle letters by LOOKING: slant \
(italic/oblique vs upright), weight (bold vs regular), width (condensed/narrow vs normal), outline \
yes/no>\",\"y_frac\":0.0-1.0,\"size_frac\":0.0-1.0,\"n_lines\":1,\"color\":\"#hex of the LETTER FILL\",\
\"background\":\"solid\"|\"scene\",\"background_color\":\"#hex or null\",\"scene_color\":\"#hex behind the words\",\
\"outline\":\"none\" or \"#hex\",\"align\":\"left\"|\"center\"|\"right\",\"bold\":true or false,\"italic\":true or false,\
\"uppercase\":true or false,\"font\":\"<one of: {FONTS}>\"}},\
\"titles\":[{{\"text\":\"...\",\"kind\":\"title\"|\"brand\",\"y_frac\":0.0-1.0,\"align\":\"left\"|\"center\"|\"right\",\
\"color\":\"#hex of text\",\"background\":\"solid\"|\"scene\",\"background_color\":\"#hex or null\",\
\"bold\":true or false,\"italic\":true or false,\"font\":\"<family below>\"}}],\
\"captions\":[{{\"text\":\"...\",\"y_frac\":0.0-1.0,\"align\":\"left\"|\"center\"|\"right\",\"color\":\"#hex of text\",\
\"background\":\"solid\"|\"scene\",\"background_color\":\"#hex or null\",\"outline\":\"none\" or \"#hex\",\
\"bold\":true or false,\"italic\":true or false,\"font\":\"<family below>\"}}]}}. \
\"captions\" = readable overlay text that is NOT the running subtitle and NOT a title/brand (e.g. a \
small reaction line like \"OH, YEAH?\"). Include EVERY such line; if none, []. If nothing, {{}}."
    );

    // Сфокусированный SP (только sub-style). ДОСЛОВНО.
    let sp = format!(
        "You are a typography analyst. Look ONLY at the MAIN running SUBTITLE / spoken-word caption in this \
frame — the prominent recurring on-screen words (a bottom dialogue line OR big styled centred \
punchline words). Study the letters closely. JSON only: \
{{\"lettering\":\"<FIRST describe by LOOKING: slant (italic/oblique vs upright), weight (bold vs regular), \
width (condensed vs normal), outline yes/no>\",\"y_frac\":0.0-1.0,\"size_frac\":0.0-1.0,\"n_lines\":1,\
\"uppercase\":true or false,\"color\":\"#hex of the LETTER FILL\",\"outline\":\"none\" or \"#hex\",\
\"scene_color\":\"#hex right behind the words\",\"scene_flat\":true or false,\
\"align\":\"left\"|\"center\"|\"right\",\"bold\":true or false,\"italic\":true or false,\
\"font\":\"<one of: {FONTS}>\"}}. \
scene_flat = is the area DIRECTLY BEHIND the words a single FLAT uniform colour (true — e.g. a plain \
white/solid backdrop or a solid card) or a TEXTURED/varied scene like grass, trees, a room, a photo \
(false)? \
color = the FILL: light letters with a dark edge -> color #FFFFFF and outline = the dark hex (NEVER \
report the outline colour as the text colour). italic = letters clearly lean to the right. uppercase \
= every letter is a capital. Choose font by LOOK (Cyrillic stand-ins for CapCut/TikTok fonts): tall \
CONDENSED heavy impact (the 'Bold'/Hormozi/Anton/Bebas look) -> Oswald; clean geometric bold \
(Montserrat/Poppins) -> Montserrat; plain neutral sans (Roboto/Inter/Helvetica) -> Roboto; very heavy \
WIDE display -> Russo One; elegant serif -> Playfair Display; handwriting/script -> Caveat or Pacifico. \
If there are no subtitle words at all, return {{}}."
    );

    let mut subs_y: Vec<f64> = vec![];
    let mut colors: Vec<String> = vec![];
    let mut fonts: Vec<String> = vec![];
    let mut bolds: Vec<bool> = vec![];
    let mut itals: Vec<bool> = vec![];
    let mut cond: Vec<bool> = vec![];
    let mut ups: Vec<bool> = vec![];
    let mut szs: Vec<f64> = vec![];
    let mut nlines: Vec<i64> = vec![];
    let mut bgs: Vec<String> = vec![];
    let mut scenecols: Vec<String> = vec![];
    let mut flats: Vec<bool> = vec![];
    let mut outs: Vec<String> = vec![];
    let mut aligns_sub: Vec<String> = vec![];
    let mut titles: Vec<Value> = vec![];
    let mut brands: Vec<Value> = vec![];
    let mut caps_acc: Vec<Value> = vec![];

    // >=5 кейфреймов, x2 (10) для длинных клипов.
    let nkf = if total <= 60.0 { 5 } else { 10 };
    for i in 0..nkf {
        let fr = 0.03 + 0.94 * (i as f64) / ((nkf - 1) as f64);
        let t_kf = (total * fr).max(0.5);

        // Сфокусированный sub-style, GREEDY (temp 0).
        let mut sub = vis_json(&imsg(llm, video, tmp, t_kf, &sp, 380, 0.0)?);
        if let Some(inner) = sub.get("sub").cloned() {
            if inner.is_object() {
                sub = inner; // терпим {"sub":{...}}-обёртку
            }
        }
        if !sub.is_object() {
            sub = Value::Object(Map::new());
        }

        // Мега-вызов только для тайтлов + вторичных капшенов.
        let mut o = vis_json(&imsg(llm, video, tmp, t_kf, &vp, 480, 0.2)?);
        if !o.is_object() {
            o = Value::Object(Map::new());
        }

        if let Some(y) = num_f64(sub.get("y_frac")) {
            subs_y.push(y * vh);
        }
        if let Some(c) = hex_opt(sub.get("color")) {
            colors.push(c);
        }
        if let Some(f) = sub.get("font").and_then(|x| x.as_str()) {
            fonts.push(f.to_string());
        }
        let lt = sub.get("lettering").and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
        let sub_bold = sub.get("bold").and_then(|x| x.as_bool()).unwrap_or(false);
        bolds.push(sub_bold || ["bold", "heavy", "thick", "black", "extrabold"].iter().any(|w| lt.contains(w)));
        let sub_ital = sub.get("italic").and_then(|x| x.as_bool()).unwrap_or(false);
        itals.push(sub_ital || ["italic", "oblique", "slant"].iter().any(|w| lt.contains(w)));
        cond.push(["condensed", "narrow", "tall"].iter().any(|w| lt.contains(w)));
        ups.push(sub.get("uppercase").and_then(|x| x.as_bool()).unwrap_or(false));
        if let Some(sz) = num_f64(sub.get("size_frac")) {
            if sz > 0.0 && sz < 1.0 {
                szs.push(sz);
            }
        }
        if let Some(nl) = num_i64(sub.get("n_lines")) {
            if (1..=4).contains(&nl) {
                nlines.push(nl);
            }
        }
        let smode = sub.get("background").and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
        let scol = hex_opt(sub.get("background_color"));
        if std::env::var("DUB_VISION_DEBUG").is_ok() {
            eprintln!(
                "  [kf {:.1}s] bg={smode:?} bgcol={scol:?} color={:?} scene_color={:?} scene_flat={:?} y_frac={:?} raw_sub={}",
                t_kf,
                hex_opt(sub.get("color")),
                hex_opt(sub.get("scene_color")),
                sub.get("scene_flat"),
                sub.get("y_frac"),
                serde_json::to_string(&sub).unwrap_or_default(),
            );
        }
        bgs.push(if smode == "solid" { scol.clone().unwrap_or_else(|| "none".into()) } else { "none".into() });
        if let Some(sc) = hex_opt(sub.get("scene_color")) {
            scenecols.push(sc);
        }
        if let Some(sf) = sub.get("scene_flat").and_then(|x| x.as_bool()) {
            flats.push(sf);
        }
        outs.push(hex_opt(sub.get("outline")).unwrap_or_else(|| "none".into()));
        if let Some(al) = sub.get("align").and_then(|x| x.as_str()) {
            if ["left", "center", "right"].contains(&al) {
                aligns_sub.push(al.to_string());
            }
        }

        // titles
        if let Some(tl) = o.get("titles").and_then(|x| x.as_array()) {
            for ti in tl {
                if !ti.is_object() {
                    continue;
                }
                let txt = ti.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().replace('\n', " ");
                if txt.is_empty() || is_counter(&txt) {
                    continue;
                }
                if ti.get("kind").and_then(|x| x.as_str()) == Some("brand") {
                    brands.push(serde_json::json!({"text": txt, "y_frac": num_f64(ti.get("y_frac"))}));
                } else {
                    push_unique(&mut titles, ti, &txt);
                }
            }
        }
        // captions (вторичные)
        if let Some(cl) = o.get("captions").and_then(|x| x.as_array()) {
            for ci in cl {
                if !ci.is_object() {
                    continue;
                }
                let ctext = ci.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().replace('\n', " ");
                if ctext.is_empty() {
                    continue;
                }
                push_unique(&mut caps_acc, ci, &ctext);
            }
        }
    }

    let mut out = Layout::default();
    if !subs_y.is_empty() {
        subs_y.sort_by(|a, b| a.partial_cmp(b).unwrap()); // subs_y дальше не нужен — сортируем на месте
        out.sub_y = Some(subs_y[subs_y.len() / 2] as i64);
    }
    if !colors.is_empty() || !fonts.is_empty() {
        let bg = if bgs.is_empty() { "none".to_string() } else { most_common(&bgs).unwrap() };
        let solid = bg != "none";
        let font = if !cond.is_empty() && cond.iter().filter(|&&b| b).count() > cond.len() / 2 {
            Some("Oswald".to_string())
        } else if !fonts.is_empty() {
            most_common(&fonts)
        } else {
            None
        };
        let size_frac = if szs.is_empty() {
            Value::Null
        } else {
            szs.sort_by(|a, b| a.partial_cmp(b).unwrap()); // szs дальше не нужен
            serde_json::json!(szs[szs.len() / 2])
        };
        out.sub_style = Some(serde_json::json!({
            "color": if colors.is_empty() { "#FFFFFF".to_string() } else { most_common(&colors).unwrap() },
            "background": bg,
            "solid": solid,
            "outline": if outs.is_empty() { "none".to_string() } else { most_common(&outs).unwrap() },
            "align": if aligns_sub.is_empty() { "center".to_string() } else { most_common(&aligns_sub).unwrap() },
            "bold": if bolds.is_empty() { true } else { bolds.iter().filter(|&&b| b).count() as f64 >= bolds.len() as f64 / 2.0 },
            "italic": if itals.is_empty() { false } else { itals.iter().filter(|&&b| b).count() as f64 > itals.len() as f64 / 2.0 },
            "size_frac": size_frac,
            "scene_color": if scenecols.is_empty() { Value::Null } else { serde_json::json!(most_common(&scenecols).unwrap()) },
            "scene_flat": if flats.is_empty() { false } else { flats.iter().filter(|&&b| b).count() as f64 > flats.len() as f64 / 2.0 },
            "n_lines": if nlines.is_empty() { Value::Null } else { serde_json::json!(most_common(&nlines).unwrap()) },
            "uppercase": if ups.is_empty() { false } else { ups.iter().filter(|&&b| b).count() as f64 > ups.len() as f64 / 2.0 },
            "font": match font { Some(f) => serde_json::json!(f), None => Value::Null },
        }));
    }
    out.titles = titles;
    out.captions = caps_acc;
    out.brands = brands;
    Ok(out)
}

/// VISION scene-контекст (фаза 2) — краткое описание кадров для переводчика. Порт scene_context.
pub fn scene_context(
    llm: &ChatClient,
    video: &Path,
    tmp: &Path,
    total: f64,
    tgt: &str,
) -> Result<String, TranslateError> {
    let sp = format!(
        "Give a translator VISUAL context for dubbing to {tgt}. From this frame note briefly what helps \
translation: setting, who is present and their roles/relationship, what's happening (action/gag), any \
on-screen text/signs, and the visual mood. 2-4 short bullets."
    );
    let mut parts: Vec<String> = vec![];
    for fr in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let t = (total * fr).max(0.5);
        let ans = imsg(llm, video, tmp, t, &sp, 220, 0.2)?;
        parts.push(format!("[~{}s] {}", t as i64, ans));
    }
    Ok(parts.join("\n\n"))
}
