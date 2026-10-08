//! Caption-композит (порт pipeline.run:388-643) — «склейка», собирающая финальные ТИТРЫ с bbox и
//! полный набор блюр-боксов из (а) vision-словаря raw_ctx (titles/captions/brands/sub_style) и
//! (б) OCR-выхода (localize-регионы + caption_boxes субтитр-полосы). Выполняется на analyze ПОСЛЕ
//! translate-стадии (raw_ctx готов) и OCR-детекции (localize/caption_boxes готовы) — точное место
//! композита в питоне.
//!
//! Что делает (дословно по pipeline.py):
//!  1. ctitles: merge по y (<=0.06) -> для каждого титра bbox матчингом y_frac*vh к localize-боксам
//!     (общий-word приоритет; fallback центр-полоса) -> Title.bbox/lh/start/end/color/bg/… (стр.497-543).
//!  2. blur_boxes: нарисованные title-блоки + таглайны (<=3.0/3.5с, центр, не в титре -> перевод+рисуем)
//!     + group_blur центр-OCR-титров + leftover под блоками + cap_blur вторичных капшенов + band_blur
//!     (стр.443-452, 579-589).
//!  3. sub_px: медиана высот OCR-боксов на sub_y (±7%vh) / cap_px / медиана caption_boxes (стр.608-610).
//!  4. sub_style mirror-from-captions + cap_px (стр.466-484).
//!  5. white-card fallback: scene_color=#FFFFFF/scene_flat при solid светлом титре (стр.490-493).
//!
//! Fail-safe: любой сбой оставляет то, что уже собрано (band_blur из OCR-стадии), analyze не падает.

use dub_core::{BlurBox, Project, SubStyle, Title};
use dub_ocr::{blur, group_captions, CaptionBox, CaptionGroup, Region};
use serde_json::Value;

/// Нормализация строки для сравнения тайтлов: lowercase + только буквенно-цифровые символы.
fn norm_alnum(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Округление f32 до 2 знаков, результат в f64 (тайминги блюр-боксов).
#[inline]
fn round2_f64(v: f32) -> f64 {
    ((v * 100.0).round() / 100.0) as f64
}

/// Сравнение f32 по возрастанию (NaN -> Equal) для sort_by над Vec<f32>.
fn f32_asc(a: &f32, b: &f32) -> std::cmp::Ordering {
    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
}

use crate::analyze::{AnalyzePaths, Progress};

fn emit(progress: &Progress, stage: &str, msg: &str) {
    progress(serde_json::json!({ "stage": stage, "msg": msg }));
}

/// r-tuple как в питоне: (text, x, y, w, h, t0, t1). Region.x/y/w/h — i64; приводим к f32 для геометрии.
struct R {
    text: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    t0: f32,
    t1: f32,
}

impl R {
    fn from_region(r: &Region) -> Self {
        R {
            text: r.text.clone(),
            x: r.x as f32,
            y: r.y as f32,
            w: r.w as f32,
            h: r.h as f32,
            t0: r.t0,
            t1: r.t1,
        }
    }
    fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
}

/// Промежуточный нарисованный блок (loc_block): bbox + текст + время + стиль. Порт loc_blocks-словаря.
struct LocBlock {
    bbox: (i32, i32, i32, i32), // x, y, w, h
    text: String,
    start: f32,
    end: f32,
    lh: i32,
    color: Option<String>,
    bg: Option<String>,
    solid: bool,
    align: String,
    bold: bool,
    italic: bool,
    font: Option<String>,
}

/// Токенизировать строку в слова (>=3 симв., нижний регистр) — как re.findall(r"\w+") + len>=3 в питоне.
fn words3(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.chars().count() >= 3)
        .map(|w| w.to_lowercase())
        .collect()
}

/// alpha-count (sum(c.isalpha())).
fn alpha_count(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphabetic()).count()
}

/// Аргументы композита.
pub struct ComposeCtx<'a> {
    pub vw: i64,
    pub vh: i64,
    pub total: f64,
    /// do_translate = dub || subs==translate (для таглайн-перевода).
    pub do_translate: bool,
    /// fresh_subs — режим свежих сабов (нечего блюрить). В нашем Project по умолчанию false.
    pub fresh_subs: bool,
    /// src_lang для таглайн-MT.
    pub src_lang: &'a str,
    /// пути (llama-бинарь/веса) для опционального таглайн-MT.
    pub paths: &'a AnalyzePaths,
}

