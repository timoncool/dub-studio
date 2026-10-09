//! PATCH /projects/{pid} — синхронные правки Project без GPU. Порт первых op из dubengine/api.py +
//! app.py.patch_project. В раунде 2 реализованы: segment (edit текста/voice/hidden/keep), subpos
//! (перетащить полосу субтитров), mode (dub/nodub/transcribe через set_mode). Прочие op -> 400
//! (реализуются в следующих раундах). Ошибки: неизвестный op -> 400; неизвестный seg id -> 404.

use dub_core::{BlurBox, CaptionOverride, SubStyle, Title, Project};
use serde_json::Value;

/// Результат применения op: Ok — Project изменён; Err — (http-код, сообщение).
pub type PatchResult = Result<(), (u16, String)>;

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string())
}

fn i(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64())
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64())
}

fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(|x| x.as_bool())
}

/// Собрать список id из edit["ids"] (массив строк).
fn ids(edit: &Value) -> Vec<String> {
    edit.get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

/// Собрать множество индексов из edit["idxs"], отсортировать по УБЫВАНИЮ (удалять с хвоста).
fn idxs_desc(edit: &Value) -> Vec<usize> {
    let set: std::collections::BTreeSet<i64> = edit
        .get("idxs")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
        .unwrap_or_default();
    set.iter().rev().filter_map(|&x| usize::try_from(x).ok()).collect::<Vec<_>>()
}

/// Пометить все сегменты dirty (после смены режима/перевода re-gen на render).
fn mark_all_dirty(p: &mut Project) {
    for seg in &mut p.segments {
        seg.dirty = true;
    }
}

/// Найти сегмент по edit["id"]: 400 если id нет, 404 если не найден. Общий шаг op_segment/op_regen/
/// op_hide_segment/op_keep_segment.
fn seg_by_id<'a>(p: &'a mut Project, edit: &Value) -> Result<&'a mut dub_core::Segment, (u16, String)> {
    let sid = s(edit, "id").ok_or((400, "missing segment id".into()))?;
    p.segments
        .iter_mut()
        .find(|x| x.id == sid)
        .ok_or((404, format!("segment {sid:?} not found")))
}

/// Id новой фразы: файлы фразы (seg-wav, ключ синтеза, дубли) зовутся по render::seg_file_id, а он
/// отбрасывает всё, кроме [A-Za-z0-9_], поэтому `s1.2` и `s12` — один файл. Занят id, чьё имя файла
/// уже у другой фразы; id без имени файла не годится.
fn free_id(p: &Project, wanted: Option<String>, fallback: impl Fn(usize) -> String) -> Result<String, (u16, String)> {
    let file_of = crate::render::seg_file_id;
    let taken = |id: &str| {
        let file = file_of(id);
        p.segments.iter().any(|x| x.id == id || (file.is_some() && file_of(&x.id) == file))
    };
    match wanted.filter(|x| !x.is_empty()) {
        Some(id) if file_of(&id).is_none() => Err((400, format!("segment id {id:?} has no letters or digits"))),
        Some(id) if taken(&id) => Err((409, format!("segment id {id:?} is taken"))),
        Some(id) => Ok(id),
        None => Ok((1..).map(fallback).find(|id| !taken(id)).unwrap_or_default()),
    }
}

/// Удалить элементы вектора по индексам edit["idxs"] (high->low, вне диапазона пропускаются).
fn del_by_idxs<T>(v: &mut Vec<T>, edit: &Value) {
    for idx in idxs_desc(edit) {
        if idx < v.len() {
            v.remove(idx);
        }
    }
}

/// Удалить ОДИН элемент по edit["idx"]: 400 если ключа нет, 404 если не число/вне диапазона.
fn del_one<T>(v: &mut Vec<T>, edit: &Value, what: &str) -> PatchResult {
    let idx = i(edit, "idx").ok_or((400, format!("missing {what} idx")))?;
    let idx = usize::try_from(idx).map_err(|_| (404, format!("bad {what} idx")))?;
    if idx >= v.len() {
        return Err((404, format!("{what} idx {idx} out of range")));
    }
    v.remove(idx);
    Ok(())
}

