//! Стадия перевода + vision для analyze (раунд 3). Порт pipeline._build_dub translate-ветки поверх
//! крейта dub-translate (Gemma через сайдкар llama-server). Заполняет tgt_text сегментов, titles/brands/
//! sub_style/sub_y и raw_ctx в Project. Все решения (do_translate / same_lang / rewrite) — как в питоне.
//!
//! Перевод нужен, но не выполнен (нет llama-бинаря / нет весов / упал сервер / большая часть строк осталась
//! на исходном языке) — стадия возвращает Err с причиной, и analyze падает с ней.

use dub_core::{Brand, GlossaryEntry, Project, SubStyle};
use dub_llm::ChatClient;
use dub_translate::{classify_content_type, ctx_run, looks_untranslated, CtxConfig, FlatOpts, Seg};
use serde_json::Value;

use crate::analyze::{AnalyzeArgs, AnalyzePaths, Progress};

fn emit(progress: &Progress, stage: &str, msg: &str) {
    progress(serde_json::json!({ "stage": stage, "msg": msg }));
}

/// tgt = исходный текст (без MT) — для transcribe- и same-lang-веток.
fn copy_src_to_tgt(proj: &mut Project) {
    for s in &mut proj.segments {
        s.tgt_text = s.src_text.clone();
    }
}

/// Нужно ли переводить: dub/voiceover-режим ИЛИ субтитры с переводом (translate, bilingual) — как
/// do_translate в pipeline.
pub(crate) fn wants_translate(proj: &Project) -> bool {
    proj.mode == "dub" || proj.mode == "voiceover" || matches!(proj.subs.mode.as_str(), "translate" | "bilingual")
}

/// Автономная классификация типа контента (real/anime) для кастинга. Нужна, когда translate-стадия
/// пропущена ранним return (same-lang / transcribe / нет LLM), а content_type="auto": иначе casting
/// молча берёт "real"-детектор для анимации. Открывает vision-провайдер (своя Gemma+mmproj, локальный
/// сервер или OpenRouter) ТОЛЬКО ради классификации и закрывает.
/// None -> классифицировать не удалось (причина в прогрессе) -> вызывающий оставит дефолт.
pub fn classify_content_type_standalone(
    paths: &AnalyzePaths,
    total: f64,
    progress: &Progress,
) -> Option<String> {
    // Без vision-модели классификация невозможна: слать кадры в text-only модель = молча "real"
    // с ложным «0 голосов». Нет vision -> None, вызывающий честно оставит дефолт.
    let provider = match crate::llm_provider::open(
        &crate::llm_provider::LlmOpen {
            llama_bin: &paths.llama_bin,
            mt_model: &paths.mt_model,
            mmproj: &paths.mmproj,
            models_root: &paths.models_root,
        },
        crate::llm_provider::LlmMode::Vision,
    ) {
        Ok(provider) => provider,
        Err(e) => {
            emit(progress, "vision", &t!("translate-no-vision", error = e));
            return None;
        }
    };
    let tmp = paths.work_dir.join("ctype_frame.png");
    let ct = classify_content_type(provider.client(), &paths.input, &tmp, total, |m| emit(progress, "vision", m));
    let _ = std::fs::remove_file(&tmp);
    Some(ct)
}

/// Доля непустых строк без перевода, начиная с которой перевод считается проваленным: озвучить ролик
/// на исходном языке под видом дубляжа хуже, чем остановиться с причиной.
const UNTRANSLATED_FAIL_SHARE: f64 = 0.5;