/// Запустить композит. Мутирует proj.captions: titles (с bbox), blur_boxes (полный набор), sub_style
/// (mirror/white-card), raw_plan["sub_px"]. localize/caption_boxes — из OCR-стадии; band_blur — уже
/// посчитанный субтитр-блюр (передаём готовым, чтобы не дублировать коалесценцию).
pub fn run(
    proj: &mut Project,
    localize: &[Region],
    caption_boxes: &[CaptionBox],
    band_blur: &[BlurBox],
    ctx: &ComposeCtx,
    progress: &Progress,
) {
    let vw = ctx.vw as f32;
    let vh = ctx.vh as f32;
    let total = ctx.total as f32;
    let cx_lo = 0.40 * vw;
    let cx_hi = 0.60 * vw;
    let centered = |x: f32, w: f32| -> bool { x < cx_hi && x + w > cx_lo };

    // localize -> R-вью (r[0..6] как в питоне). Порядок как отдал analyze_layout (отсортирован по t0,y).
    let loc: Vec<R> = localize.iter().map(R::from_region).collect();

    // ── ctx-словарь (raw_ctx = ce) ────────────────────────────────────────────
    let ce = &proj.raw_ctx;
    let titles_v: Vec<Value> = ce
        .get("titles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let captions_v: Vec<Value> = ce
        .get("captions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let brands_v: Vec<Value> = ce
        .get("brands")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // ── group_blur: центр-OCR-титры (как в питоне groups) ──────────────────────
    // TITLE-кандидаты из localize: (<=1.5с в начале ИЛИ >= total-6 в конце) И lh>=4%vh И центр.
    let title_src: Vec<(String, f32, f32, f32, f32, f32, f32)> = loc
        .iter()
        .filter(|r| {
            (r.t0 <= 1.5 || r.t1 >= total - 6.0) && r.h >= 0.04 * vh && centered(r.x, r.w)
        })
        .map(|r| (r.text.clone(), r.x, r.y, r.w, r.h, r.t0, r.t1))
        .collect();
    let mut groups: Vec<CaptionGroup> = group_captions(&title_src, 1.0, 0.0)
        .into_iter()
        .filter(|g| alpha_count(&g.text) >= 4)
        .collect();
    // отсечь кандидатов, идущих КОНКУРЕНТНО с субтитр-полосой (_runs_with_subs).
    let band_t: Vec<f32> = {
        let mut v: Vec<f32> = caption_boxes.iter().map(|b| b.4).collect();
        v.sort_by(f32_asc);
        v
    };
    let runs_with_subs = |gs: f32, ge: f32| -> bool {
        let inside: Vec<f32> = band_t.iter().copied().filter(|&t| gs <= t && t <= ge).collect();
        if inside.len() < 3 {
            return false;
        }
        // band_t отсортирован, filter сохраняет порядок -> min/max = первый/последний (без двух fold).
        let span = inside[inside.len() - 1] - inside[0];
        span >= 0.4 * (ge - gs).max(0.1)
    };
    groups.retain(|g| !runs_with_subs(g.start, g.end));

    // ── sub_style: mirror-from-captions + white-card fallback + cap_px ──────────
    // sub_style уже мог прийти из vision (apply_extra). Если пуст И есть captions -> зеркалим.
    let mut cap_px: Option<i64> = None;
    ensure_sub_style_mirror(proj, &captions_v, &loc, vw, vh, &mut cap_px);
    white_card_fallback(proj, &titles_v);

    // ── ctitles -> loc_blocks с bbox (порт 497-543) ────────────────────────────
    let mut loc_blocks: Vec<LocBlock> = Vec::new();
    // ctitles = titles c непустым tgt.
    let mut ctitles: Vec<Value> = titles_v
        .iter()
        .filter(|t| {
            t.get("tgt").and_then(|x| x.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false)
        })
        .cloned()
        .collect();
    // merge по y_frac (<=0.06) в ОДИН (склеенные слова титра не встают стопкой на один Y).
    ctitles.sort_by(|a, b| {
        yfrac(a).partial_cmp(&yfrac(b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let ctitles = merge_titles_by_y(ctitles);

    // cap_blur + cap_ys (вторичные капшены -> блюр их региона).
    let mut cap_blur: Vec<BlurBox> = Vec::new();
    let mut cap_ys: Vec<f32> = Vec::new();

    if !ctitles.is_empty() {
        let ycs: Vec<f32> = ctitles.iter().map(|t| yfrac(t) as f32 * vh).collect();
        // токенизируем каждую localize-строку ОДИН раз.
        let rowwords: Vec<std::collections::HashSet<String>> =
            loc.iter().map(|r| words3(&r.text)).collect();
        for (ti, t) in ctitles.iter().enumerate() {
            let yc = ycs[ti];
            let tw = words3(t.get("text").and_then(|x| x.as_str()).unwrap_or(""));
            // кандидаты: боксы, чей БЛИЖАЙШИЙ титр по y — этот, в пределах 13%vh.
            let cand: Vec<usize> = loc
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    let cyr = r.cy();
                    (cyr - yc).abs() <= vh * 0.13
                        && nearest_title(&ycs, cyr) == ti
                })
                .map(|(ri, _)| ri)
                .collect();
            let shared: Vec<usize> =
                cand.iter().copied().filter(|&ri| !tw.is_disjoint(&rowwords[ri])).collect();
            // МУЛЬТИ-СТРОЧНЫЙ титр — ОДИН вертикально-стопочный блок, но OCR может слепить целую строку
            // в бесшовный токен ("tomyplacelineonaSlavic") -> она НЕ делит слово с текстом титра и выпадает
            // из shared, оставляя bbox коротким -> ОРИГИНАЛ средней строки ПРОСВЕЧИВАЕТ под нарисованным
            // титром. Дорастить набор до СОБСТВЕННОЙ contiguous-стопки титра: любой cand-бокс, вертикально
            // примыкающий + горизонтально пересекающий уже выбранный (тот же v_adjacent && h_overlap, что и в
            // group_captions) И присутствующий НА ЭКРАНЕ ВО ВРЕМЯ САМОГО ТИТРА (overlap с ЯКОРНЫМ спаном =
            // t0..t1 word-matched shared-строк). Якорный тайм-гейт (НЕ транзитивный попарный) критичен:
            // повторяющийся нижний капшен на t3,t6,…t27 каждый перекрывает предыдущий -> попарный гейт
            // ПРОТЯНУЛ бы таглайн t1.5-3с до scene-label t27 и растянул серую плашку на всё видео. Ограничено
            // cand (yc±13%vh, nearest-title==this) -> не дотянется до субтитр-полосы; shared — якорь: чужие
            // капшены той же высоты из ДРУГИХ сцен отсекаются при отсутствии вертикальной связности.
            // anchor = word-matched строки; если OCR слепил строку титра в бесшовный токен и НИ ОДНА не
            // матчится ("ExplainedbyDucks"), fallback на ЕДИНСТВЕННЫЙ cand-бокс, ближайший к yc титра (это и
            // есть СОБСТВЕННАЯ строка титра) — НЕ весь cand, иначе далёкий scene-label той же колонки утянул
            // бы якорный спан и дал over-blur.
            let anchor: Vec<usize> = if !shared.is_empty() {
                shared.clone()
            } else {
                cand.iter()
                    .copied()
                    .min_by(|&a, &b| {
                        let da = ((loc[a].y + loc[a].h / 2.0) - yc).abs();
                        let db = ((loc[b].y + loc[b].h / 2.0) - yc).abs();
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .into_iter()
                    .collect()
            };
            let a_t0 = anchor.iter().map(|&ri| loc[ri].t0).fold(f32::MAX, f32::min);
            let a_t1 = anchor.iter().map(|&ri| loc[ri].t1).fold(f32::MIN, f32::max);
            let mut picked: std::collections::BTreeSet<usize> = anchor.iter().copied().collect();
            let mut grew = true;
            while grew {
                grew = false;
                for &ri in &cand {
                    if picked.contains(&ri) {
                        continue;
                    }
                    let rc = &loc[ri];
                    if !(rc.t1.min(a_t1) > rc.t0.max(a_t0)) {
                        continue; // не на экране во время самого титра
                    }
                    let joins = picked.iter().any(|&pj| {
                        let rp = &loc[pj];
                        let v_adjacent =
                            rc.y <= rp.y + rp.h + rc.h && rc.y + rc.h >= rp.y - rc.h;
                        let h_overlap = rc.x < rp.x + rp.w && rc.x + rc.w > rp.x;
                        v_adjacent && h_overlap
                    });
                    if joins {
                        picked.insert(ri);
                        grew = true;
                    }
                }
            }
            let near_vec: Vec<usize> = picked.into_iter().collect();
            let near_idx: &[usize] = &near_vec;
            let (x0, y0, x1, y1, st, en, lh);
            if !near_idx.is_empty() {
                let near: Vec<&R> = near_idx.iter().map(|&ri| &loc[ri]).collect();
                x0 = near.iter().map(|r| r.x).fold(f32::MAX, f32::min);
                y0 = near.iter().map(|r| r.y).fold(f32::MAX, f32::min);
                x1 = near.iter().map(|r| r.x + r.w).fold(f32::MIN, f32::max);
                y1 = near.iter().map(|r| r.y + r.h).fold(f32::MIN, f32::max);
                st = near.iter().map(|r| r.t0).fold(f32::MAX, f32::min);
                en = near.iter().map(|r| r.t1).fold(f32::MIN, f32::max);
                lh = near.iter().map(|r| r.h).fold(f32::MAX, f32::min);
            } else {
                // нет OCR-бокса на этом y -> центр-полоса.
                x0 = (vw * 0.08) as i32 as f32;
                x1 = (vw * 0.92) as i32 as f32;
                y0 = (yc - vh * 0.045) as i32 as f32;
                y1 = (yc + vh * 0.045) as i32 as f32;
                st = 0.0;
                en = total.min(4.0);
                lh = (vh * 0.05) as i32 as f32;
            }
            loc_blocks.push(LocBlock {
                bbox: (x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32),
                text: t.get("tgt").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                start: st,
                end: en,
                lh: lh as i32,
                color: sval(t, "color"),
                bg: sval(t, "bg"),
                solid: bval(t, "solid", false),
                align: sval(t, "align").unwrap_or_else(|| "center".into()),
                bold: bval(t, "bold", true),
                italic: bval(t, "italic", false),
                font: sval(t, "font"),
            });
        }

        // cap_blur вторичных капшенов + cap_ys (порт 549-554).
        for c in &captions_v {
            let yc2 = yfrac(c) as f32 * vh;
            for r in loc.iter().filter(|r| (r.cy() - yc2).abs() <= vh * 0.06) {
                cap_blur.push(region_box(r));
                cap_ys.push(r.cy());
            }
        }

        // таглайны/leftover-под-блоками, оставшийся центр-текст титр-карты (tcard_rows) -> перевести и
        // нарисовать с заливкой (порт 555-578). MT опционален и fail-safe.
        let bys: Vec<f32> = brands_v.iter().map(|b| yfrac(b) as f32 * vh).collect();
        let mut loc_ys: Vec<f32> =
            loc_blocks.iter().map(|b| b.bbox.1 as f32 + b.bbox.3 as f32 / 2.0).collect();
        loc_ys.extend(cap_ys.iter().copied());
        let tcard_rows: Vec<&R> = loc
            .iter()
            .filter(|r| {
                r.t0 <= 3.5
                    && alpha_count(&r.text) >= 3
                    && centered(r.x, r.w)
                    && !bys.iter().any(|&by| (r.cy() - by).abs() <= vh * 0.05)
                    && !loc_ys.iter().any(|&ly| (r.cy() - ly).abs() <= vh * 0.06)
            })
            .collect();
        if !tcard_rows.is_empty() && ctx.do_translate {
            translate_taglines(proj, &tcard_rows, ctx, &mut loc_blocks, progress);
        }
    }

    // ── финальный blur_boxes ───────────────────────────────────────────────────
    let group_blur: Vec<BlurBox> = groups
        .iter()
        .map(|g| bbox_box(g.bbox, g.start, g.end))
        .collect();

    // ПОМЕТИТЬ band-боксы маркером extra["band"]=true. Питон держит субтитр-полосу в ОТДЕЛЬНОМ списке
    // caption_boxes (analyze_layout) и per-segment seg_y (pipeline.py 757-765) медианит ТОЛЬКО по нему —
    // титры/таглайны/group туда не попадают по построению. Порт складывает всё в один blur_boxes, поэтому
    // маркируем именно band-подмножество (= питоновский caption_boxes-производный band_blur), чтобы
    // render.rs::seg_y ехал на реальную полосу оригинала, а не на верхний title/tagline-блюр.
    let band_tagged: Vec<BlurBox> = band_blur
        .iter()
        .cloned()
        .map(|mut b| {
            b.extra.insert("band".to_string(), serde_json::Value::Bool(true));
            b
        })
        .collect();
    let band_blur: &[BlurBox] = &band_tagged;

    let final_blur: Vec<BlurBox> = if !ctitles.is_empty() {
        // ctx-ветка: loc_blocks + band_blur + leftover + cap_blur + group_blur (порт 586-587).
        let drawn: Vec<(f32, f32, f32, f32)> = loc_blocks
            .iter()
            .map(|b| (b.bbox.1 as f32, (b.bbox.1 + b.bbox.3) as f32, b.start, b.end))
            .collect();
        let leftover: Vec<BlurBox> = loc
            .iter()
            .filter(|r| {
                centered(r.x, r.w)
                    && alpha_count(&r.text) >= 3
                    && drawn.iter().any(|&(yy0, yy1, sst, een)| {
                        let cyr = r.cy();
                        yy0 <= cyr && cyr <= yy1 && r.t0 < een + 0.3 && r.t1 > sst - 0.3
                    })
            })
            .map(region_box)
            .collect();
        let mut v: Vec<BlurBox> = loc_blocks.iter().map(locblock_box).collect();
        v.extend_from_slice(band_blur);
        v.extend(leftover);
        v.extend(cap_blur);
        v.extend(group_blur);
        v
    } else {
        // не-ctx (нет ctitles): loc_blocks(пусто) + таглайны + group_blur + band_blur (порт 450-452).
        // Таглайны здесь = центр-localize <=3.0с не в титре — блюрим оригинал, чтоб не ликовал.
        let in_title = |r: &R| -> bool {
            groups.iter().any(|g| {
                let (_, gy, _, gh) = g.bbox;
                gy <= r.cy() && r.cy() <= gy + gh
            })
        };
        let taglines: Vec<BlurBox> = if ctx.do_translate && !ctx.fresh_subs {
            loc.iter()
                .filter(|r| r.t0 <= 3.0 && alpha_count(&r.text) >= 4 && centered(r.x, r.w) && !in_title(r))
                .map(region_box)
                .collect()
        } else {
            Vec::new()
        };
        let mut v: Vec<BlurBox> = loc_blocks.iter().map(locblock_box).collect();
        v.extend(taglines);
        v.extend(group_blur);
        v.extend_from_slice(band_blur);
        v
    };

    // ── записать в Project ─────────────────────────────────────────────────────
    proj.captions.titles = loc_blocks.iter().map(locblock_to_title).collect();
    let mut final_blur = final_blur;
    let hint: Option<(bool, Option<String>)> = if !ctitles.is_empty() {
        proj.captions.sub_style.as_ref().map(|s| (s.scene_flat, s.scene_color.clone()))
    } else {
        None
    };
    auto_fill_boxes(
        &ctx.paths.input,
        &mut final_blur,
        hint.as_ref().map(|(f, c)| (*f, c.as_deref())),
        ctx.vw,
        ctx.vh,
    );
    proj.captions.blur_boxes = final_blur;

    // ── sub_px (порт 608-610) ──────────────────────────────────────────────────
    let sub_y = proj.captions.sub_y;
    let mut sh: Vec<f32> = if let Some(sy) = sub_y {
        loc.iter()
            .filter(|r| (r.cy() - sy as f32).abs() <= vh * 0.07)
            .map(|r| r.h)
            .collect()
    } else {
        Vec::new()
    };
    sh.sort_by(f32_asc);
    let mut bh: Vec<i64> = caption_boxes.iter().map(|b| b.3).collect();
    bh.sort_unstable();
    let sub_px: Option<i64> = if !sh.is_empty() {
        Some(sh[sh.len() / 2] as i64)
    } else {
        cap_px
    }
    .or_else(|| if bh.is_empty() { None } else { Some(bh[bh.len() / 2]) });
    if let Some(px) = sub_px {
        proj.captions
            .raw_plan
            .insert("sub_px".into(), Value::from(px));
    }

    emit(
        progress,
        "ocr_detect",
        &t!(
            "compose-summary",
            titles = proj.captions.titles.len(),
            boxes = proj.captions.blur_boxes.len(),
            sub_px = format!("{sub_px:?}")
        ),
    );
}

// ─── helpers ────────────────────────────────────────────────────────────────

/// Выбор накрытия per-box (порт _auto_fill_boxes, pipeline.py 48-99): фон бокса почти однотонный ->
/// fill его цветом (при плоской сцене и совпадении — scene_color от Gemma); текстура -> блюр.
/// Фон меряется по верхней+нижней полосам бокса (текст центрирован — полосы чистые) на 3 кадрах
/// спана, берётся лучший: длинные коалесцированные боксы на части кадров содержат чужой текст.
fn auto_fill_boxes(
    video: &std::path::Path,
    boxes: &mut [BlurBox],
    hint: Option<(bool, Option<&str>)>,
    vw: i64,
    vh: i64,
) {
    let g_flat = hint.map(|(f, _)| f).unwrap_or(false);
    let g_col: Option<String> = hint
        .and_then(|(_, c)| c)
        .filter(|c| c.starts_with('#') && c.len() >= 7)
        .map(|c| c.to_string());
    for b in boxes.iter_mut() {
        let x0 = b.x.max(0);
        let y0 = b.y.max(0);
        let x1 = (b.x + b.w).min(vw);
        let y1 = (b.y + b.h).min(vh);
        if x1 - x0 < 6 || y1 - y0 < 8 {
            continue;
        }
        let best = [0.25, 0.5, 0.75]
            .iter()
            .filter_map(|q| {
                let t = (b.t0 + (b.t1 - b.t0) * q).max(0.0);
                box_bg_colour(video, x0, y0, x1 - x0, y1 - y0, t)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let Some((dom, frac)) = best else { continue };
        if let Some(fill) = fill_choice(g_flat, g_col.as_deref(), &dom, frac as f64) {
            b.fill = Some(fill);
        }
    }
}

/// Решение авто-заливки бокса (чистая функция, тестируемая без ffmpeg). Порт _auto_fill_boxes (pipeline.py:83-86):
/// g_flat (Gemma: плоская подложка) -> ВСЕГДА solid, предпочесть scene-цвет g_col безусловно, иначе доминанту;
/// без хинта -> solid лишь если бокс подавляюще одноцветный (frac>=0.72), иначе None (блюр).
fn fill_choice(g_flat: bool, g_col: Option<&str>, dom: &str, frac: f64) -> Option<String> {
    if g_flat {
        Some(g_col.map(|c| c.to_string()).unwrap_or_else(|| dom.to_string()))
    } else if frac >= 0.72 {
        Some(dom.to_string())
    } else {
        None
    }
}

/// Цвет фона бокса в момент t: доминанта верхней+нижней полос ROI ("#rrggbb", доля). Сбой -> None.
fn box_bg_colour(
    video: &std::path::Path,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    t: f64,
) -> Option<(String, f64)> {
    let ffmpeg = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let out = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-ss", &format!("{t:.3}")])
        .arg("-i")
        .arg(video)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("crop={w}:{h}:{x}:{y}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stride = (w as usize) * 3;
    let rows = out.stdout.len() / stride;
    if rows < 8 {
        return None;
    }
    let ins = (rows / 6).max(4);
    let mut strips = Vec::with_capacity(2 * ins * stride);
    for r in (0..ins).chain(rows - ins..rows) {
        strips.extend_from_slice(&out.stdout[r * stride..(r + 1) * stride]);
    }
    dominant_colour_rgb24(&strips)
}

/// Доминирующий цвет RGB24-буфера с квантованием по 24 (питоновские бины //24): усреднённый hex + доля.
fn dominant_colour_rgb24(rgb: &[u8]) -> Option<(String, f64)> {
    let n = rgb.len() / 3;
    if n == 0 {
        return None;
    }
    let key = |p: &[u8]| -> u32 {
        ((p[0] as u32 / 24) << 16) | ((p[1] as u32 / 24) << 8) | (p[2] as u32 / 24)
    };
    let mut counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for p in rgb.chunks_exact(3) {
        *counts.entry(key(p)).or_insert(0) += 1;
    }
    let (&bk, &bc) = counts.iter().max_by_key(|(_, c)| **c)?;
    let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
    for p in rgb.chunks_exact(3) {
        if key(p) == bk {
            sr += p[0] as u64;
            sg += p[1] as u64;
            sb += p[2] as u64;
        }
    }
    let m = bc as u64;
    Some((
        format!("#{:02x}{:02x}{:02x}", sr / m, sg / m, sb / m),
        bc as f64 / n as f64,
    ))
}

fn yfrac(t: &Value) -> f64 {
    t.get("y_frac").and_then(|v| v.as_f64()).unwrap_or(0.0)
}
fn sval(t: &Value, k: &str) -> Option<String> {
    t.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
}
fn bval(t: &Value, k: &str, d: bool) -> bool {
    t.get(k).and_then(|v| v.as_bool()).unwrap_or(d)
}

/// Ближайший титр по y (argmin |ycs[j] - cy|). Порт min(range(len), key=|abs|).
fn nearest_title(ycs: &[f32], cy: f32) -> usize {
    ycs.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (**a - cy).abs().partial_cmp(&(**b - cy).abs()).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Слить титры на (почти) одной линии (y_frac diff <= 0.06) — tgt через \n. Порт _merged.
fn merge_titles_by_y(sorted: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for t in sorted {
        if let Some(last) = merged.last_mut() {
            if (yfrac(&t) - yfrac(last)).abs() <= 0.06 {
                let prev = last.get("tgt").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let cur = t.get("tgt").and_then(|x| x.as_str()).unwrap_or("");
                // склеиваем строки, но одинаковые (vision дублирует персистентный тайтл) не повторяем
                let mut lines: Vec<String> = prev.split('\n').map(|s| s.to_string()).collect();
                if !cur.trim().is_empty() && !lines.iter().any(|l| norm_alnum(l) == norm_alnum(cur)) {
                    lines.push(cur.to_string());
                }
                let joined = lines.join("\n").trim().to_string();
                if let Value::Object(m) = last {
                    m.insert("tgt".into(), Value::from(joined));
                }
                continue;
            }
        }
        merged.push(t);
    }
    dedup_substring_titles(merged)
}

/// Дубли от vision: один титр — подстрока другого на соседней строке (Δy<=0.13, чуть за порогом
/// мёржа), напр. «СДВГ» и «СДВГ и перфекционизм.» — рисовались оба стопкой. Оставляем более полный.
fn dedup_substring_titles(titles: Vec<Value>) -> Vec<Value> {
    let norm = |v: &Value, k: &str| norm_alnum(v.get(k).and_then(|x| x.as_str()).unwrap_or(""));
    let mut keep = vec![true; titles.len()];
    for i in 0..titles.len() {
        for j in 0..titles.len() {
            if i == j || !keep[i] || !keep[j] {
                continue;
            }
            let (ti, tj) = (norm(&titles[i], "text"), norm(&titles[j], "text"));
            if ti.is_empty() || tj.is_empty() {
                continue;
            }
            // i — подстрока j (и короче), близко по y → i лишний
            if ti != tj && tj.contains(&ti) && (yfrac(&titles[i]) - yfrac(&titles[j])).abs() <= 0.13 {
                keep[i] = false;
            }
        }
    }
    titles.into_iter().zip(keep).filter(|(_, k)| *k).map(|(t, _)| t).collect()
}

/// region -> BlurBox (x,y,w,h,t0,t1). Порт region_blur_box (без роста — регион уже с pad detect_regions).
fn region_box(r: &R) -> BlurBox {
    let b = blur::region_blur_box(r.x, r.y, r.w, r.h, r.t0, r.t1);
    BlurBox { x: b.0 as i64, y: b.1 as i64, w: b.2 as i64, h: b.3 as i64, t0: b.4 as f64, t1: b.5 as f64, hidden: false, fill: None, extra: Default::default() }
}

/// (x,y,w,h)+time -> BlurBox с округлением времени (как group_blur/loc_block: bbox + start/end).
fn bbox_box(bbox: (f32, f32, f32, f32), t0: f32, t1: f32) -> BlurBox {
    BlurBox {
        x: bbox.0 as i64,
        y: bbox.1 as i64,
        w: bbox.2 as i64,
        h: bbox.3 as i64,
        t0: round2_f64(t0),
        t1: round2_f64(t1),
        hidden: false,
        fill: None,
        extra: Default::default(),
    }
}

fn locblock_box(b: &LocBlock) -> BlurBox {
    BlurBox {
        x: b.bbox.0 as i64,
        y: b.bbox.1 as i64,
        w: b.bbox.2 as i64,
        h: b.bbox.3 as i64,
        t0: round2_f64(b.start),
        t1: round2_f64(b.end),
        hidden: false,
        fill: None,
        extra: Default::default(),
    }
}

/// loc_block -> типизированный Title (bbox/tgt/цвет/стиль). Рисуется emit_title (bbox есть -> не скипнет).
fn locblock_to_title(b: &LocBlock) -> Title {
    Title {
        text: b.text.clone(),
        tgt: b.text.clone(), // уже переведён; map_title в render берёт tgt при непустом
        bbox: Some(vec![b.bbox.0 as i64, b.bbox.1 as i64, b.bbox.2 as i64, b.bbox.3 as i64]),
        color: b.color.clone(),
        bg: b.bg.clone(),
        font: b.font.clone(),
        italic: b.italic,
        align: b.align.clone(),
        start: b.start as f64,
        end: b.end as f64,
        lh: Some(b.lh as i64),
        solid: b.solid,
        bold: b.bold,
        size_px: None,
        outline: None,
        outline_w: None,
        shadow_dir: None,
        uppercase: false,
        extra: Default::default(),
    }
}

/// sub_style mirror-from-captions (порт 466-484): при пустом sub_style и непустых captions собрать
/// стиль из мод-значений капшенов + cap_px = медиана высот центр-нижних OCR-боксов.
fn ensure_sub_style_mirror(
    proj: &mut Project,
    captions_v: &[Value],
    loc: &[R],
    vw: f32,
    vh: f32,
    cap_px: &mut Option<i64>,
) {
    if proj.captions.sub_style.is_some() || captions_v.is_empty() {
        return;
    }
    let cx_lo = 0.40 * vw;
    let cx_hi = 0.60 * vw;
    let centered = |x: f32, w: f32| -> bool { x < cx_hi && x + w > cx_lo };
    let mode_str = |vals: Vec<String>, d: &str| -> String {
        let vals: Vec<String> = vals.into_iter().filter(|v| !v.is_empty()).collect();
        if vals.is_empty() {
            return d.to_string();
        }
        let mut counts: std::collections::HashMap<&String, usize> = std::collections::HashMap::new();
        for v in &vals {
            *counts.entry(v).or_insert(0) += 1;
        }
        // детерминизм как Counter.most_common: ничья -> первый по появлению (не HashMap-порядок)
        let maxc = counts.values().copied().max().unwrap_or(0);
        vals.iter().find(|v| counts[v] == maxc).cloned().unwrap_or_else(|| d.to_string())
    };
    // cap_px: медиана высот центр-нижних OCR-боксов (центр X, cy > 0.40*vh). Порт _caph.
    let mut caph: Vec<f32> = loc
        .iter()
        .filter(|r| centered(r.x, r.w) && r.cy() > vh * 0.40)
        .map(|r| r.h)
        .collect();
    caph.sort_by(f32_asc);
    if !caph.is_empty() {
        *cap_px = Some(caph[caph.len() / 2] as i64);
    }
    let colors: Vec<String> = captions_v.iter().filter_map(|c| sval(c, "color")).collect();
    let outlines: Vec<String> = captions_v
        .iter()
        .filter_map(|c| sval(c, "outline"))
        .filter(|o| o != "none")
        .collect();
    let fonts: Vec<String> = captions_v.iter().filter_map(|c| sval(c, "font")).collect();
    let n = captions_v.len() as f64;
    let bold = captions_v.iter().filter(|c| bval(c, "bold", false)).count() as f64 >= n / 2.0;
    let italic = captions_v.iter().filter(|c| bval(c, "italic", false)).count() as f64 > n / 2.0;
    let mut style = SubStyle {
        color: mode_str(colors, "#FFFFFF"),
        outline: mode_str(outlines, "#000000"),
        bold,
        italic,
        align: "center".to_string(),
        font: if fonts.is_empty() { None } else { Some(mode_str(fonts, "")) },
        ..SubStyle::default()
    };
    style.extra.insert("background".into(), Value::from("none"));
    style.extra.insert("solid".into(), Value::from(false));
    proj.captions.sub_style = Some(style);
}

/// white-card fallback (порт 490-493): sub_style без scene_color + среди titles есть solid светлый
/// (lum>0.7) фон -> scene_color=#FFFFFF, scene_flat=true (активирует cover_c-крышку в build).
fn white_card_fallback(proj: &mut Project, titles_v: &[Value]) {
    let Some(style) = proj.captions.sub_style.as_mut() else { return };
    if style.scene_color.is_some() {
        return;
    }
    let has_white_card = titles_v.iter().any(|t| {
        bval(t, "solid", false)
            && t.get("bg")
                .and_then(|v| v.as_str())
                .map(|bg| dub_captions_lum(bg) > 0.7)
                .unwrap_or(false)
    });
    if has_white_card {
        style.scene_color = Some("#FFFFFF".to_string());
        style.scene_flat = true;
    }
}

/// Люминанс hex (порт captions._lum). Дублирует look::lum (тот не экспортируется из dub-captions в
/// нужной форме для #hex -> делаем локально по той же формуле, чтоб не тянуть зависимость на приватный).
fn dub_captions_lum(hexstr: &str) -> f32 {
    dub_captions::lum(hexstr)
}

/// Опциональный таглайн-MT (порт 564-578): поднять text-only Gemma, перевести tcard_rows, нарисовать
/// с заливкой из sub_style. Fail-safe: без бинаря/весов/сбой — блюр всё равно накроет оригинал.
fn translate_taglines(
    proj: &mut Project,
    tcard_rows: &[&R],
    ctx: &ComposeCtx,
    loc_blocks: &mut Vec<LocBlock>,
    progress: &Progress,
) {
    use dub_translate::{flat_run, Seg};

    let paths = ctx.paths;
    // LLM-провайдер перевода: своя Gemma (плоский MT, без mmproj), локальный сервер или OpenRouter.
    let prov = match crate::llm_provider::open(
        &crate::llm_provider::LlmOpen {
            llama_bin: &paths.llama_bin,
            mt_model: &paths.mt_model,
            mmproj: &paths.mmproj,
            models_root: &paths.models_root,
        },
        crate::llm_provider::LlmMode::Text,
    ) {
        Ok(p) => p,
        Err(e) => {
            emit(progress, "ocr_detect", &t!("compose-taglines-no-mt", error = e.to_string()));
            return;
        }
    };
    emit(progress, "ocr_detect", &t!("compose-taglines-translating"));
    let client = prov.client();
    let mut segs: Vec<Seg> = tcard_rows.iter().map(|r| Seg::new(r.text.clone(), 0)).collect();
    let ok = flat_run(client, &mut segs, ctx.src_lang, &proj.tgt_lang, false, &proj.audio.translate_style).is_ok();
    drop(prov);
    if !ok {
        emit(progress, "ocr_detect", &t!("compose-taglines-failed"));
        return;
    }
    // стиль заливки = sub_style (как ss в питоне).
    let ss = proj.captions.sub_style.clone().unwrap_or_default();
    let ss_solid = ss.extra.get("solid").and_then(|v| v.as_bool()).unwrap_or(false);
    let ss_bg = ss
        .extra
        .get("background")
        .and_then(|v| v.as_str())
        .filter(|b| ss_solid && *b != "none")
        .map(|s| s.to_string());
    for (r, s2) in tcard_rows.iter().zip(segs.iter()) {
        let t2 = s2.tgt.trim();
        if t2.is_empty() {
            continue;
        }
        loc_blocks.push(LocBlock {
            bbox: (r.x as i32, r.y as i32, r.w as i32, r.h as i32),
            text: t2.to_string(),
            start: r.t0,
            end: r.t1,
            lh: (r.h * 0.8) as i32,
            color: Some(ss.color.clone()),
            bg: ss_bg.clone(),
            solid: ss_solid,
            align: ss.align.clone(),
            bold: ss.bold,
            italic: ss.italic,
            font: ss.font.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn fill_choice_matches_python() {
        // g_flat -> ВСЕГДА solid, scene-цвет g_col предпочтителен даже при низком frac
        assert_eq!(fill_choice(true, Some("#123456"), "#ffffff", 0.1), Some("#123456".into()));
        // g_flat без g_col -> доминанта, даже при низком frac
        assert_eq!(fill_choice(true, None, "#abcdef", 0.1), Some("#abcdef".into()));
        // не flat, frac>=0.72 -> доминанта
        assert_eq!(fill_choice(false, None, "#222222", 0.72), Some("#222222".into()));
        // не flat, frac<0.72 -> None (блюр), g_col игнорируется
        assert_eq!(fill_choice(false, Some("#123456"), "#222222", 0.5), None);
    }

    #[test]
    fn substring_title_dupe_dropped_adhd() {
        // «СДВГ» ⊂ «СДВГ и перфекционизм.» на Δy=0.0601 (за порогом мёржа 0.06) — остаётся полный.
        let t = |text: &str, y: f64| serde_json::json!({"text": text, "tgt": text, "y_frac": y});
        let out = merge_titles_by_y(vec![t("СДВГ", 0.134), t("СДВГ и перфекционизм.", 0.1941)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["text"], "СДВГ и перфекционизм.");
        // НЕ подстрока — оба живут
        let out2 = merge_titles_by_y(vec![t("Утка", 0.1), t("Совсем другой титр", 0.3)]);
        assert_eq!(out2.len(), 2);
    }

    #[test]
    fn identical_persistent_title_not_doubled_boris() {
        // Персистентный тайтл vision вернул дважды на соседних y (0.17/0.185) — склейка НЕ повторяет
        // одинаковую строку (иначе «POV:…\nPOV:…» вдвое строк -> кегль ужимается до нечитаемого).
        let t = |tgt: &str, y: f64| serde_json::json!({"text": tgt, "tgt": tgt, "y_frac": y});
        let out = merge_titles_by_y(vec![
            t("POV: Ты пытаешься затащить славянку к себе домой.", 0.17),
            t("POV: Ты пытаешься затащить славянку к себе домой.", 0.185),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["tgt"], "POV: Ты пытаешься затащить славянку к себе домой.");
        assert!(!out[0]["tgt"].as_str().unwrap().contains('\n'), "одна строка, без дубля");
    }

    fn reg(text: &str, x: i64, y: i64, w: i64, h: i64, t0: f32, t1: f32) -> Region {
        Region { text: text.into(), x, y, w, h, t0, t1 }
    }

    fn dummy_paths() -> AnalyzePaths {
        // Все пути несуществующие -> таглайн-MT никогда не поднимется (fail-safe no-op в тестах).
        let p = PathBuf::from("/nonexistent");
        AnalyzePaths {
            input: p.clone(),
            work_dir: p.clone(),
            repo_root: p.clone(),
            asr: crate::models::AsrChoice::Parakeet(p.clone()),
            sortformer_onnx: p.clone(),
            llama_bin: p.clone(),
            mt_model: p.clone(),
            mmproj: p.clone(),
            models_root: p.clone(),
            caption_fps: 4,
            import_subs: None,
            bsroformer_cli: p.clone(),
            bsroformer_model: p,
        }
    }

    // Эталон питона (см. отчёт, standalone-прогон блока pipeline.py:500-543):
    //  ctitle 'КОРОЧЕ ПОДКЛЮЧЕН'/tgt 'IN SHORT CONNECTED', y_frac=84/1280, два localize-бокса разделяют
    //  слова -> bbox (150,80,370,46), start 0.0, end 1.5, lh 42. Боковой low-бокс вне 13%vh -> не матчится.
    #[test]
    fn ctitle_bbox_matches_localize_boxes() {
        let vw = 720i64;
        let vh = 1280i64;
        let localize = vec![
            reg("КОРОЧЕ", 150, 80, 180, 44, 0.0, 1.4),
            reg("ПОДКЛЮЧЕН", 300, 84, 220, 42, 0.2, 1.5),
            reg("random", 600, 700, 90, 30, 5.0, 6.0),
        ];
        let mut proj = Project::default();
        proj.mode = "dub".into();
        proj.tgt_lang = "en".into();
        proj.raw_ctx.insert(
            "titles".into(),
            serde_json::json!([{
                "text": "КОРОЧЕ ПОДКЛЮЧЕН", "tgt": "IN SHORT CONNECTED",
                "y_frac": 84.0 / 1280.0, "color": "#FFFFFF", "bg": null, "solid": false,
                "align": "center", "bold": true, "italic": false, "font": null
            }]),
        );
        let paths = dummy_paths();
        let ctx = ComposeCtx {
            vw, vh, total: 30.0, do_translate: true, fresh_subs: false,
            src_lang: "ru", paths: &paths,
        };
        let noop: Box<Progress> = Box::new(|_| {});
        run(&mut proj, &localize, &[], &[], &ctx, &noop);

        assert_eq!(proj.captions.titles.len(), 1);
        let t = &proj.captions.titles[0];
        assert_eq!(t.bbox.as_deref(), Some(&[150i64, 80, 370, 46][..]));
        assert_eq!(t.tgt, "IN SHORT CONNECTED");
        assert_eq!(t.start, 0.0);
        assert_eq!(t.end, 1.5);
        assert_eq!(t.lh, Some(42));
        // титр -> нарисован -> его bbox в блюр-боксах (не просвечивает EN-оригинал).
        assert!(proj
            .captions
            .blur_boxes
            .iter()
            .any(|b| b.x == 150 && b.y == 80 && b.w == 370 && b.h == 46));
    }

    // Fallback центр-полоса: нет localize-боксов на y титра -> bbox = центр (8%..92% ширины), y=yc±4.5%vh.
    #[test]
    fn ctitle_fallback_center_band() {
        let vw = 720i64;
        let vh = 1280i64;
        let mut proj = Project::default();
        proj.mode = "dub".into();
        proj.raw_ctx.insert(
            "titles".into(),
            serde_json::json!([{
                "text": "SOME TITLE", "tgt": "НЕКИЙ ТИТР", "y_frac": 0.2, "align": "center"
            }]),
        );
        let paths = dummy_paths();
        let ctx = ComposeCtx {
            vw, vh, total: 30.0, do_translate: true, fresh_subs: false,
            src_lang: "en", paths: &paths,
        };
        let noop: Box<Progress> = Box::new(|_| {});
        run(&mut proj, &[], &[], &[], &ctx, &noop);
        assert_eq!(proj.captions.titles.len(), 1);
        let t = &proj.captions.titles[0];
        let yc = 0.2 * vh as f64; // 256
        let bbox = t.bbox.as_ref().unwrap();
        assert_eq!(bbox[0], (vw as f64 * 0.08) as i64); // 57
        assert_eq!(bbox[2], (vw as f64 * 0.92) as i64 - (vw as f64 * 0.08) as i64); // 662-57=605
        assert_eq!(bbox[1], (yc - vh as f64 * 0.045) as i64); // 256-57=199
    }

    // Титр обязан получить bbox и отрисоваться —
    // тот артефакт создан ДО compose-фикса (project.json 00:59 < f6fd476 01:51), титр шёл из raw_ctx с
    // bbox=null, emit_title его скипал. КОРЕНЬ: композит не вызывался/не проставлял bbox. Текущий run
    // ОБЯЗАН всегда давать титру валидный 4-эл. bbox (матч localize ИЛИ fallback центр-полоса) при
    // непустом tgt — иначе титр молча пропадает. Здесь воспроизведён ТОЧНЫЙ титр ae61067
    // ("THAT VERY PROGRESSIVE ACQUAINTANCE", y_frac=0.54) в ДВУХ входах: (1) с localize-боксами на его
    // y, (2) с ПУСТЫМ localize (OCR ничего не дал на этом кадре). Оба -> ровно 1 нарисованный титр с
    // непустым tgt и bbox.len()==4 (никогда 0 титров).
    #[test]
    fn ae61067_title_always_drawn_with_bbox() {
        let vw = 464i64;
        let vh = 824i64;
        let yc = (0.54 * vh as f64) as i64; // 444
        let mk = |localize: &[Region]| -> Project {
            let mut proj = Project::default();
            proj.mode = "dub".into();
            proj.raw_ctx.insert(
                "titles".into(),
                serde_json::json!([{
                    "text": "ТОТ САМЫЙ ПРОГРЕССИВНЫЙ ЗНАКОМЫЙ",
                    "tgt": "THAT VERY \"PROGRESSIVE\" ACQUAINTANCE",
                    "y_frac": 0.54, "color": "#FFFFFF", "bg": null, "solid": false
                }]),
            );
            let paths = dummy_paths();
            let ctx = ComposeCtx {
                vw, vh, total: 60.0, do_translate: false, fresh_subs: false,
                src_lang: "ru", paths: &paths,
            };
            let noop: Box<Progress> = Box::new(|_| {});
            run(&mut proj, localize, &[], &[], &ctx, &noop);
            proj
        };
        // (1) localize-боксы на y титра -> матч (bbox из них).
        let with_ocr = vec![
            reg("ТОТ САМЫЙ", 90, yc, 180, 40, 0.0, 1.5),
            reg("ЗНАКОМЫЙ", 100, yc + 42, 160, 40, 0.0, 1.5),
        ];
        for localize in [with_ocr.as_slice(), &[]] {
            let proj = mk(localize);
            assert_eq!(proj.captions.titles.len(), 1, "ровно 1 титр (не 0!) для localize={}", localize.len());
            let t = &proj.captions.titles[0];
            assert_eq!(t.tgt, "THAT VERY \"PROGRESSIVE\" ACQUAINTANCE");
            let bbox = t.bbox.as_ref().expect("bbox не должен быть null (иначе emit_title скипнет титр)");
            assert_eq!(bbox.len(), 4, "bbox из 4 элементов");
            assert!(bbox[2] > 0 && bbox[3] > 0, "ненулевые w/h: {bbox:?}");
        }
    }

    // Блюр-блид: 4-строчный EN-титр на ярком небе. Gemma
    // отдала ОДИН титр y_frac=0.17 (верхняя строка). OCR детектил все строки, но СРЕДНЮЮ склеил в
    // бесшовный токен "tomyplacelineonaSlavic" — она НЕ делит слово с текстом титра -> выпадала из shared,
    // titr-bbox накрывал только 1-ю строку (y 162..232), а средняя (y 207..281) ПРОСВЕЧИВАЛА под русским
    // сабом. Фикс: дорастить набор до contiguous-стопки титра (v_adjacent && h_overlap). Проверяем: ровно 1
    // титр, его bbox накрывает ВСЮ стопку (y0<=162, y1>=281) -> средняя строка внутри блюра.
    #[test]
    fn boris_glued_middle_title_line_covered() {
        let vw = 720i64;
        let vh = 1280i64;
        // localize из живого OCR boris (первые титр-строки; средняя — спейслес-токен).
        let localize = vec![
            reg("POV:Youtrythecomeback", 65, 162, 541, 70, 0.0, 28.5),
            reg("tomyplacelineonaSlavic", 79, 207, 513, 74, 0.0, 28.5),
            reg("gir", 287, 250, 98, 81, 0.0, 2.25),
            // субтитр-полоса — НЕ должна попасть в титр-стопку (вне yc±13%vh).
            reg("NICE,ExPENSIVEDINNER", 231, 865, 257, 52, 0.5, 1.5),
        ];
        let mut proj = Project::default();
        proj.mode = "dub".into();
        proj.tgt_lang = "ru".into();
        proj.raw_ctx.insert(
            "titles".into(),
            serde_json::json!([{
                "text": "POV: You try the \"come back to my place\" line on a Slavic girl",
                "tgt": "POV: Ты пытаешься затащить славянку к себе домой.",
                "y_frac": 0.17, "color": "#FFFFFF", "bg": null, "solid": false,
                "align": "center", "bold": true, "italic": false, "font": "Oswald"
            }]),
        );
        let paths = dummy_paths();
        let ctx = ComposeCtx {
            vw, vh, total: 28.54, do_translate: true, fresh_subs: false,
            src_lang: "en", paths: &paths,
        };
        let noop: Box<Progress> = Box::new(|_| {});
        run(&mut proj, &localize, &[], &[], &ctx, &noop);

        assert_eq!(proj.captions.titles.len(), 1, "ровно 1 нарисованный титр");
        let t = &proj.captions.titles[0];
        let bbox = t.bbox.as_ref().expect("bbox не null");
        let (y0, h) = (bbox[1], bbox[3]);
        let y1 = y0 + h;
        assert!(y0 <= 162, "верх стопки накрыт: y0={y0} (ожидалось <=162)");
        assert!(
            y1 >= 281,
            "НИЗ стопки накрыт (средняя строка y207..281 не просвечивает): y1={y1} (ожидалось >=281)"
        );
        // средняя строка (cy=244) обязана лежать внутри блюр-бокса титра.
        assert!(
            proj.captions.blur_boxes.iter().any(|b| {
                let by1 = b.y + b.h;
                b.y <= 207 && by1 >= 281 && b.x <= 79 && b.x + b.w >= 79 + 513
            }),
            "блюр-бокс накрывает средную строку целиком; боксы={:?}",
            proj.captions
                .blur_boxes
                .iter()
                .map(|b| (b.x, b.y, b.w, b.h))
                .collect::<Vec<_>>()
        );
    }

    // Over-blur: стопка-расширение НЕ должна цеплять ПОЗЖЕ
    // появившийся капшен/лейбл той же колонки. Два реальных случая autistic:
    //  (1) титр "Autistic Boyfriend" (t0..3.25, стопка y144/232) + позже "I Love You..." (t50, та же
    //      вертикаль) -> не тянуть спан к t50 (word-anchor overlap режет).
    //  (2) титр "Explained by Ducks" на t1.5-3.25, но OCR слепил строку в "ExplainedbyDucks" -> shared
    //      ПУСТ, а рядом scene-label "SWETCHILL" на t27 в той же колонке. Пустой shared не должен делать
    //      anchor=ВЕСЬ cand (t1.5..28.25) -> спан растягивался до 28.25с (плашка на всё видео). Fallback
    //      на ЕДИНСТВЕННЫЙ ближайший к yc cand-ряд оставляет спан 1.5-3.25.
    #[test]
    fn later_same_column_caption_not_chained() {
        let vw = 720i64;
        let vh = 1280i64;
        let localize = vec![
            reg("Autistic", 215, 144, 301, 89, 0.0, 3.25),
            reg("Boyfriend", 196, 232, 372, 111, 0.0, 3.25),
            // titr2 «Explained by Ducks» — OCR слепил в бесшовный токен (shared будет ПУСТ).
            reg("ExplainedbyDucks", 89, 826, 576, 111, 1.5, 3.25),
            // scene-label той же колонки на t27 — НЕ часть titr2.
            reg("SWETCHILL", 285, 732, 147, 58, 27.0, 28.25),
            // ПОЗЖЕ (t50) под titr1 — НЕ часть титра.
            reg("ILoveYou", 319, 227, 256, 79, 50.5, 51.25),
        ];
        let mut proj = Project::default();
        proj.mode = "dub".into();
        proj.raw_ctx.insert(
            "titles".into(),
            serde_json::json!([
                {"text": "Autistic Boyfriend", "tgt": "Аутичный парень",
                 "y_frac": 232.0 / 1280.0, "color": "#FFFFFF", "solid": false},
                {"text": "Explained by Ducks", "tgt": "Объясняют утки",
                 "y_frac": 826.0 / 1280.0, "color": "#FFFFFF", "solid": false}
            ]),
        );
        let paths = dummy_paths();
        let ctx = ComposeCtx {
            vw, vh, total: 61.0, do_translate: false, fresh_subs: false,
            src_lang: "en", paths: &paths,
        };
        let noop: Box<Progress> = Box::new(|_| {});
        run(&mut proj, &localize, &[], &[], &ctx, &noop);
        assert_eq!(proj.captions.titles.len(), 2, "два титра");
        // titr1: накрывает обе строки (y144..343), НЕ тянется к t50.
        let t1 = proj.captions.titles.iter().find(|t| t.bbox.as_ref().unwrap()[1] < 500).unwrap();
        let b1 = t1.bbox.as_ref().unwrap();
        assert!(b1[1] <= 144 && b1[1] + b1[3] >= 343, "titr1 накрывает обе строки: {b1:?}");
        assert!(t1.end <= 3.5, "titr1 спан НЕ растянут: end={}", t1.end);
        // titr2 (glued-token, shared пуст): спан 1.5-3.25, НЕ растянут до t27 SWETCHILL.
        let t2 = proj.captions.titles.iter().find(|t| t.bbox.as_ref().unwrap()[1] >= 500).unwrap();
        assert!(
            t2.end <= 3.5,
            "titr2 (склеенный токен) спан НЕ растянут scene-label t27: end={} (ожидалось <=3.5)",
            t2.end
        );
    }

    // white-card fallback: sub_style без scene_color + solid светлый (lum>0.7) титр -> scene_color=#FFFFFF.
    #[test]
    fn white_card_activates_cover() {
        let vw = 720i64;
        let vh = 1280i64;
        let mut proj = Project::default();
        proj.captions.sub_style = Some(SubStyle::default()); // scene_color=None
        proj.raw_ctx.insert(
            "titles".into(),
            serde_json::json!([{"text": "T", "tgt": "Т", "y_frac": 0.1, "solid": true, "bg": "#FFFFFF"}]),
        );
        let paths = dummy_paths();
        let ctx = ComposeCtx {
            vw, vh, total: 30.0, do_translate: true, fresh_subs: false,
            src_lang: "en", paths: &paths,
        };
        let noop: Box<Progress> = Box::new(|_| {});
        run(&mut proj, &[], &[], &[], &ctx, &noop);
        let ss = proj.captions.sub_style.as_ref().unwrap();
        assert_eq!(ss.scene_color.as_deref(), Some("#FFFFFF"));
        assert!(ss.scene_flat);
    }

    // sub_px: медиана высот OCR-боксов на sub_y (±7%vh). Эталон: боксы h=[40,44,48] на sub_y=1050 -> 44.
    #[test]
    fn sub_px_median_on_band() {
        let vw = 720i64;
        let vh = 1280i64;
        let sub_y = 1050i64;
        let localize = vec![
            reg("a", 200, 1040, 100, 40, 0.0, 1.0),
            reg("b", 200, 1044, 100, 44, 1.0, 2.0),
            reg("c", 200, 1046, 100, 48, 2.0, 3.0),
            reg("d", 200, 200, 100, 30, 0.0, 1.0), // вне полосы
        ];
        let mut proj = Project::default();
        proj.captions.sub_y = Some(sub_y);
        let paths = dummy_paths();
        let ctx = ComposeCtx {
            vw, vh, total: 30.0, do_translate: true, fresh_subs: false,
            src_lang: "ru", paths: &paths,
        };
        let noop: Box<Progress> = Box::new(|_| {});
        run(&mut proj, &localize, &[], &[], &ctx, &noop);
        assert_eq!(proj.captions.raw_plan.get("sub_px").and_then(|v| v.as_i64()), Some(44));
    }
}