/// edit_segment — правка одной строки транскрипта. Порт api.edit_segment.
fn op_segment(p: &mut Project, edit: &Value) -> PatchResult {
    let timing_changed = edit.get("start").is_some() || edit.get("end").is_some();
    let seg = seg_by_id(p, edit)?;
    if let Some(t) = edit.get("tgt_text").and_then(|x| x.as_str()) {
        seg.tgt_text = t.to_string();
    }
    if let Some(t) = edit.get("src_text").and_then(|x| x.as_str()) {
        seg.src_text = t.to_string();
    }
    // правка тайминга: start/end (сек). Клампим end > start; порядок в списке не трогаем (рендер сортирует
    // по времени сам). Полезно, когда ASR-тайминг чуть разъехался с речью.
    if let Some(v) = edit.get("start").and_then(|x| x.as_f64()) {
        seg.start = v.max(0.0);
    }
    if let Some(v) = edit.get("end").and_then(|x| x.as_f64()) {
        seg.end = v.max(seg.start + 0.1);
    }
    if seg.end <= seg.start {
        seg.end = seg.start + 0.1;
    }
    // переброс фразы другому спикеру (в т.ч. НОВОМУ id, если ASR определил меньше спикеров, чем есть):
    // голос спикера задаётся в настройках голосов, здесь лишь меняем принадлежность -> ref_of при рендере
    // берёт реф целевого спикера. "" -> None (снять привязку).
    if let Some(sp) = edit.get("speaker") {
        seg.speaker = sp.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
    // hidden / keep_original — хранятся в extra (dub-core Segment их не типизирует, но проносит).
    if let Some(h) = edit.get("hidden").and_then(|x| x.as_bool()) {
        seg.extra.insert("hidden".into(), Value::Bool(h));
    }
    if let Some(k) = edit.get("keep_original").and_then(|x| x.as_bool()) {
        seg.extra.insert("keep_original".into(), Value::Bool(k));
    }
    seg.dirty = true;
    // Правка тайминга могла нарушить монотонность списка по времени, а render считает слот озвучки по
    // ИНДЕКСУ списка (nxt = segments[i+1].start) — поэтому пересортируем по start (как op_add_segment).
    // Кэш seg-файлов привязан к id сегмента (render.rs), переупорядочивание безопасно.
    if timing_changed {
        p.segments
            .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    }
    Ok(())
}

/// subpos — перетащить полосу субтитров вертикально; ставит sub_y_locked=true (honor для всех строк).
fn op_subpos(p: &mut Project, edit: &Value) -> PatchResult {
    let sub_y = edit
        .get("sub_y")
        .and_then(|x| x.as_i64())
        .ok_or((400, "bad subpos sub_y".to_string()))?;
    p.captions.sub_y = Some(sub_y);
    p.captions.sub_y_locked = true;
    Ok(())
}

/// mode — верхнеуровневый режим вывода. Порт api.set_mode:
///   subtitles -> nodub + subs.transcribe; dub, voiceover -> свой аудио-выход + subs.translate;
///   transcribe -> transcribe + subs.transcribe; funny -> dub + subs.translate + rewrite.
/// Язык субтитров, выбранный отдельно (subs_content), пресет не перебивает: «нет» — ни один пресет,
/// «оба» и «оригинал» под дубляжем или закадром — dub, voiceover и funny.
/// Помечает все сегменты dirty. ValueError (неизвестное значение) -> 400.
fn op_mode(p: &mut Project, edit: &Value) -> PatchResult {
    let value = edit.get("value").and_then(|x| x.as_str()).unwrap_or_default();
    // Композируемость: пресет режима НЕ трогает ЯВНЫЙ выбор «без субтитров». Иначе клик по чипу режима
    // (op_mode) воскрешал бы субтитры, которые юзер выключил через subs_content=none (баг-репорт code-review:
    // «subs=none всё равно прожигает субтитры» — через редактор). subs.mode остаётся под управлением
    // независимого subs_content-контрола, пресет задаёт лишь его дефолт, когда он НЕ «none».
    let keep_no_subs = p.subs.mode == "none";
    // «Оригинал» в nodub/transcribe — собственный дефолт пресетов subtitles/transcribe, а не выбор под
    // дубляж: при переходе к дубляжу он становится переводом. «Оба» ни один пресет не ставит.
    let keep_dub_subs = p.subs.mode == "bilingual"
        || (p.subs.mode == "transcribe" && matches!(p.mode.as_str(), "dub" | "voiceover"));
    let set_subs = |p: &mut Project, m: &str| {
        if !keep_no_subs {
            p.subs.mode = m.into();
        }
    };
    let set_dub_subs = |p: &mut Project| {
        if !keep_no_subs && !keep_dub_subs {
            p.subs.mode = "translate".into();
        }
    };
    match value {
        "subtitles" => {
            p.mode = "nodub".into();
            set_subs(p, "transcribe"); // субтитры = язык оригинала, без перевода
            p.audio.rewrite = None;
        }
        "dub" => {
            p.mode = "dub".into();
            set_dub_subs(p);
            p.audio.rewrite = None;
        }
        "voiceover" => {
            // закадровый: перевод+TTS поверх приглушённого оригинала (громкость — audio.voiceover_gain_db)
            p.mode = "voiceover".into();
            set_dub_subs(p);
            p.audio.rewrite = None;
        }
        "transcribe" => {
            // транскрипт+диаризация: без дубляжа/перевода, субтитры на языке оригинала
            p.mode = "transcribe".into();
            set_subs(p, "transcribe");
            p.audio.rewrite = None;
        }
        "funny" => {
            p.mode = "dub".into();
            set_dub_subs(p);
            if p.audio.rewrite.is_none() {
                p.audio.rewrite = Some("make it a funny, playful dub".into());
            }
        }
        other => return Err((400, format!("unknown mode {other:?}"))),
    }
    mark_all_dirty(p);
    Ok(())
}

/// dub — независимо задать аудио-выход: none (оригинал, без дубляжа) | dub | voiceover. Развязано от
/// субтитров и шуточного ремикса (audio.rewrite сохраняется) — можно комбинировать: шуточный дубляж +
/// свои голоса, дубляж без субтитров, перевод субтитров без дубляжа и т.д.
fn op_dub(p: &mut Project, edit: &Value) -> PatchResult {
    let v = edit.get("value").and_then(|x| x.as_str()).unwrap_or_default();
    match v {
        "none" => p.mode = "nodub".into(),
        "dub" => p.mode = "dub".into(),
        "voiceover" => p.mode = "voiceover".into(),
        other => return Err((400, format!("unknown audio output {other:?}"))),
    }
    // dub/voiceover требуют TTS -> пометить сегменты dirty (следующий /render синтезирует озвучку).
    if p.mode == "dub" || p.mode == "voiceover" {
        mark_all_dirty(p);
    }
    Ok(())
}

/// subs_burn — вжигать ли субтитры/титры на видео (композируемость: дубляж без сабов и т.п.).
/// {on: bool}. Меняет только наложение на выходе — НЕ трогает TTS (dirty не ставим), следующий
/// /render пересоберёт видео с учётом флага.
fn op_subs_burn(p: &mut Project, edit: &Value) -> PatchResult {
    p.subs.burn = edit.get("on").and_then(Value::as_bool).unwrap_or(true);
    Ok(())
}

/// subs_content — независимо задать содержимое субтитров: none (нет) | transcribe (язык оригинала) |
/// translate (перевод) | bilingual (перевод и оригинал второй строкой). Развязывает субтитры от
/// аудио-режима (перевод сабов без дубляжа, оригинал под дубляжем). Для двуязычных — order
/// (translation_top | original_top) и secondary {size_pct 40..=100, color #RRGGBB, opacity 10..=100;
/// null — как у основной строки}. Поля, которых нет, не меняются; всё проверяется до записи.
fn op_subs_content(p: &mut Project, edit: &Value) -> PatchResult {
    let mode = match edit.get("value") {
        None => None,
        Some(v) => match v.as_str() {
            Some(m @ ("none" | "transcribe" | "translate" | "bilingual")) => Some(m.to_string()),
            _ => return Err((400, format!("unknown subs content {v}"))),
        },
    };
    let mut bilingual = p.subs.bilingual.clone();
    if let Some(v) = edit.get("order") {
        match v.as_str() {
            Some(o @ (dub_core::ORDER_TRANSLATION_TOP | dub_core::ORDER_ORIGINAL_TOP)) => bilingual.order = o.to_string(),
            _ => return Err((400, format!("order is translation_top or original_top, not {v}"))),
        }
    }
    if let Some(sec) = edit.get("secondary") {
        let sec = sec.as_object().ok_or((400, "secondary is an object {size_pct, color, opacity}".to_string()))?;
        for key in sec.keys() {
            if !matches!(key.as_str(), "size_pct" | "color" | "opacity") {
                return Err((400, format!("secondary has no field {key:?}")));
            }
        }
        if let Some(v) = sec.get("size_pct") {
            match v.as_i64() {
                Some(n) if (40..=100).contains(&n) => bilingual.secondary.size_pct = n,
                _ => return Err((400, format!("secondary.size_pct is 40..100, not {v}"))),
            }
        }
        if let Some(v) = sec.get("color") {
            bilingual.secondary.color = match v {
                Value::Null => None,
                Value::String(c) if is_hex_rgb(c) => Some(c.to_uppercase()),
                _ => return Err((400, format!("secondary.color is #RRGGBB or null, not {v}"))),
            };
        }
        if let Some(v) = sec.get("opacity") {
            bilingual.secondary.opacity = match (v, v.as_i64()) {
                (Value::Null, _) => None,
                (_, Some(n)) if (10..=100).contains(&n) => Some(n),
                _ => return Err((400, format!("secondary.opacity is 10..100 or null, not {v}"))),
            };
        }
    }
    if mode.is_none() && edit.get("order").is_none() && edit.get("secondary").is_none() {
        return Err((400, "subs_content needs value, order or secondary".into()));
    }
    if let Some(m) = mode {
        p.subs.mode = m;
    }
    p.subs.bilingual = bilingual;
    Ok(())
}

/// #RRGGBB.
fn is_hex_rgb(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// translate — сменить целевой язык (+режим subs=translate; funny -> rewrite). Порт api.translate.
/// Двуязычные субтитры остаются двуязычными: перевод в них и так основная строка.
/// Помечает все сегменты dirty (перевод/дубляж перегенерятся на следующем analyze/render). Смена языка
/// требует ре-перевода, но analyze здесь не запускаем — это GPU-джоба; PATCH лишь фиксирует намерение.
fn op_translate(p: &mut Project, edit: &Value) -> PatchResult {
    // api.translate(project, lang, mode="plain"): tgt_lang=lang; subs=translate; funny -> rewrite.
    if let Some(lang) = s(edit, "lang") {
        p.tgt_lang = lang;
    }
    // Как в op_mode: выключенные субтитры остаются выключенными, «оба» и «оригинал» под дубляжем или
    // закадром остаются; транскрипт проекта субтитров становится переводом.
    let keep = matches!(p.subs.mode.as_str(), "none" | "bilingual")
        || (p.subs.mode == "transcribe" && matches!(p.mode.as_str(), "dub" | "voiceover"));
    if !keep {
        p.subs.mode = "translate".into();
    }
    if s(edit, "mode").as_deref() == Some("funny") {
        p.audio.rewrite = Some("make it a funny, playful dub".into());
    }
    mark_all_dirty(p);
    Ok(())
}

/// rewrite — задать творческую инструкцию ре-дубляжа. Порт api.rewrite: audio.rewrite=instruction;
/// mode=dub; все dirty. Пустая инструкция -> 400 (нечего переписывать).
fn op_rewrite(p: &mut Project, edit: &Value) -> PatchResult {
    let instr = s(edit, "instruction").unwrap_or_default();
    if instr.trim().is_empty() {
        return Err((400, "rewrite requires non-empty instruction".into()));
    }
    p.audio.rewrite = Some(instr);
    p.mode = "dub".into();
    mark_all_dirty(p);
    Ok(())
}

/// translate_style — задать стилевую инструкцию перевода (#112): доп-указание тона/регистра/лексики
/// («formal», «gen-z slang»). Дополняет перевод (в отличие от rewrite, который ЗАМЕНЯЕТ содержимое).
/// Текст нормализуем: trim, переводы строк -> пробелы, кап ~500 символов (как inline-инструкция промпта,
/// не абзац). Помечает все сегменты dirty тем же механизмом, что смена режима/rewrite: сам ре-перевод —
/// GPU-стадия (analyze/vision), PATCH лишь фиксирует намерение (как op_translate). Пустой style снимает
/// стиль (сохраняем ""). Стиль читает translate::stage из proj.audio.translate_style и вносит в sysmsg.
fn op_translate_style(p: &mut Project, edit: &Value) -> PatchResult {
    let raw = s(edit, "style").unwrap_or_default();
    // схлопнуть любые переводы строк/табы в одиночные пробелы, затем trim; кап 500 символов по границам char.
    let flat: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    p.audio.translate_style = flat.chars().take(500).collect();
    mark_all_dirty(p);
    Ok(())
}

/// recast — сменить режим/голос дубляжа. Порт api.recast: audio.voice.mode/name; все сегменты dirty
/// (следующий /render перегенерит дубляж с новым голосом). Порт app.py op=="recast" (раунд 4).
fn op_recast(p: &mut Project, edit: &Value) -> PatchResult {
    let mode = s(edit, "voice_mode").unwrap_or_else(|| "clone".into());
    p.audio.voice.mode = mode;
    p.audio.voice.name = s(edit, "voice_name");
    mark_all_dirty(p);
    Ok(())
}

/// Новый нонс «перегенерировать»: входит в ключ синтеза сегмента, поэтому рендер синтезирует
/// сегмент заново, даже если текст и голос не менялись (новый дубль вместо кэша).
fn mark_regen(seg: &mut dub_core::Segment) {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    seg.extra.insert(crate::render::REGEN_NONCE.into(), Value::String(nonce));
    seg.dirty = true;
}

/// regen — пометить ОДИН сегмент dirty, а ВСЕ ДРУГИЕ сегменты — NOT dirty (ре-TTS только его на /render).
fn op_regen(p: &mut Project, edit: &Value) -> PatchResult {
    let target_id = s(edit, "id").ok_or((400, "missing segment id".into()))?;
    let mut found = false;
    for s in &mut p.segments {
        if s.id == target_id {
            mark_regen(s);
            found = true;
        } else {
            s.dirty = false;
        }
    }
    if !found {
        return Err((404, format!("segment {target_id:?} not found")));
    }
    Ok(())
}

/// regen_all — ре-TTS всего дубляжа (новый нонс у каждого сегмента). Порт app.py op=="regen_all".
fn op_regen_all(p: &mut Project, _edit: &Value) -> PatchResult {
    for seg in &mut p.segments {
        mark_regen(seg);
    }
    Ok(())
}

/// add_segment — вставить ПОЛЬЗОВАТЕЛЬСКУЮ фразу (start/end/speaker[/tgt_text]), помеченную dirty →
/// на /render она синтезируется (Higgs клонирует голос спикера) и попадает в субтитры, как обычный сегмент.
/// Текст можно дописать потом (op "segment"). Список пересортировывается по start. Своей речи в источнике
/// нет — reference-голос берётся по speaker (render.rs ref_of: свой спикер → его клон, иначе первый).
fn op_add_segment(p: &mut Project, edit: &Value) -> PatchResult {
    let start = f(edit, "start").unwrap_or(0.0).max(0.0);
    let end = f(edit, "end").unwrap_or(start + 2.0).max(start + 0.2);
    // speaker: явный из запроса, иначе первый существующий (чтобы клон-голос был знакомым).
    let speaker = s(edit, "speaker").or_else(|| p.segments.first().and_then(|x| x.speaker.clone()));
    let id = free_id(p, s(edit, "id"), |n| format!("u{}", p.segments.len() + n))?;
    // Строим Segment через JSON — #[serde(flatten)] extra заполняется пустым объектом сам.
    let seg: dub_core::Segment = serde_json::from_value(serde_json::json!({
        "id": id, "start": start, "end": end, "speaker": speaker,
        "src_text": "", "tgt_text": s(edit, "tgt_text").unwrap_or_default(),
        "voice": Value::Null, "dirty": true,
    }))
    .map_err(|e| (400, format!("bad segment: {e}")))?;
    p.segments.push(seg);
    p.segments
        .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(())
}

/// gain — монтажный гейн всей дорожки (dB). НЕ помечает dirty: ре-TTS не нужен, применяется на рендере
/// поверх нормализации (сегменты берутся из кэша).
fn op_gain(p: &mut Project, edit: &Value) -> PatchResult {
    if let Some(g) = f(edit, "gain_db") {
        p.audio.gain_db = g.clamp(-24.0, 24.0);
    }
    Ok(())
}

/// loudness — выравнивание громкости дубляжа {on: bool}. Только пересведение, ре-TTS не нужен.
fn op_loudness(p: &mut Project, edit: &Value) -> PatchResult {
    if let Some(on) = edit.get("on").and_then(|v| v.as_bool()) {
        p.audio.loudness_normalize = on;
    }
    Ok(())
}

/// Громкость ОРИГИНАЛЬНОЙ дорожки в режиме voiceover (закадровый). 0 = в полную силу, отрицательное =
/// тише перевода. Ре-TTS не нужен — только пересведение (лёгкий ре-рендер), сегменты из кэша.
fn op_voiceover_gain(p: &mut Project, edit: &Value) -> PatchResult {
    if let Some(g) = f(edit, "gain_db") {
        p.audio.voiceover_gain_db = g.clamp(-40.0, 0.0);
    }
    Ok(())
}

/// sub_blur — блюр-подложка ПОД сожжёнными субтитрами {on: bool}. Опция (не всем нужна): выкл -> текст без
/// размытой подложки. Дефолт вкл. Только рендер-настройка (dirty не ставим — вжигание на экспорте).
fn op_sub_blur(p: &mut Project, edit: &Value) -> PatchResult {
    if let Some(on) = edit.get("on").and_then(|v| v.as_bool()) {
        p.render.blur = on;
    }
    Ok(())
}

/// keep_original — экспортировать вторую дорожку с оригиналом {keep: bool, container?: "mp4"|"mkv"}.
/// Только ремукс на экспорте (дубляж из кэша) — dirty не ставим. Неверный container -> 400.
fn op_keep_original(p: &mut Project, edit: &Value) -> PatchResult {
    p.audio.keep_original_track = b(edit, "keep").unwrap_or(true);
    if let Some(c) = s(edit, "container") {
        match c.as_str() {
            "mp4" | "mkv" => p.audio.container = c,
            other => return Err((400, format!("unknown container {other:?}"))),
        }
    }
    Ok(())
}

/// Наложить caption-поля стиля на SubStyle (типизированные — в поля, прочие — в extra passthrough).
/// Порт edit_caption._apply: неизвестных ключей нет (Pydantic валидирует), но extra="allow" сохраняет
/// vision-поля (background/scene_*). Здесь принимаем любые ключи стиля; типизированные кладём в поля,
/// остальные — в extra, чтобы map_sub_style (render.rs) их подхватил (в т.ч. plate/plate_color — тумблер
/// подложки).
fn apply_substyle_fields(st: &mut SubStyle, fields: &serde_json::Map<String, Value>) {
    for (k, v) in fields {
        match k.as_str() {
            "color" => {
                if let Some(x) = v.as_str() { st.color = x.to_string(); }
            }
            "outline" => {
                if let Some(x) = v.as_str() { st.outline = x.to_string(); }
            }
            "align" => {
                if let Some(x) = v.as_str() { st.align = x.to_string(); }
            }
            "font" => st.font = v.as_str().map(|x| x.to_string()),
            "scene_color" => st.scene_color = v.as_str().map(|x| x.to_string()),
            "italic" => {
                if let Some(x) = v.as_bool() { st.italic = x; }
            }
            "bold" => {
                if let Some(x) = v.as_bool() { st.bold = x; }
            }
            "uppercase" => {
                if let Some(x) = v.as_bool() { st.uppercase = x; }
            }
            "scene_flat" => {
                if let Some(x) = v.as_bool() { st.scene_flat = x; }
            }
            "n_lines" => st.n_lines = v.as_i64(),
            "size_px" => st.size_px = v.as_i64(),
            "outline_w" => st.outline_w = v.as_i64(),
            "shadow_dir" => st.shadow_dir = v.as_i64(), // null -> None (снять тень), int -> угол
            // Прочее (background, size_frac, solid, plate, plate_color, …) — в extra passthrough.
            _ => {
                st.extra.insert(k.clone(), v.clone());
            }
        }
    }
}

/// caption — правка стиля субтитров. seg_id=None -> ГЛОБАЛЬНЫЙ sub_style; иначе per-segment override.
/// Порт api.edit_caption + app.py op=="caption". Тумблер подложки: {op:caption, plate:false} снимает
/// продуктовую плашку глобально (или per-seg с seg_id).
fn op_caption(p: &mut Project, edit: &Value) -> PatchResult {
    // поля стиля = всё, кроме op/seg_id.
    let mut fields = serde_json::Map::new();
    if let Some(obj) = edit.as_object() {
        for (k, v) in obj {
            if k != "op" && k != "seg_id" {
                fields.insert(k.clone(), v.clone());
            }
        }
    }
    let seg_id = s(edit, "seg_id");
    match seg_id {
        None => {
            let mut st = p.captions.sub_style.take().unwrap_or_default();
            apply_substyle_fields(&mut st, &fields);
            p.captions.sub_style = Some(st);
        }
        Some(sid) => {
            let idx = p.captions.overrides.iter().position(|o| o.seg_id == sid);
            let ov = match idx {
                Some(i) => &mut p.captions.overrides[i],
                None => {
                    p.captions.overrides.push(CaptionOverride {
                        seg_id: sid.clone(),
                        ..Default::default()
                    });
                    p.captions.overrides.last_mut().unwrap()
                }
            };
            // Геометрия/текст override — типизированные поля; стиль — во вложенный SubStyle.
            if let Some(t) = fields.get("text").and_then(|v| v.as_str()) {
                ov.text = Some(t.to_string());
            }
            if let Some(x) = fields.get("x").and_then(|v| v.as_i64()) {
                ov.x = Some(x);
            }
            if let Some(y) = fields.get("y").and_then(|v| v.as_i64()) {
                ov.y = Some(y);
            }
            if let Some(w) = fields.get("w").and_then(|v| v.as_i64()) {
                ov.w = Some(w);
            }
            if let Some(fs) = fields.get("fs").and_then(|v| v.as_i64()) {
                ov.fs = Some(fs);
            }
            // Прочие поля -> вложенный style SubStyle (color/font/plate/…).
            let style_fields: serde_json::Map<String, Value> = fields
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "text" | "x" | "y" | "w" | "fs"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if !style_fields.is_empty() {
                let mut st = ov.style.take().unwrap_or_default();
                apply_substyle_fields(&mut st, &style_fields);
                ov.style = Some(st);
            }
        }
    }
    Ok(())
}

/// del_segment — удалить строку целиком (уходит субтитр И дубляж). Первый оставшийся -> dirty. Порт api.
fn op_del_segment(p: &mut Project, edit: &Value) -> PatchResult {
    let sid = s(edit, "id").ok_or((400, "missing segment id".into()))?;
    del_segment(p, &sid)
}

fn del_segment(p: &mut Project, sid: &str) -> PatchResult {
    let n = p.segments.len();
    p.segments.retain(|s| s.id != sid);
    if p.segments.len() == n {
        return Err((404, format!("segment {sid:?} not found")));
    }
    if let Some(first) = p.segments.first_mut() {
        first.dirty = true;
    }
    Ok(())
}

/// hide_segment — тоггл/установка hidden (в extra). Порт app.py op=="hide_segment".
fn op_hide_segment(p: &mut Project, edit: &Value) -> PatchResult {
    let seg = seg_by_id(p, edit)?;
    let cur = seg.extra.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false);
    let new = b(edit, "hidden").unwrap_or(!cur);
    seg.extra.insert("hidden".into(), Value::Bool(new));
    seg.dirty = true;
    Ok(())
}

/// del_segments — массовое удаление (несуществующие пропускаются). Порт app.py op=="del_segments".
fn op_del_segments(p: &mut Project, edit: &Value) -> PatchResult {
    for sid in ids(edit) {
        let _ = del_segment(p, &sid); // KeyError глотается, как в питоне
    }
    Ok(())
}

/// hide_segments — массовое скрытие (явный флаг). Порт app.py op=="hide_segments".
fn op_hide_segments(p: &mut Project, edit: &Value) -> PatchResult {
    let hid = b(edit, "hidden").unwrap_or(true);
    for sid in ids(edit) {
        if let Some(seg) = p.segments.iter_mut().find(|x| x.id == sid) {
            seg.extra.insert("hidden".into(), Value::Bool(hid));
            seg.dirty = true;
        }
    }
    Ok(())
}

/// keep_segment — тоггл keep_original (в extra). Порт app.py op=="keep_segment".
fn op_keep_segment(p: &mut Project, edit: &Value) -> PatchResult {
    let seg = seg_by_id(p, edit)?;
    let cur = seg.extra.get("keep_original").and_then(|v| v.as_bool()).unwrap_or(false);
    let new = b(edit, "keep").unwrap_or(!cur);
    seg.extra.insert("keep_original".into(), Value::Bool(new));
    seg.dirty = true;
    Ok(())
}

/// keep_segments — массовый keep_original (явный флаг). Порт app.py op=="keep_segments".
fn op_keep_segments(p: &mut Project, edit: &Value) -> PatchResult {
    let kp = b(edit, "keep").unwrap_or(true);
    for sid in ids(edit) {
        if let Some(seg) = p.segments.iter_mut().find(|x| x.id == sid) {
            seg.extra.insert("keep_original".into(), Value::Bool(kp));
            seg.dirty = true;
        }
    }
    Ok(())
}

/// del_titles — массовое удаление титров (high->low). Порт app.py op=="del_titles".
fn op_del_titles(p: &mut Project, edit: &Value) -> PatchResult {
    del_by_idxs(&mut p.captions.titles, edit);
    Ok(())
}

/// del_blurs — массовое удаление blur-боксов (high->low). Порт app.py op=="del_blurs".
fn op_del_blurs(p: &mut Project, edit: &Value) -> PatchResult {
    del_by_idxs(&mut p.captions.blur_boxes, edit);
    Ok(())
}

/// blur — правка геометрии/полей одного blur-бокса. Порт api.edit_blur (IndexError -> 404).
fn op_blur(p: &mut Project, edit: &Value) -> PatchResult {
    let idx = i(edit, "idx").ok_or((400, "missing blur idx".into()))?;
    let idx = usize::try_from(idx).map_err(|_| (404, "bad blur idx".to_string()))?;
    let bx = p
        .captions
        .blur_boxes
        .get_mut(idx)
        .ok_or((404, format!("bad blur idx: {idx} out of range")))?;
    if let Some(x) = i(edit, "x") { bx.x = x; }
    if let Some(y) = i(edit, "y") { bx.y = y; }
    if let Some(w) = i(edit, "w") { bx.w = w; }
    if let Some(h) = i(edit, "h") { bx.h = h; }
    if let Some(t0) = f(edit, "t0") { bx.t0 = t0; }
    if let Some(t1) = f(edit, "t1") { bx.t1 = t1; }
    if let Some(hidden) = b(edit, "hidden") { bx.hidden = hidden; }
    if let Some(v) = edit.get("fill") {
        bx.fill = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
    Ok(())
}

/// blur_add — новый blur-бокс (по умолчанию весь ролик). Порт api.add_blur. Отсутствие x/y/w/h -> 400.
fn op_blur_add(p: &mut Project, edit: &Value) -> PatchResult {
    let bad = |k: &str| (400, format!("bad blur_add: missing/invalid field {k:?}"));
    let x = i(edit, "x").ok_or_else(|| bad("x"))?;
    let y = i(edit, "y").ok_or_else(|| bad("y"))?;
    let w = i(edit, "w").ok_or_else(|| bad("w"))?;
    let h = i(edit, "h").ok_or_else(|| bad("h"))?;
    let t0 = f(edit, "t0").unwrap_or(0.0);
    let t1 = f(edit, "t1").unwrap_or(p.meta.duration);
    p.captions.blur_boxes.push(BlurBox {
        x, y, w, h, t0, t1, hidden: false, fill: None, extra: Default::default(),
    });
    Ok(())
}

/// blur_del — удалить blur-бокс по индексу. Порт api.del_blur (IndexError -> 404).
fn op_blur_del(p: &mut Project, edit: &Value) -> PatchResult {
    del_one(&mut p.captions.blur_boxes, edit, "blur")
}

/// blur_enable — глобальный тоггл блюра (render.blur). Порт app.py op=="blur_enable".
fn op_blur_enable(p: &mut Project, edit: &Value) -> PatchResult {
    p.render.blur = b(edit, "on").unwrap_or(true);
    Ok(())
}

/// preset — имя TEMPLATE-пресета (None/"match" = как оригинал); только re-burn. Порт app.py op=="preset".
fn op_preset(p: &mut Project, edit: &Value) -> PatchResult {
    // name отсутствует ИЛИ пустое -> None (match original).
    p.captions.preset.name = s(edit, "name").filter(|x| !x.is_empty());
    Ok(())
}

/// title — правка титра (текст/italic/font/color/bbox/тайминг). Порт api.edit_title (IndexError -> 404).
fn op_title(p: &mut Project, edit: &Value) -> PatchResult {
    let idx = i(edit, "idx").ok_or((400, "missing title idx".into()))?;
    let idx = usize::try_from(idx).map_err(|_| (404, "bad title idx".to_string()))?;
    let t = p
        .captions
        .titles
        .get_mut(idx)
        .ok_or((404, format!("bad title idx: {idx} out of range")))?;
    if let Some(x) = edit.get("text").and_then(|v| v.as_str()) { t.text = x.to_string(); }
    if let Some(x) = edit.get("tgt").and_then(|v| v.as_str()) { t.tgt = x.to_string(); }
    if let Some(x) = b(edit, "italic") { t.italic = x; }
    if let Some(x) = b(edit, "bold") { t.bold = x; }
    if let Some(x) = b(edit, "uppercase") { t.uppercase = x; }
    if let Some(x) = b(edit, "solid") { t.solid = x; }
    if let Some(v) = edit.get("font") { t.font = v.as_str().map(|x| x.to_string()); }
    if let Some(v) = edit.get("color") { t.color = v.as_str().map(|x| x.to_string()); }
    if let Some(v) = edit.get("bg") { t.bg = v.as_str().map(|x| x.to_string()); }
    if let Some(v) = edit.get("outline") { t.outline = v.as_str().map(|x| x.to_string()); }
    if let Some(v) = edit.get("shadow_dir") { t.shadow_dir = v.as_i64(); } // null->None
    if let Some(a) = edit.get("align").and_then(|v| v.as_str()) { t.align = a.to_string(); }
    if let Some(st) = f(edit, "start") { t.start = st; }
    if let Some(en) = f(edit, "end") { t.end = en; }
    // nullable как shadow_dir: явный null снимает значение (возврат к авто-межстрочному/авто-фиту/авто-контуру),
    // число выставляет, отсутствие ключа не трогает. i()/as_i64() на null давал None -> сброс молча игнорировался.
    if let Some(v) = edit.get("lh") { t.lh = v.as_i64(); }
    if let Some(v) = edit.get("size_px") { t.size_px = v.as_i64(); }
    if let Some(v) = edit.get("outline_w") { t.outline_w = v.as_i64(); }
    if let Some(bbox) = edit.get("bbox").and_then(|v| v.as_array()) {
        t.bbox = Some(bbox.iter().filter_map(|x| x.as_i64()).collect());
    }
    Ok(())
}

/// title_del — удалить титр по индексу. Порт api.del_title (IndexError -> 404).
fn op_title_del(p: &mut Project, edit: &Value) -> PatchResult {
    del_one(&mut p.captions.titles, edit, "title")
}

/// title_add — новый кастомный титр в боксе на [t0,t1]. Порт api.add_title. Нет x/y/w/h -> 400.
fn op_title_add(p: &mut Project, edit: &Value) -> PatchResult {
    let bad = |k: &str| (400, format!("bad title_add: missing/invalid field {k:?}"));
    let text = s(edit, "text").unwrap_or_default();
    let x = i(edit, "x").ok_or_else(|| bad("x"))?;
    let y = i(edit, "y").ok_or_else(|| bad("y"))?;
    let w = i(edit, "w").ok_or_else(|| bad("w"))?;
    let h = i(edit, "h").ok_or_else(|| bad("h"))?;
    let t0 = f(edit, "t0").unwrap_or(0.0);
    let t1 = f(edit, "t1").unwrap_or(p.meta.duration);
    p.captions.titles.push(Title {
        text: text.clone(),
        tgt: text,
        bbox: Some(vec![x, y, w, h]),
        start: t0,
        end: t1,
        italic: b(edit, "italic").unwrap_or(false),
        font: s(edit, "font"),
        color: Some(s(edit, "color").unwrap_or_else(|| "#FFFFFF".into())),
        ..Default::default()
    });
    Ok(())
}

/// Применить одну PATCH-операцию к Project. op берётся из поля "op". Неизвестный op -> 400.
pub fn apply(p: &mut Project, edit: &Value) -> PatchResult {
    let op = s(edit, "op").unwrap_or_default();
    match op.as_str() {
        "caption" => op_caption(p, edit),
        "segment" => op_segment(p, edit),
        "del_segment" => op_del_segment(p, edit),
        "add_segment" => op_add_segment(p, edit),
        "hide_segment" => op_hide_segment(p, edit),
        "del_segments" => op_del_segments(p, edit),
        "hide_segments" => op_hide_segments(p, edit),
        "del_titles" => op_del_titles(p, edit),
        "del_blurs" => op_del_blurs(p, edit),
        "keep_segment" => op_keep_segment(p, edit),
        "keep_segments" => op_keep_segments(p, edit),
        "blur" => op_blur(p, edit),
        "blur_add" => op_blur_add(p, edit),
        "blur_del" => op_blur_del(p, edit),
        "blur_enable" => op_blur_enable(p, edit),
        "preset" => op_preset(p, edit),
        "title" => op_title(p, edit),
        "title_del" => op_title_del(p, edit),
        "title_add" => op_title_add(p, edit),
        "subpos" => op_subpos(p, edit),
        "mode" => op_mode(p, edit),
        "dub" => op_dub(p, edit),
        "subs_burn" => op_subs_burn(p, edit),
        "subs_content" => op_subs_content(p, edit),
        "translate" => op_translate(p, edit),
        "translate_style" => op_translate_style(p, edit),
        "rewrite" => op_rewrite(p, edit),
        "recast" => op_recast(p, edit),
        "regen" => op_regen(p, edit),
        "regen_all" => op_regen_all(p, edit),
        "gain" => op_gain(p, edit),
        "loudness" => op_loudness(p, edit),
        "voiceover_gain" => op_voiceover_gain(p, edit),
        "sub_blur" => op_sub_blur(p, edit),
        "keep_original" => op_keep_original(p, edit),
        "reorder_segments" => op_reorder_segments(p, edit),
        "split_segment" => op_split_segment(p, edit),
        "merge_segments" => op_merge_segments(p, edit),
        "take_select" => op_take_select(p, edit),
        "take_pin" => op_take_pin(p, edit),
        other => Err((400, format!("unknown op {other:?}"))),
    }
}

/// Самая короткая часть, которая остаётся от фразы при разрезе, в секундах.
const MIN_PART: f64 = 0.1;

/// Делит текст в доле `fraction`: по словам, а для письма без пробелов (китайский, японский) — по символам.
/// Обе части непустые, пока в тексте есть хотя бы два слова или символа.
fn split_text(text: &str, fraction: f64) -> (String, String) {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() >= 2 {
        let cut = ((fraction * words.len() as f64).round() as usize).clamp(1, words.len() - 1);
        return (words[..cut].join(" "), words[cut..].join(" "));
    }
    let chars: Vec<char> = text.trim().chars().collect();
    if chars.len() >= 2 {
        let cut = ((fraction * chars.len() as f64).round() as usize).clamp(1, chars.len() - 1);
        let (head, tail): (String, String) = (chars[..cut].iter().collect(), chars[cut..].iter().collect());
        return (head.trim().to_string(), tail.trim().to_string());
    }
    (text.trim().to_string(), String::new())
}

/// split_segment — разрезать фразу id в момент at (сек) на две, как ножницы монтажа. Пословные тайминги
/// ASR делятся по времени; исходный текст — по ним, когда их столько же, сколько слов текста, иначе в той же
/// доле, что время. Перевод берётся из tgt_text/tgt_text_2, если их прислали, иначе делится в доле исходного
/// текста. Вторая часть получает new_id или свободный id вида `<id>_2`. Обе части dirty. Оверрайд субтитра
/// фразы (captions.overrides) переходит к обеим частям: место и стиль те же, свой текст делится в доле
/// исходного текста.
fn op_split_segment(p: &mut Project, edit: &Value) -> PatchResult {
    let sid = s(edit, "id").ok_or((400, "missing segment id".into()))?;
    let at = f(edit, "at").ok_or((400, "missing split time 'at' (seconds)".into()))?;
    let idx = p.segments.iter().position(|x| x.id == sid).ok_or((404, format!("segment {sid:?} not found")))?;
    let (start, end) = (p.segments[idx].start, p.segments[idx].end);
    if at < start + MIN_PART || at > end - MIN_PART {
        return Err((400, format!("split time {at} is not inside segment {sid:?} ({start:.2}..{end:.2}) by {MIN_PART} s")));
    }
    let new_id = free_id(p, s(edit, "new_id"), |n| format!("{sid}_{}", n + 1))?;
    let first = &p.segments[idx];
    let words = first.extra.get("words").and_then(Value::as_array).cloned();
    let (words_1, words_2): (Option<Vec<Value>>, Option<Vec<Value>>) = match &words {
        Some(all) => {
            let (head, tail): (Vec<Value>, Vec<Value>) = all.iter().cloned().partition(|word| word.get("start").and_then(Value::as_f64).is_some_and(|w| w < at));
            (Some(head), Some(tail))
        }
        None => (None, None),
    };
    let src_words: Vec<&str> = first.src_text.split_whitespace().collect();
    let time_fraction = (at - start) / (end - start);
    let src_fraction = match (&words, &words_1) {
        (Some(all), Some(head)) if !src_words.is_empty() && all.len() == src_words.len() => head.len() as f64 / all.len() as f64,
        _ => time_fraction,
    };
    let divide = |text: &str| match src_fraction {
        share if share <= 0.0 => (String::new(), text.trim().to_string()),
        share if share >= 1.0 => (text.trim().to_string(), String::new()),
        share => split_text(text, share),
    };
    let (src_1, src_2) = divide(&first.src_text);
    let (auto_1, auto_2) = divide(&first.tgt_text);
    let tgt_1 = s(edit, "tgt_text").unwrap_or(auto_1);
    let tgt_2 = s(edit, "tgt_text_2").unwrap_or(auto_2);
    // build_ass рисует override.text вместо tgt_text: целый текст на первой части задвоил бы вторую.
    // Оверрайд удалённой фразы с тем же id (del_segment их не чистит) к новой части не относится.
    p.captions.overrides.retain(|o| o.seg_id != new_id);
    let caption_2 = p.captions.overrides.iter_mut().find(|o| o.seg_id == sid).map(|own| {
        let mut copy = own.clone();
        copy.seg_id = new_id.clone();
        if let Some(text) = own.text.take() {
            let (head, tail) = divide(&text);
            own.text = Some(head);
            copy.text = Some(tail);
        }
        copy
    });

    let mut second = p.segments[idx].clone();
    second.id = new_id;
    second.start = at;
    second.src_text = src_2;
    second.tgt_text = tgt_2;
    second.dirty = true;
    second.ckpt = None;
    let first = &mut p.segments[idx];
    first.end = at;
    first.src_text = src_1;
    first.tgt_text = tgt_1;
    first.dirty = true;
    first.ckpt = None;
    for (segment, part) in [(&mut *first, words_1), (&mut second, words_2)] {
        if let Some(part) = part {
            segment.extra.insert("words".into(), Value::Array(part));
        }
    }
    p.segments.insert(idx + 1, second);
    p.captions.overrides.extend(caption_2);
    Ok(())
}

/// merge_segments — склеить фразы ids в одну. Они должны стоять подряд в списке фраз; остаётся id первой
/// по списку, её спикер и голос; время — от самого раннего начала до самого позднего конца, тексты и
/// пословные тайминги — друг за другом. Результат dirty. Оверрайды субтитров частей сводятся в один на id
/// склеенной фразы: место и стиль — первого по порядку, а если хоть у одной части свой текст, текст
/// субтитра — тексты частей подряд (свой текст части или её перевод).
fn op_merge_segments(p: &mut Project, edit: &Value) -> PatchResult {
    let wanted = ids(edit);
    if wanted.len() < 2 {
        return Err((400, "merge_segments needs at least two segment ids".into()));
    }
    let mut places: Vec<usize> = Vec::with_capacity(wanted.len());
    for id in &wanted {
        let at = p.segments.iter().position(|x| &x.id == id).ok_or((404, format!("segment {id:?} not found")))?;
        if !places.contains(&at) {
            places.push(at);
        }
    }
    places.sort_unstable();
    if places.len() < 2 {
        return Err((400, "merge_segments needs at least two different segments".into()));
    }
    if places.windows(2).any(|pair| pair[1] != pair[0] + 1) {
        return Err((400, "segments to merge must follow one another in the list".into()));
    }
    let parts: Vec<dub_core::Segment> = p.segments.drain(places[0]..=places[places.len() - 1]).collect();
    let join = |text: fn(&dub_core::Segment) -> &str| parts.iter().map(text).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    let mut merged = parts[0].clone();
    merged.start = parts.iter().map(|x| x.start).fold(f64::INFINITY, f64::min);
    merged.end = parts.iter().map(|x| x.end).fold(f64::NEG_INFINITY, f64::max);
    merged.src_text = join(|x| x.src_text.as_str());
    merged.tgt_text = join(|x| x.tgt_text.as_str());
    merged.dirty = true;
    merged.ckpt = None;
    let words: Vec<Value> = parts.iter().filter_map(|x| x.extra.get("words").and_then(Value::as_array)).flatten().cloned().collect();
    if words.is_empty() {
        merged.extra.remove("words");
    } else {
        merged.extra.insert("words".into(), Value::Array(words));
    }
    let captions: Vec<Option<CaptionOverride>> = parts.iter().map(|part| p.captions.overrides.iter().find(|o| o.seg_id == part.id).cloned()).collect();
    if let Some(first) = captions.iter().flatten().next() {
        let mut kept = first.clone();
        kept.seg_id = merged.id.clone();
        if captions.iter().flatten().any(|o| o.text.is_some()) {
            let texts = parts.iter().zip(&captions).map(|(part, own)| own.as_ref().and_then(|o| o.text.as_deref()).unwrap_or(&part.tgt_text));
            kept.text = Some(texts.map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" "));
        }
        p.captions.overrides.retain(|o| !parts.iter().any(|part| part.id == o.seg_id));
        p.captions.overrides.push(kept);
    }
    p.segments.insert(places[0], merged);
    Ok(())
}

/// take_select — сделать дубль из истории фразы активным (takes.rs кладёт его файл в сегмент). Правка
/// приходит дополненной обработчиком PATCH: take_text/take_nonce/take_key выбранного дубля. Другой текст
/// дубля возвращает и текст реплики; нонс и ключ — те, с которыми дубль озвучен, чтобы рендер взял его
/// без нового синтеза.
fn op_take_select(p: &mut Project, edit: &Value) -> PatchResult {
    let text = s(edit, "take_text").ok_or((400, "take_select is resolved by PATCH /projects/{pid}: no take_text".to_string()))?;
    let key = s(edit, "take_key").ok_or((400, "take_select is resolved by PATCH /projects/{pid}: no take_key".to_string()))?;
    let nonce = edit.get("take_nonce").cloned().unwrap_or(Value::Null);
    let seg = seg_by_id(p, edit)?;
    if seg.tgt_text.trim() != text {
        seg.tgt_text = text;
    }
    if nonce.is_null() {
        seg.extra.remove(crate::render::REGEN_NONCE);
    } else {
        seg.extra.insert(crate::render::REGEN_NONCE.into(), nonce);
    }
    seg.ckpt = Some(key);
    seg.dirty = true;
    Ok(())
}

/// take_pin — закрепить активный дубль фразы или снять закрепление (история на диске, takes.rs).
fn op_take_pin(p: &mut Project, edit: &Value) -> PatchResult {
    b(edit, "pinned").ok_or((400, "take_pin needs pinned (true or false)".to_string()))?;
    seg_by_id(p, edit)?;
    Ok(())
}

/// reorder_segments — изменить порядок сегментов согласно списку id в edit["ids"].
fn op_reorder_segments(p: &mut Project, edit: &Value) -> PatchResult {
    let new_ids = ids(edit);
    if new_ids.is_empty() {
        return Err((400, "reorder_segments requires non-empty ids array".into()));
    }
    let mut map: std::collections::HashMap<String, dub_core::Segment> =
        p.segments.drain(..).map(|s| (s.id.clone(), s)).collect();
    for id in &new_ids {
        if let Some(s) = map.remove(id) {
            p.segments.push(s);
        }
    }
    for (_, s) in map {
        p.segments.push(s);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proj_with_seg() -> Project {
        let mut p = Project::default();
        p.segments.push(dub_core::Segment {
            id: "s0".into(),
            start: 0.0,
            end: 1.0,
            src_text: "hi".into(),
            ..Default::default()
        });
        p
    }

    #[test]
    fn segment_edits_text_and_marks_dirty() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"segment","id":"s0","tgt_text":"привет"})).unwrap();
        assert_eq!(p.segments[0].tgt_text, "привет");
        assert!(p.segments[0].dirty);
    }

    #[test]
    fn segment_unknown_id_404() {
        let mut p = proj_with_seg();
        let e = apply(&mut p, &json!({"op":"segment","id":"sX","tgt_text":"x"})).unwrap_err();
        assert_eq!(e.0, 404);
    }

    #[test]
    fn subpos_sets_locked() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"subpos","sub_y":720})).unwrap();
        assert_eq!(p.captions.sub_y, Some(720));
        assert!(p.captions.sub_y_locked);
    }

    #[test]
    fn mode_dub_and_unknown() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"mode","value":"dub"})).unwrap();
        assert_eq!(p.mode, "dub");
        assert!(p.segments[0].dirty);
        let e = apply(&mut p, &json!({"op":"mode","value":"nope"})).unwrap_err();
        assert_eq!(e.0, 400);
    }

    #[test]
    fn a_dub_mode_keeps_the_subtitle_language_chosen_for_the_dub() {
        let mut p = proj_with_seg();
        p.mode = "dub".into();
        p.subs.mode = "bilingual".into();
        apply(&mut p, &json!({"op":"mode","value":"voiceover"})).unwrap();
        assert_eq!((p.mode.as_str(), p.subs.mode.as_str()), ("voiceover", "bilingual"));

        p.mode = "dub".into();
        p.subs.mode = "transcribe".into();
        apply(&mut p, &json!({"op":"mode","value":"funny"})).unwrap();
        assert_eq!((p.mode.as_str(), p.subs.mode.as_str()), ("dub", "transcribe"));
        apply(&mut p, &json!({"op":"mode","value":"voiceover"})).unwrap();
        assert_eq!(p.subs.mode, "transcribe");

        p.mode = "nodub".into();
        p.subs.mode = "bilingual".into();
        apply(&mut p, &json!({"op":"mode","value":"dub"})).unwrap();
        assert_eq!(p.subs.mode, "bilingual");

        p.subs.mode = "none".into();
        apply(&mut p, &json!({"op":"mode","value":"funny"})).unwrap();
        assert_eq!(p.subs.mode, "none");
    }

    #[test]
    fn a_dub_mode_after_the_subtitles_preset_shows_the_translation() {
        let mut p = proj_with_seg();
        p.subs.mode = "translate".into();
        apply(&mut p, &json!({"op":"mode","value":"subtitles"})).unwrap();
        assert_eq!((p.mode.as_str(), p.subs.mode.as_str()), ("nodub", "transcribe"));
        apply(&mut p, &json!({"op":"mode","value":"dub"})).unwrap();
        assert_eq!((p.mode.as_str(), p.subs.mode.as_str()), ("dub", "translate"));
        apply(&mut p, &json!({"op":"mode","value":"transcribe"})).unwrap();
        apply(&mut p, &json!({"op":"mode","value":"voiceover"})).unwrap();
        assert_eq!((p.mode.as_str(), p.subs.mode.as_str()), ("voiceover", "translate"));

        p.subs.mode = "bilingual".into();
        apply(&mut p, &json!({"op":"mode","value":"subtitles"})).unwrap();
        assert_eq!(p.subs.mode, "transcribe", "пресет «Субтитры» — субтитры оригинала");
    }

    #[test]
    fn translate_sets_lang_and_dirty() {
        let mut p = proj_with_seg();
        p.mode = "nodub".into();
        p.subs.mode = "transcribe".into();
        apply(&mut p, &json!({"op":"translate","lang":"de"})).unwrap();
        assert_eq!(p.tgt_lang, "de");
        assert_eq!(p.subs.mode, "translate");
        assert!(p.audio.rewrite.is_none());
        assert!(p.segments[0].dirty);
    }

    #[test]
    fn translate_funny_sets_rewrite() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"translate","lang":"en","mode":"funny"})).unwrap();
        assert_eq!(p.audio.rewrite.as_deref(), Some("make it a funny, playful dub"));
    }

    #[test]
    fn translate_style_normalizes_and_marks_dirty() {
        let mut p = proj_with_seg();
        // переводы строк -> пробелы, обрезка краёв.
        apply(&mut p, &json!({"op":"translate_style","style":"  formal,\n very polite  "})).unwrap();
        assert_eq!(p.audio.translate_style, "formal, very polite");
        assert!(p.segments[0].dirty);
        // кап длины ~500 символов.
        let long = "a".repeat(700);
        apply(&mut p, &json!({"op":"translate_style","style":long})).unwrap();
        assert_eq!(p.audio.translate_style.chars().count(), 500);
        // пустой style снимает стиль.
        apply(&mut p, &json!({"op":"translate_style","style":"  "})).unwrap();
        assert_eq!(p.audio.translate_style, "");
    }

    #[test]
    fn rewrite_sets_instruction_and_dub() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"rewrite","instruction":"as a pirate"})).unwrap();
        assert_eq!(p.audio.rewrite.as_deref(), Some("as a pirate"));
        assert_eq!(p.mode, "dub");
        assert!(p.segments[0].dirty);
        let e = apply(&mut p, &json!({"op":"rewrite","instruction":"  "})).unwrap_err();
        assert_eq!(e.0, 400);
    }

    // ── PATCH-хвост (раунд 5) ────────────────────────────────────────────────

    #[test]
    fn caption_global_sets_substyle_and_plate_toggle() {
        let mut p = proj_with_seg();
        // тумблер подложки: plate=false -> в extra sub_style (map_sub_style читает).
        apply(&mut p, &json!({"op":"caption","color":"#FF0000","plate":false})).unwrap();
        let ss = p.captions.sub_style.as_ref().unwrap();
        assert_eq!(ss.color, "#FF0000");
        assert_eq!(ss.extra.get("plate").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn caption_per_segment_override() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"caption","seg_id":"s0","text":"свой текст","color":"#00FF00"})).unwrap();
        assert_eq!(p.captions.overrides.len(), 1);
        let o = &p.captions.overrides[0];
        assert_eq!(o.seg_id, "s0");
        assert_eq!(o.text.as_deref(), Some("свой текст"));
        assert_eq!(o.style.as_ref().unwrap().color, "#00FF00");
        // повторный caption на тот же seg_id ОБНОВЛЯЕТ, не добавляет.
        apply(&mut p, &json!({"op":"caption","seg_id":"s0","text":"новый"})).unwrap();
        assert_eq!(p.captions.overrides.len(), 1);
        assert_eq!(p.captions.overrides[0].text.as_deref(), Some("новый"));
    }

    #[test]
    fn del_segment_marks_first_dirty_and_404() {
        let mut p = proj_with_seg();
        p.segments.push(dub_core::Segment { id: "s1".into(), ..Default::default() });
        apply(&mut p, &json!({"op":"del_segment","id":"s1"})).unwrap();
        assert_eq!(p.segments.len(), 1);
        assert!(p.segments[0].dirty);
        let e = apply(&mut p, &json!({"op":"del_segment","id":"nope"})).unwrap_err();
        assert_eq!(e.0, 404);
    }

    #[test]
    fn blur_add_edit_del_cycle() {
        let mut p = proj_with_seg();
        p.meta.duration = 12.0;
        apply(&mut p, &json!({"op":"blur_add","x":10,"y":20,"w":100,"h":40})).unwrap();
        assert_eq!(p.captions.blur_boxes.len(), 1);
        assert_eq!(p.captions.blur_boxes[0].t1, 12.0); // дефолт весь ролик
        apply(&mut p, &json!({"op":"blur","idx":0,"x":15,"hidden":true})).unwrap();
        assert_eq!(p.captions.blur_boxes[0].x, 15);
        assert!(p.captions.blur_boxes[0].hidden);
        // out-of-range -> 404
        let e = apply(&mut p, &json!({"op":"blur","idx":9,"x":1})).unwrap_err();
        assert_eq!(e.0, 404);
        apply(&mut p, &json!({"op":"blur_del","idx":0})).unwrap();
        assert!(p.captions.blur_boxes.is_empty());
        // отсутствие обязательного поля -> 400
        let e = apply(&mut p, &json!({"op":"blur_add","x":1,"y":2})).unwrap_err();
        assert_eq!(e.0, 400);
    }

    #[test]
    fn title_add_edit_del_cycle() {
        let mut p = proj_with_seg();
        p.meta.duration = 8.0;
        apply(&mut p, &json!({"op":"title_add","text":"HELLO","x":50,"y":60,"w":300,"h":80})).unwrap();
        assert_eq!(p.captions.titles.len(), 1);
        let t = &p.captions.titles[0];
        assert_eq!(t.text, "HELLO");
        assert_eq!(t.tgt, "HELLO");
        assert_eq!(t.bbox.as_deref(), Some(&[50i64, 60, 300, 80][..]));
        assert_eq!(t.end, 8.0);
        apply(&mut p, &json!({"op":"title","idx":0,"tgt":"ПРИВЕТ","color":"#FF0000"})).unwrap();
        assert_eq!(p.captions.titles[0].tgt, "ПРИВЕТ");
        assert_eq!(p.captions.titles[0].color.as_deref(), Some("#FF0000"));
        apply(&mut p, &json!({"op":"title_del","idx":0})).unwrap();
        assert!(p.captions.titles.is_empty());
        let e = apply(&mut p, &json!({"op":"title_del","idx":0})).unwrap_err();
        assert_eq!(e.0, 404);
    }

    #[test]
    fn title_size_px_null_resets_to_auto() {
        // регресс: очистка поля px/контура/межстрочья у титра (явный JSON null) должна вернуть авто-подбор,
        // а не молча игнорироваться (i()/as_i64() на null давал None -> сброс не применялся).
        let mut p = Project::default();
        apply(&mut p, &json!({"op":"title_add","text":"T","x":0,"y":0,"w":100,"h":40})).unwrap();
        apply(&mut p, &json!({"op":"title","idx":0,"size_px":80,"outline_w":6,"lh":50})).unwrap();
        assert_eq!(p.captions.titles[0].size_px, Some(80));
        assert_eq!(p.captions.titles[0].outline_w, Some(6));
        assert_eq!(p.captions.titles[0].lh, Some(50));
        apply(&mut p, &json!({"op":"title","idx":0,"size_px":null,"outline_w":null,"lh":null})).unwrap();
        assert_eq!(p.captions.titles[0].size_px, None);   // null -> авто-фит
        assert_eq!(p.captions.titles[0].outline_w, None);
        assert_eq!(p.captions.titles[0].lh, None);
        apply(&mut p, &json!({"op":"title","idx":0,"size_px":42})).unwrap();
        apply(&mut p, &json!({"op":"title","idx":0,"color":"#fff"})).unwrap();   // отсутствие ключа не трогает size_px
        assert_eq!(p.captions.titles[0].size_px, Some(42));
    }

    #[test]
    fn preset_and_blur_enable() {
        let mut p = proj_with_seg();
        apply(&mut p, &json!({"op":"preset","name":"hormozi"})).unwrap();
        assert_eq!(p.captions.preset.name.as_deref(), Some("hormozi"));
        apply(&mut p, &json!({"op":"preset","name":"match"})).unwrap();
        // "match"/пусто хранится как есть в питоне (None only когда name отсутствует/пусто); тут name="match".
        apply(&mut p, &json!({"op":"preset"})).unwrap();
        assert!(p.captions.preset.name.is_none());
        apply(&mut p, &json!({"op":"blur_enable","on":false})).unwrap();
        assert!(!p.render.blur);
    }

    #[test]
    fn subs_content_sets_bilingual_and_its_second_line() {
        let mut p = Project::default();
        apply(&mut p, &json!({"op":"subs_content","value":"bilingual"})).unwrap();
        assert_eq!(p.subs.mode, "bilingual");
        assert_eq!(p.subs.bilingual, dub_core::Bilingual::default());
        apply(&mut p, &json!({"op":"subs_content","order":"original_top","secondary":{"size_pct":60,"color":"#ffd400","opacity":80}})).unwrap();
        assert_eq!(p.subs.mode, "bilingual", "без value режим не меняется");
        assert_eq!(p.subs.bilingual.order, "original_top");
        assert_eq!(p.subs.bilingual.secondary.size_pct, 60);
        assert_eq!(p.subs.bilingual.secondary.color.as_deref(), Some("#FFD400"));
        assert_eq!(p.subs.bilingual.secondary.opacity, Some(80));
        apply(&mut p, &json!({"op":"subs_content","secondary":{"color":null,"opacity":null}})).unwrap();
        assert_eq!((p.subs.bilingual.secondary.color.clone(), p.subs.bilingual.secondary.opacity), (None, None));
        assert_eq!(p.subs.bilingual.secondary.size_pct, 60, "поле, которого нет, не меняется");
        apply(&mut p, &json!({"op":"subs_content","value":"transcribe"})).unwrap();
        assert_eq!(p.subs.mode, "transcribe");
        assert_eq!(p.subs.bilingual.order, "original_top", "настройки двуязычных сохраняются");
    }

    #[test]
    fn a_new_target_language_keeps_bilingual_subtitles() {
        let mut p = Project::default();
        p.subs.mode = "bilingual".into();
        apply(&mut p, &json!({"op":"translate","lang":"de"})).unwrap();
        assert_eq!((p.tgt_lang.as_str(), p.subs.mode.as_str()), ("de", "bilingual"));
        p.mode = "nodub".into();
        p.subs.mode = "transcribe".into();
        apply(&mut p, &json!({"op":"translate","lang":"fr"})).unwrap();
        assert_eq!(p.subs.mode, "translate", "the transcript of a subtitles project becomes the translation");
        p.mode = "dub".into();
        p.subs.mode = "transcribe".into();
        apply(&mut p, &json!({"op":"translate","lang":"es"})).unwrap();
        assert_eq!(p.subs.mode, "transcribe", "the original's language chosen under a dub stays");
        p.subs.mode = "none".into();
        apply(&mut p, &json!({"op":"translate","lang":"it"})).unwrap();
        assert_eq!(p.subs.mode, "none", "subtitles switched off stay off");
    }

    #[test]
    fn subs_content_refuses_bad_values_without_changing_anything() {
        let mut p = Project::default();
        for bad in [
            json!({"op":"subs_content","value":"both"}),
            json!({"op":"subs_content","value":"bilingual","order":"left"}),
            json!({"op":"subs_content","value":"bilingual","secondary":{"size_pct":10}}),
            json!({"op":"subs_content","value":"bilingual","secondary":{"color":"yellow"}}),
            json!({"op":"subs_content","value":"bilingual","secondary":{"opacity":0}}),
            json!({"op":"subs_content","value":"bilingual","secondary":{"font":"Arial"}}),
            json!({"op":"subs_content"}),
        ] {
            let e = apply(&mut p, &bad).unwrap_err();
            assert_eq!(e.0, 400, "{bad}");
        }
        assert_eq!(p.subs.mode, "none");
        assert_eq!(p.subs.bilingual, dub_core::Bilingual::default());
    }

    #[test]
    fn keep_original_toggles_and_validates_container() {
        let mut p = proj_with_seg();
        // дефолты: выключено, mp4.
        assert!(!p.audio.keep_original_track);
        assert_eq!(p.audio.container, "mp4");
        // keep + mkv.
        apply(&mut p, &json!({"op":"keep_original","keep":true,"container":"mkv"})).unwrap();
        assert!(p.audio.keep_original_track);
        assert_eq!(p.audio.container, "mkv");
        // ре-TTS не требуется — dirty НЕ ставится.
        assert!(!p.segments[0].dirty);
        // keep без container — контейнер не трогается.
        apply(&mut p, &json!({"op":"keep_original","keep":false})).unwrap();
        assert!(!p.audio.keep_original_track);
        assert_eq!(p.audio.container, "mkv");
        // невалидный container -> 400.
        let e = apply(&mut p, &json!({"op":"keep_original","keep":true,"container":"avi"})).unwrap_err();
        assert_eq!(e.0, 400);
    }

    #[test]
    fn take_select_restores_the_take_text_nonce_and_key() {
        let mut p = proj_with_seg();
        p.segments[0].tgt_text = "Новый".into();
        p.segments[0].extra.insert(crate::render::REGEN_NONCE.into(), json!("n2"));
        let e = apply(&mut p, &json!({"op":"take_select","id":"s0","take":0})).unwrap_err();
        assert_eq!(e.0, 400, "an unresolved take_select is refused");
        apply(&mut p, &json!({"op":"take_select","id":"s0","take":0,"take_text":"Старый","take_nonce":null,"take_key":"k0"})).unwrap();
        let s = &p.segments[0];
        assert_eq!(s.tgt_text, "Старый");
        assert!(s.extra.get(crate::render::REGEN_NONCE).is_none());
        assert_eq!(s.ckpt.as_deref(), Some("k0"));
        assert!(s.dirty);
        assert_eq!(apply(&mut p, &json!({"op":"take_pin","id":"s0"})).unwrap_err().0, 400);
        assert_eq!(apply(&mut p, &json!({"op":"take_pin","id":"nope","pinned":true})).unwrap_err().0, 404);
        apply(&mut p, &json!({"op":"take_pin","id":"s0","pinned":true})).unwrap();
    }

    #[test]
    fn del_titles_and_del_blurs_high_to_low() {
        let mut p = proj_with_seg();
        for _ in 0..3 {
            p.captions.titles.push(dub_core::Title { text: "t".into(), ..Default::default() });
            p.captions.blur_boxes.push(dub_core::BlurBox::default());
        }
        apply(&mut p, &json!({"op":"del_titles","idxs":[0,2]})).unwrap();
        assert_eq!(p.captions.titles.len(), 1);
        apply(&mut p, &json!({"op":"del_blurs","idxs":[1]})).unwrap();
        assert_eq!(p.captions.blur_boxes.len(), 2);
    }

    fn line(id: &str, start: f64, end: f64, src: &str, tgt: &str) -> dub_core::Segment {
        dub_core::Segment { id: id.into(), start, end, src_text: src.into(), tgt_text: tgt.into(), speaker: Some("1".into()), ..Default::default() }
    }

    #[test]
    fn split_cuts_a_line_at_a_moment_by_its_words() {
        let mut p = Project::default();
        let mut s1 = line("s1", 1.0, 5.0, "one two three four", "раз два три четыре");
        s1.ckpt = Some("k".into());
        s1.extra.insert("words".into(), json!([
            { "word": "one", "start": 1.0, "end": 1.5 }, { "word": "two", "start": 1.6, "end": 2.0 },
            { "word": "three", "start": 3.1, "end": 3.6 }, { "word": "four", "start": 4.0, "end": 4.8 },
        ]));
        p.segments.push(s1);
        p.segments.push(line("s2", 6.0, 7.0, "five", "пять"));
        apply(&mut p, &json!({"op":"split_segment","id":"s1","at":3.0})).unwrap();
        let ids: Vec<&str> = p.segments.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["s1", "s1_2", "s2"]);
        let (a, b) = (&p.segments[0], &p.segments[1]);
        assert_eq!((a.start, a.end, b.start, b.end), (1.0, 3.0, 3.0, 5.0));
        assert_eq!((a.src_text.as_str(), b.src_text.as_str()), ("one two", "three four"));
        assert_eq!((a.tgt_text.as_str(), b.tgt_text.as_str()), ("раз два", "три четыре"));
        assert_eq!(a.extra["words"].as_array().unwrap().len(), 2);
        assert_eq!(b.extra["words"][0]["word"], "three");
        assert!(a.dirty && b.dirty && a.ckpt.is_none() && b.ckpt.is_none());
        assert_eq!(b.speaker.as_deref(), Some("1"));
    }

    #[test]
    fn split_takes_the_translation_it_is_given_and_refuses_a_moment_outside() {
        let mut p = Project::default();
        p.segments.push(line("s1", 0.0, 4.0, "a b c d", "один два три четыре"));
        apply(&mut p, &json!({"op":"split_segment","id":"s1","at":1.0,"new_id":"x","tgt_text":"первая","tgt_text_2":"вторая"})).unwrap();
        assert_eq!(p.segments[1].id, "x");
        assert_eq!((p.segments[0].src_text.as_str(), p.segments[1].src_text.as_str()), ("a", "b c d"));
        assert_eq!((p.segments[0].tgt_text.as_str(), p.segments[1].tgt_text.as_str()), ("первая", "вторая"));
        assert_eq!(apply(&mut p, &json!({"op":"split_segment","id":"x","at":1.05})).unwrap_err().0, 400, "too close to the start");
        assert_eq!(apply(&mut p, &json!({"op":"split_segment","id":"x","at":9.0})).unwrap_err().0, 400, "after the end");
        assert_eq!(apply(&mut p, &json!({"op":"split_segment","id":"s1","at":0.5,"new_id":"x"})).unwrap_err().0, 409, "an id in use");
        assert_eq!(apply(&mut p, &json!({"op":"split_segment","id":"nope","at":0.5})).unwrap_err().0, 404);
        assert_eq!(split_text("你好世界", 0.5), ("你好".to_string(), "世界".to_string()), "text without spaces is cut by characters");
    }

    #[test]
    fn a_new_line_never_shares_another_line_s_files() {
        let mut p = Project::default();
        p.segments.extend([line("s1", 0.0, 4.0, "a b", "раз два"), line("s12", 5.0, 6.0, "c", "три"), line("s1_2", 7.0, 8.0, "d", "четыре")]);
        apply(&mut p, &json!({"op":"split_segment","id":"s1","at":2.0})).unwrap();
        assert_eq!(p.segments[1].id, "s1_3", "s1_2 is taken");
        assert_eq!(apply(&mut p, &json!({"op":"split_segment","id":"s1","at":1.0,"new_id":"s1.2"})).unwrap_err().0, 409, "s1.2 is the file of s12");
        assert_eq!(apply(&mut p, &json!({"op":"split_segment","id":"s1","at":1.0,"new_id":"--"})).unwrap_err().0, 400, "no file name");
        assert_eq!(apply(&mut p, &json!({"op":"add_segment","start":9.0,"id":"s-12"})).unwrap_err().0, 409);
        let files: std::collections::HashSet<_> = p.segments.iter().map(|x| crate::render::seg_file_id(&x.id)).collect();
        assert_eq!(files.len(), p.segments.len());
    }

    #[test]
    fn merge_joins_neighbouring_lines() {
        let mut p = Project::default();
        let mut s1 = line("s1", 1.0, 2.0, "one", "раз");
        s1.extra.insert("words".into(), json!([{ "word": "one", "start": 1.0, "end": 1.4 }]));
        let mut s2 = line("s2", 2.2, 3.0, "two", "два");
        s2.speaker = Some("2".into());
        s2.extra.insert("words".into(), json!([{ "word": "two", "start": 2.2, "end": 2.8 }]));
        p.segments.extend([s1, s2, line("s3", 4.0, 5.0, "three", "три")]);
        assert_eq!(apply(&mut p, &json!({"op":"merge_segments","ids":["s1","s3"]})).unwrap_err().0, 400, "not neighbours");
        assert_eq!(apply(&mut p, &json!({"op":"merge_segments","ids":["s1"]})).unwrap_err().0, 400, "one line");
        assert_eq!(apply(&mut p, &json!({"op":"merge_segments","ids":["s1","zz"]})).unwrap_err().0, 404);
        apply(&mut p, &json!({"op":"merge_segments","ids":["s2","s1"]})).unwrap();
        assert_eq!(p.segments.len(), 2);
        let merged = &p.segments[0];
        assert_eq!((merged.id.as_str(), merged.start, merged.end), ("s1", 1.0, 3.0));
        assert_eq!((merged.src_text.as_str(), merged.tgt_text.as_str()), ("one two", "раз два"));
        assert_eq!(merged.speaker.as_deref(), Some("1"));
        assert_eq!(merged.extra["words"].as_array().unwrap().len(), 2);
        assert!(merged.dirty);
    }

    fn caption_of<'a>(p: &'a Project, id: &str) -> Vec<&'a CaptionOverride> {
        p.captions.overrides.iter().filter(|o| o.seg_id == id).collect()
    }

    #[test]
    fn split_divides_a_line_s_own_subtitle_and_keeps_its_place_and_style() {
        let mut p = Project::default();
        p.segments.push(line("s1", 0.0, 4.0, "a b c d", "один два три четыре"));
        apply(&mut p, &json!({"op":"caption","seg_id":"s1","text":"свой текст этой фразы","y":120,"color":"#00FF00"})).unwrap();
        p.captions.overrides.push(CaptionOverride { seg_id: "s1_2".into(), text: Some("от удалённой фразы".into()), ..Default::default() });
        apply(&mut p, &json!({"op":"split_segment","id":"s1","at":2.0})).unwrap();
        let (first, second) = (caption_of(&p, "s1"), caption_of(&p, "s1_2"));
        assert_eq!((first.len(), second.len()), (1, 1), "one override each, the stale one of a deleted line gone");
        assert_eq!((first[0].text.as_deref(), second[0].text.as_deref()), (Some("свой текст"), Some("этой фразы")));
        assert_eq!((second[0].y, second[0].style.as_ref().map(|st| st.color.as_str())), (Some(120), Some("#00FF00")));

        apply(&mut p, &json!({"op":"caption","seg_id":"s1","y":40})).unwrap();
        apply(&mut p, &json!({"op":"split_segment","id":"s1","at":1.0})).unwrap();
        let third = caption_of(&p, "s1_3");
        assert_eq!((third[0].y, third[0].text.as_deref(), caption_of(&p, "s1")[0].text.as_deref()), (Some(40), Some("текст"), Some("свой")));

        let mut styled = Project::default();
        styled.segments.push(line("s1", 0.0, 4.0, "a b", "раз два"));
        apply(&mut styled, &json!({"op":"caption","seg_id":"s1","x":30})).unwrap();
        apply(&mut styled, &json!({"op":"split_segment","id":"s1","at":2.0})).unwrap();
        let copy = caption_of(&styled, "s1_2");
        assert_eq!((copy[0].x, copy[0].text.as_deref()), (Some(30), None), "a style-only override is copied without a text");

        let mut plain = Project::default();
        plain.segments.push(line("s1", 0.0, 4.0, "a b", "раз два"));
        apply(&mut plain, &json!({"op":"split_segment","id":"s1","at":2.0})).unwrap();
        assert!(plain.captions.overrides.is_empty(), "a line without an override gets none");
    }

    #[test]
    fn merge_joins_the_parts_own_subtitles_under_the_first_id() {
        let mut p = Project::default();
        p.segments.extend([line("s1", 0.0, 1.0, "one", "раз"), line("s2", 1.0, 2.0, "two", "два"), line("s3", 2.0, 3.0, "three", "три")]);
        apply(&mut p, &json!({"op":"caption","seg_id":"s2","text":"ДВА","x":10})).unwrap();
        apply(&mut p, &json!({"op":"caption","seg_id":"s3","color":"#FF0000"})).unwrap();
        apply(&mut p, &json!({"op":"merge_segments","ids":["s1","s2","s3"]})).unwrap();
        assert_eq!(p.captions.overrides.len(), 1, "the absorbed lines leave no overrides behind");
        let kept = &p.captions.overrides[0];
        assert_eq!((kept.seg_id.as_str(), kept.text.as_deref(), kept.x), ("s1", Some("раз ДВА три"), Some(10)));

        let mut styled = Project::default();
        styled.segments.extend([line("s1", 0.0, 1.0, "one", "раз"), line("s2", 1.0, 2.0, "two", "два")]);
        apply(&mut styled, &json!({"op":"caption","seg_id":"s1","y":50})).unwrap();
        apply(&mut styled, &json!({"op":"caption","seg_id":"s2","y":90})).unwrap();
        apply(&mut styled, &json!({"op":"merge_segments","ids":["s1","s2"]})).unwrap();
        assert_eq!(styled.captions.overrides.len(), 1);
        assert_eq!((styled.captions.overrides[0].y, styled.captions.overrides[0].text.as_deref()), (Some(50), None), "without own texts the subtitle keeps the joined translation");
    }
}