/// Прогнать стадию. proj уже собран транскрипт-стадией (segments + mode/tgt_lang). vh/total — из probe.
/// Err — перевод нужен (дубляж, закадр, перевод субтитров, ремикс), но не выполнен.
pub fn stage(
    args: &AnalyzeArgs,
    paths: &AnalyzePaths,
    proj: &mut Project,
    vocals16: &std::path::Path,
    vh: i64,
    total: f64,
    progress: &Progress,
) -> Result<(), String> {
    // Нет сегментов -> нечего переводить (auto-nodub / музыка). Как ранний return в питоне.
    if proj.segments.is_empty() {
        return Ok(());
    }
    // Импортированы субтитры УЖЕ на языке перевода: tgt заполнен из cues (analyze import-ветка),
    // MT и vision-раскладка не нужны — Даб Студио только озвучивает готовый текст.
    if args.import_translated {
        emit(progress, "translate", &t!("translate-subs-already-translated"));
        return Ok(());
    }
    let rewrite = if args.rewrite.is_empty() { None } else { Some(args.rewrite.as_str()) };
    let do_translate = wants_translate(proj) || rewrite.is_some();
    if !do_translate {
        // transcribe-режим: tgt = исходный текст, БЕЗ MT (parity с pipeline «transcribe» веткой).
        copy_src_to_tgt(proj);
        emit(progress, "translate", &t!("translate-transcribe-only"));
        return Ok(());
    }

    // src == tgt -> оставить исходник, ноль MT (same_lang в питоне). src берём из query (auto -> не знаем
    // язык детерминированно тут; ASR его не вернул типизированно, потому same_lang проверяем лишь по
    // явному src_lang — как str(src).lower()==tgt в питоне при известном src).
    let src = &args.src_lang;
    let src_lc = src.to_lowercase();
    let same_lang = !src.is_empty() && src_lc != "auto" && src_lc == proj.tgt_lang.to_lowercase();
    if same_lang && rewrite.is_none() {
        copy_src_to_tgt(proj);
        emit(progress, "translate", &t!("translate-same-language"));
        return Ok(());
    }

    // Провайдеры перевода и vision выбираются независимо (своя Gemma / локальный сервер / OpenRouter): облачный
    // перевод не требует локальных весов. Перевод недоступен — Err с причиной; недоступный vision перевод
    // не останавливает.
    let pair = match crate::llm_provider::open_pair(&crate::llm_provider::LlmOpen {
        llama_bin: &paths.llama_bin,
        mt_model: &paths.mt_model,
        mmproj: &paths.mmproj,
        models_root: &paths.models_root,
    }) {
        Ok(pair) => {
            emit(progress, "translate", &pair.describe());
            pair
        }
        Err(e) => return Err(t!("translate-no-llm", error = e)),
    };
    let client = pair.text();

    // Авто-детект типа контента для кастинга (#115): юзер выбрал «Авто» + кастинг включён -> классифицируем
    // live-action vs анимация vision-моделью (уже открыта). Нет vision -> casting-стадия сделает автономный
    // детект/дефолт. Результат в проект; casting-стадия прочитает.
    if let (true, Some(vision)) = (args.casting && args.content_type == "auto", pair.vision()) {
        let tmp = paths.work_dir.join("ctype_frame.png");
        let ct = classify_content_type(vision, &paths.input, &tmp, total, |m| {
            emit(progress, "vision", m);
        });
        let _ = std::fs::remove_file(&tmp);
        proj.audio.content_type = ct;
    }

    // Seg-вью для dub-translate (text/speaker). speaker -> i64 (питон speaker=0 по умолчанию). Темп голоса
    // спикера для бюджета длины — калибровка прошлого рендера этого проекта, иначе таблица языка.
    let cps = crate::fitplan::cps_by_segment(&paths.work_dir, proj)?;
    let mut segs: Vec<Seg> = proj
        .segments
        .iter()
        .zip(cps)
        .map(|(s, cps)| {
            let spk = crate::analyze::speaker_to_i64(s.speaker.as_deref());
            let mut seg = Seg::new(s.src_text.clone(), spk);
            seg.start = s.start;
            seg.end = s.end;
            seg.cps = Some(cps);
            seg
        })
        .collect();

    // VISION-layout нужен только когда его выход (sub_style/titles/brands) реально попадёт на экран:
    // вжигание включено И субтитры не «none». Иначе (например «Дубляж без субтитров») это 10-20
    // vision-вызовов Gemma впустую — на длинном видео минуты (баг-репорт юзера).
    let want_layout = proj.subs.burn && proj.subs.mode != "none";
    let cfg = CtxConfig {
        input: paths.input.clone(),
        work_dir: paths.work_dir.clone(),
        tgt_lang: proj.tgt_lang.clone(),
        vocals16: if vocals16.is_file() { Some(vocals16.to_path_buf()) } else { None },
        vh: vh as f64,
        total,
        want_layout,
        // Стиль перевода (#112): из проекта. Тем же путём, что rewrite попадает в ctx_run.
        style: proj.audio.translate_style.clone(),
        glossary: proj.glossary.clone(),
    };

    emit(progress, "vision", &t!("translate-ctx-pass"));
    let contract = dub_translate::Contract::for_client(client);
    let res = ctx_run(client, pair.vision(), &cfg, &contract, &mut segs, rewrite, |m| {
        emit(progress, "vision", m);
    });

    // Сервер больше не нужен -> глушим (освобождаем VRAM, как del llm в питоне перед TTS/берном).
    // ГЕЙТ ПОКРЫТИЯ ПЕРЕВОДА (валидация В пайплайне): сегменты, оставшиеся английскими/непереведёнными
    // (tgt≈src ИЛИ латиница при нелатинском tgt), доперевести точечно flat_run — пока LLM ещё жив.
    let glossary = dub_core::glossary::for_translation(&proj.glossary, &proj.tgt_lang);
    if res.is_ok() {
        ensure_translation_coverage(client, &contract, &mut segs, &args.src_lang, &proj.tgt_lang, &glossary, progress);
    }
    drop(pair); // глушим свою Gemma (освобождаем VRAM перед TTS/берном); удалённые провайдеры — no-op

    let extra = match res {
        Ok(r) => r.extra,
        Err(e) => return Err(t!("translate-failed", error = e.to_string())),
    };

    // Перенести tgt в сегменты Project. segs строился 1:1 из proj.segments и дальше не используется —
    // переносим строки перемещением (zip по равной длине, без клонов).
    for (s, sg) in proj.segments.iter_mut().zip(segs) {
        s.tgt_text = sg.tgt;
    }

    // Замапить extra -> типизированные поля Project + сохранить сырой ctx (byte-identical passthrough,
    // как raw_ctx = ce_d в from_artifacts).
    apply_extra(proj, &extra);

    let spoken: Vec<&dub_core::Segment> =
        proj.segments.iter().filter(|s| !s.src_text.trim().is_empty()).collect();
    let untranslated = spoken
        .iter()
        .filter(|s| looks_untranslated(&s.src_text, &s.tgt_text, &proj.tgt_lang, &glossary))
        .count();
    if rewrite.is_none() && !spoken.is_empty() {
        let share = untranslated as f64 / spoken.len() as f64;
        if share >= UNTRANSLATED_FAIL_SHARE {
            return Err(if src.is_empty() || src_lc == "auto" {
                t!("translate-untranslated-auto", left = untranslated, total = spoken.len())
            } else {
                t!("translate-untranslated", left = untranslated, total = spoken.len())
            });
        }
    }
    emit(progress, "translate", &t!(
        "translate-done",
        done = spoken.len() - untranslated, total = spoken.len(), titles = proj.captions.titles.len()));
    Ok(())
}

/// Строка журнала джобы о строках, оставшихся на исходном языке после перевода; None — переведены все.
/// `pairs` — (что переводилось, что получилось).
pub(crate) fn untranslated_note<'a>(
    pairs: impl Iterator<Item = (&'a str, &'a str)>,
    tgt_lang: &str,
    glossary: &[GlossaryEntry],
) -> Option<String> {
    let (mut total, mut left) = (0usize, 0usize);
    for (src, tgt) in pairs.filter(|(src, _)| !src.trim().is_empty()) {
        total += 1;
        if looks_untranslated(src, tgt, tgt_lang, glossary) {
            left += 1;
        }
    }
    (left > 0).then(|| t!("translate-left-untranslated", left = left, total = total))
}

/// extra (ctx_extra.json) -> типизированные captions.sub_style/sub_y/titles/brands + raw_ctx.
/// Точная параллель project.from_artifacts (строки 239-249): raw_ctx = сырой extra; sub_style/titles/
/// brands десериализуются в типы (extra="allow" ловит все ключи vision-словаря).
fn apply_extra(proj: &mut Project, extra: &Value) {
    // raw_ctx — весь ctx как есть (для будущего byte-identical re-render captions-стадии раунда 4).
    if let Value::Object(m) = extra {
        proj.raw_ctx = m.clone();
    }
    // sub_style
    if let Some(ss) = extra.get("sub_style") {
        if ss.is_object() {
            if let Ok(style) = serde_json::from_value::<SubStyle>(ss.clone()) {
                proj.captions.sub_style = Some(style);
            }
        }
    }
    // sub_y
    if let Some(y) = extra.get("sub_y").and_then(|v| v.as_i64()) {
        proj.captions.sub_y = Some(y);
    }
    // titles: НЕ строим здесь. Их финальный вид (bbox + время + стиль) собирает caption-композит
    // (compose.rs) ПОСЛЕ OCR-стадии — до OCR у нас нет localize-боксов для матчинга y_frac -> bbox, а
    // без bbox emit_title молча скипает титр. raw_ctx["titles"] (уже с tgt) переносится выше как есть,
    // композит читает его оттуда. Порт pipeline.run:497-543 живёт в compose::run.
    // brands
    if let Some(arr) = extra.get("brands").and_then(|v| v.as_array()) {
        proj.captions.brands = arr
            .iter()
            .filter(|b| b.is_object())
            .filter_map(|b| serde_json::from_value::<Brand>(b.clone()).ok())
            .collect();
    }
}


/// Гейт покрытия перевода: доперевести сегменты, оставшиеся непереведёнными (english leak), точечным
/// flat_run по их исходным текстам. До 2 проходов; меняем только реально улучшившиеся tgt; логируем остаток.
fn ensure_translation_coverage(
    client: &ChatClient,
    contract: &dub_translate::Contract,
    segs: &mut [Seg],
    src: &str,
    tgt_lang: &str,
    glossary: &[GlossaryEntry],
    progress: &Progress,
) {
    let bad: Vec<usize> = segs
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.text.trim().is_empty() && looks_untranslated(&s.text, &s.tgt, tgt_lang, glossary))
        .map(|(i, _)| i)
        .collect();
    if bad.is_empty() {
        return;
    }
    emit(progress, "translate", &t!("translate-coverage-retry", count = bad.len()));
    for _ in 0..2 {
        let mut sub: Vec<Seg> = bad
            .iter()
            .map(|&i| {
                let mut g = Seg::new(segs[i].text.clone(), segs[i].speaker);
                g.start = segs[i].start;
                g.end = segs[i].end;
                g.cps = segs[i].cps;
                g
            })
            .collect();
        let opts = FlatOpts { src, tgt: tgt_lang, spoken: true, style: "", glossary, contract };
        if let Err(e) = dub_translate::flat_run_with(client, &mut sub, &opts, &mut |m: &str| emit(progress, "translate", m)) {
            emit(progress, "translate", &t!("translate-coverage-failed", error = e.to_string()));
            break;
        }
        for (k, &i) in bad.iter().enumerate() {
            if !looks_untranslated(&segs[i].text, &sub[k].tgt, tgt_lang, glossary) {
                segs[i].tgt = std::mem::take(&mut sub[k].tgt);
            }
        }
        let still = bad
            .iter()
            .filter(|&&i| looks_untranslated(&segs[i].text, &segs[i].tgt, tgt_lang, glossary))
            .count();
        emit(progress, "translate", &t!("translate-coverage-left", count = still));
        if still == 0 {
            break;
        }
    }
}
