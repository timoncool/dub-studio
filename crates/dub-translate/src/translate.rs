//! Порт dubengine/translate.py — плоский MT через Gemma (llama.cpp): весь транскрипт как нумерованные
//! строки в ОДНОМ вызове (чанки по 40), чтобы каждая строка переводилась в контексте всего диалога;
//! глоссарий пиннит термины и повторяющиеся ИМЕНА. Ответ — по контракту contract.rs (JSON-схема или
//! нумерованные строки), каждая строка проверяется, непрошедшие переспрашиваются (batch::drive).

use std::collections::{HashMap, HashSet};

use dub_core::glossary::{for_translation, normalize};
use dub_core::GlossaryEntry;
use dub_llm::{strip_think, ChatClient, Message, Sampling};
use regex::Regex;

use crate::contract::{label, rule as contract_rule, Answer, Contract, Format, LineCheck};
use crate::seg::Seg;
use crate::{LineFailure, Note, TranslateError};

const CHUNK: usize = 40;

/// _LANGS из translate.py — код -> английское имя языка.
pub(crate) fn lang_name(code: &str, default: &str) -> String {
    let c = code.trim().to_lowercase();
    if c.is_empty() || c == "auto" {
        return default.to_string();
    }
    crate::WHISPER_LANGS
        .iter()
        .find(|(k, _)| *k == c.as_str())
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| code.to_string())
}

/// _name(code, default="the source language").
pub(crate) fn name_src(code: &str) -> String {
    lang_name(code, "the source language")
}

fn has_cjk(s: &str) -> bool {
    // re.search(r"[぀-ヿ一-鿿]") — хирагана/катакана + CJK-иероглифы.
    s.chars().any(|c| ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

/// Слово на позиции `at` строки начинает предложение: перед ним только пробелы, кавычки, тире и скобки, а перед
/// ними — начало строки или конец предложения.
fn sentence_start(t: &str, at: usize) -> bool {
    let before = t[..at].trim_end_matches(|c: char| c.is_whitespace() || "\"'«»“”„‘’([¿¡-–—".contains(c));
    before.is_empty() || before.ends_with(['.', '!', '?', '…', ':'])
}

/// _glossary — пары (имя_src -> имя_tgt) для повторяющихся собственных ИМЁН (заглавные, от 3 повторов).
/// Имя — слово, которое хоть раз стоит с заглавной не в начале предложения и ни разу не пишется со строчной:
/// «You», «What», «The» в начале реплик — служебные слова. Разовый проход: один короткий вызов модели на имя.
/// `limit` — сколько самых частых имён (None — все); имена, уже известные глоссарию, не спрашиваются. Первый
/// сбой вызова обрывает проход с ошибкой.
pub(crate) fn glossary_pairs<'a>(
    llm: &ChatClient,
    texts: impl Iterator<Item = &'a str>,
    src: &str,
    tgt: &str,
    limit: Option<usize>,
    known: &[GlossaryEntry],
) -> Result<Vec<(String, String)>, TranslateError> {
    // counts по \b[A-Z][a-z]{2,}\b
    let re = Regex::new(r"\b[A-Z][a-z]{2,}\b").unwrap();
    let lower_re = Regex::new(r"\b[a-z]{3,}\b").unwrap();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new(); // порядок ПЕРВОГО появления (Counter сохраняет вставку)
    let mut mid: HashSet<String> = HashSet::new();
    let mut lower: HashSet<String> = HashSet::new();
    for t in texts {
        for m in re.find_iter(t) {
            let w = m.as_str().to_string();
            if !sentence_start(t, m.start()) {
                mid.insert(w.clone());
            }
            let e = counts.entry(w.clone()).or_insert(0);
            if *e == 0 {
                order.push(w);
            }
            *e += 1;
        }
        lower.extend(lower_re.find_iter(t).map(|m| m.as_str().to_string()));
    }
    // most_common(limit), c>=3 — по счёту убыв.; ничья -> порядок появления (стабильная сортировка), НЕ алфавит
    let mut items: Vec<(String, usize)> = order.iter().map(|w| (w.clone(), counts[w])).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));
    let known: Vec<String> = known.iter().map(|e| normalize(&e.term)).collect();
    let terms: Vec<String> = items
        .into_iter()
        .filter(|(w, c)| *c >= 3 && mid.contains(w) && !lower.contains(&w.to_lowercase()) && !known.contains(&normalize(w)))
        .take(limit.unwrap_or(usize::MAX))
        .map(|(w, _)| w)
        .collect();

    let mut gloss: Vec<(String, String)> = Vec::new();
    for w in terms {
        let sys = format!(
            "Translate this single name/word from {src} to {tgt}. Output only the {tgt} word."
        );
        let s = Sampling::new(0.2, 0.9, 16);
        let v = strip_think(&llm.chat(&[Message::system(sys), Message::user_text(&w)], &s)?);
        // v.splitlines()[0].strip(' ."')
        let v = v.lines().next().unwrap_or("").trim_matches(|c| c == ' ' || c == '.' || c == '"').to_string();
        if !v.is_empty() && !has_cjk(&v) {
            gloss.push((w, v));
        }
    }
    Ok(gloss)
}

/// Индексы непустых сегментов + число уникальных спикеров среди них (общий шаг run/rewrite).
fn nonempty_idxs_and_nspk(segs: &[Seg]) -> (Vec<usize>, usize) {
    let idxs: Vec<usize> =
        segs.iter().enumerate().filter(|(_, s)| !s.text.trim().is_empty()).map(|(i, _)| i).collect();
    let nspk = idxs
        .iter()
        .map(|&i| segs[i].speaker)
        .collect::<std::collections::HashSet<i64>>()
        .len();
    (idxs, nspk)
}

/// Нумерованный блок "1. текст\n2. текст…" для пакета индексов (общий для run/rewrite; номера — label формата).
/// С мягким лимитом длины (#107): после номера «(≤NN)» из бюджета символов сегмента (темп голоса × длит., Seg::budget), у
/// сегментов без таймингов лимита нет. Лимит вычищается из ответа защитно (strip_budget_marker при разборе).
fn numbered_block(texts: &[String], budgets: &[Option<usize>], chunk: &[usize], fmt: Format) -> String {
    chunk
        .iter()
        .enumerate()
        .map(|(j, &gi)| match budgets[gi] {
            Some(lim) => format!("{}. (\u{2264}{lim}) {}", label(fmt, j + 1, chunk.len()), texts[gi]),
            None => format!("{}. {}", label(fmt, j + 1, chunk.len()), texts[gi]),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Вычистить ведущий маркер лимита «(≤NN)» из перевода (если модель его протащила). Устойчиво (#116,
/// находка [14]): пробелы после «(» и перед числом, варианты ≤/<=/=<.
pub(crate) fn strip_budget_marker(s: &str) -> String {
    let re = Regex::new(r"^\s*\(\s*(?:\u{2264}|<=|=<)\s*\d+\s*\)\s*").unwrap();
    re.replace(s, "").into_owned()
}

/// Доп-инструкция стиля перевода (#112): отдельное предложение в конце инструкционной части sysmsg. Пусто,
/// если стиль не задан. Формат-контракт ответа ставится ПОСЛЕ этого текста и остаётся приоритетным, поэтому
/// стиль не может сломать разбор ответа.
pub(crate) fn style_clause(style: &str) -> String {
    let s = style.trim();
    if s.is_empty() {
        String::new()
    } else {
        format!(" Translation style: {s}.")
    }
}

/// Параметры плоского перевода.
pub struct FlatOpts<'a> {
    pub src: &'a str,
    pub tgt: &'a str,
    /// Озвучка: числа, даты и символы — словами.
    pub spoken: bool,
    /// Стиль перевода (#112); пусто — без стиля.
    pub style: &'a str,
    /// Глоссарий проекта; записи другого языка перевод не задают.
    pub glossary: &'a [GlossaryEntry],
    /// Формат ответа джобы (общий для всех её проходов).
    pub contract: &'a Contract,
}

/// Пакеты плоского прохода: общий для run/rewrite цикл спросить -> проверить -> переспросить.
struct Flat<'a> {
    llm: &'a ChatClient,
    contract: &'a Contract,
    texts: Vec<String>,
    budgets: Vec<Option<usize>>,
    tgt_code: &'a str,
    tgt_name: String,
    glossary: Vec<GlossaryEntry>,
    /// Авто-пары повторяющихся имён: подсказка и term-lock, не проверка.
    names: Vec<(String, String)>,
    rewrite: bool,
}

impl Flat<'_> {
    fn run(
        &self,
        idxs: &[usize],
        system: &dyn Fn(Format) -> String,
        sampling: &dyn Fn(usize) -> Sampling,
        log: &mut dyn FnMut(&Note),
    ) -> Result<crate::batch::Outcome, TranslateError> {
        self.contract.announce(self.llm, log);
        let log_cell = std::cell::RefCell::new(log);
        let mut ask = |idx: &[usize], _: &HashMap<usize, String>| -> Result<Answer, TranslateError> {
            let texts: Vec<&str> = idx.iter().map(|&i| self.texts[i].as_str()).collect();
            let gloss_block = if self.rewrite { String::new() } else { crate::gloss::block(&self.glossary, &texts) };
            let messages = |fmt: Format| {
                let numbered = numbered_block(&self.texts, &self.budgets, idx, fmt);
                vec![
                    Message::system(system(fmt)),
                    Message::user_text(format!("{gloss_block}{numbered}\n\n{}", contract_rule(fmt, &self.tgt_name))),
                ]
            };
            let mut answer = self.contract.ask(self.llm, &messages, &sampling(idx.len()), idx.len(), &mut |m: &Note| (log_cell.borrow_mut())(m))?;
            if !self.rewrite {
                for line in answer.lines.iter_mut().flatten() {
                    *line = crate::gloss::term_lock(line, &self.glossary, &self.names);
                }
            }
            Ok(answer)
        };
        let check = |i: usize, line: Option<&str>, cut: bool| {
            LineCheck { src: &self.texts[i], budget: self.budgets[i], tgt_lang: self.tgt_code, glossary: &self.glossary, rewrite: self.rewrite }
                .check(line, cut)
        };
        let chunks: Vec<Vec<usize>> = idxs.chunks(CHUNK).map(<[usize]>::to_vec).collect();
        crate::batch::drive(chunks, &mut ask, &check, &|i| i + 1, &mut |m: &Note| (log_cell.borrow_mut())(m))
    }
}

/// run — перевод каждого seg.text -> seg.tgt (плоский MT, порт _run_hunyuan) без глоссария проекта; журнал —
/// в stderr сервера.
pub fn run(
    llm: &ChatClient,
    segs: &mut [Seg],
    src: &str,
    tgt: &str,
    spoken: bool,
    style: &str,
) -> Result<(), TranslateError> {
    let contract = Contract::for_client(llm);
    run_with(llm, segs, &FlatOpts { src, tgt, spoken, style, glossary: &[], contract: &contract }, &mut |m: &Note| eprintln!("[translate] {m}"))
}

/// run с глоссарием и журналом. style (#112) — доп-инструкция стиля, вставляется в инструкционную часть.
pub fn run_with(llm: &ChatClient, segs: &mut [Seg], o: &FlatOpts, log: &mut dyn FnMut(&Note)) -> Result<(), TranslateError> {
    let tgt_name = lang_name(o.tgt, o.tgt);
    let glossary = for_translation(o.glossary, o.tgt);
    let names = match glossary_pairs(llm, segs.iter().map(|s| s.text.as_str()), &name_src(o.src), &tgt_name, Some(6), &glossary) {
        Ok(pairs) => pairs,
        Err(e) => {
            log(&Note::NamesSkipped { error: &e });
            Vec::new()
        }
    };
    let extra = if o.spoken {
        " Spell out all numbers, dates, times and symbols as full words."
    } else {
        ""
    };
    let style_c = style_clause(o.style);
    for s in segs.iter_mut() {
        s.tgt = String::new();
    }
    let (idxs, nspk) = nonempty_idxs_and_nspk(segs);
    let dlg = if nspk > 1 {
        format!(
            " This is a DIALOGUE between {nspk} speakers taking turns — render it as one coherent \
             back-and-forth conversation, keeping each speaker's voice and tone consistent."
        )
    } else {
        String::new()
    };
    let gloss_rule = if glossary.is_empty() { "" } else { crate::gloss::RULE };
    let names_c = crate::gloss::names_clause(&names);
    // Инструкция одна на все пакеты (KV-кэш llama-server); стиль (#112) и глоссарий — ПЕРЕД форматом ответа,
    // он остаётся финальным и приоритетным.
    let system = |fmt: Format| {
        format!(
            "You are a professional subtitle translator localizing a video for DUBBING into {tgt_name}.\
             {dlg} Use the WHOLE numbered list as shared context so each line (even one word) is correct and \
             consistent. Preserve the MEANING, write natural SPOKEN {tgt_name}, and keep each line about the \
             SAME LENGTH as its source so it fits the dub timing. After each number, a parenthesis like \
             (\u{2264}45) gives a soft character limit for that line — stay within it: if it doesn't fit, drop \
             filler words and repetitions, keep the meaning, invent nothing. Do NOT copy the (\u{2264}NN) marker \
             into your output.{extra}{style_c}{names_c}{gloss_rule} {} No reasoning, no English, no notes.",
            contract_rule(fmt, &tgt_name)
        )
    };
    let sampling = |n: usize| Sampling::new(0.3, 0.9, (96 + 52 * n).min(4096) as u32).top_k(20).repeat_penalty(1.05);
    let flat = Flat {
        llm,
        contract: o.contract,
        texts: segs.iter().map(|s| s.text.trim().to_string()).collect(),
        budgets: segs.iter().map(|s| s.budget(o.tgt)).collect(),
        tgt_code: o.tgt,
        tgt_name: tgt_name.clone(),
        glossary,
        names,
        rewrite: false,
    };
    let out = flat.run(&idxs, &system, &sampling, log)?;
    let last_failure: Option<LineFailure> = out.failed.last().and_then(|f| f.1.clone());
    for (i, t) in out.accepted {
        segs[i].tgt = crate::fix_translation(&t, o.tgt);
    }
    // деградация: пустые -> оставить исходник, чтобы дубляж не был пуст (как в питоне).
    let empty: Vec<usize> = idxs.iter().cloned().filter(|&gi| segs[gi].tgt.is_empty()).collect();
    for &gi in &empty {
        segs[gi].tgt = segs[gi].text.trim().to_string();
    }
    if !idxs.is_empty() && empty.len() == idxs.len() {
        return Err(TranslateError::Empty { lines: idxs.len(), last: last_failure });
    }
    Ok(())
}

/// rewrite — творческое ПЕРЕОЗВУЧИВАНИЕ всего транскрипта по инструкции. Порт translate.rewrite.
pub fn rewrite(
    llm: &ChatClient,
    segs: &mut [Seg],
    instruction: &str,
    _src: &str,
    tgt: &str,
    spoken: bool,
    style: &str,
) -> Result<(), TranslateError> {
    let tgt_name = lang_name(tgt, tgt);
    let style_c = style_clause(style);
    for s in segs.iter_mut() {
        s.tgt = String::new();
    }
    let (idxs, nspk) = nonempty_idxs_and_nspk(segs);
    let extra = if spoken {
        " Spell out all numbers, dates, times and symbols as full words."
    } else {
        ""
    };
    let dlg = if nspk > 1 {
        format!(" It is a dialogue between {nspk} speakers taking turns — keep the back-and-forth.")
    } else {
        String::new()
    };
    // То же, что ctx.rs: ЗАМЕНИТЬ содержимое на тему/стиль инструкции, НЕ переводить исходник (иначе Q4
    // просто переводит, тема не меняется — репорт юзера). Оба пути (funny-анализ и editor-remix) одинаковы.
    let system = |fmt: Format| {
        format!(
            "You are a creative scriptwriter writing a BRAND-NEW voice-over script in {tgt_name} for a video.{dlg} \
             IGNORE the literal meaning of the source lines — they are ONLY a rhythm/length template. Write a completely \
             NEW script whose CONTENT follows this instruction: \"{instruction}\". Every line must fit the instruction, \
             NOT translate the source. Keep the SAME number of lines and make each new line roughly the SAME LENGTH as \
             its source line so it fits the dub timing. After each number, a parenthesis like (\u{2264}45) gives a soft \
             character limit for that line — stay within it, invent nothing, and do NOT copy the (\u{2264}NN) marker into \
             your output.{extra}{style_c} Output natural spoken {tgt_name}. {} No notes, no source text.",
            contract_rule(fmt, &tgt_name)
        )
    };
    let sampling = |n: usize| Sampling::new(0.85, 0.95, (128 + 64 * n).min(4096) as u32).top_k(40).repeat_penalty(1.05);
    let contract = Contract::for_client(llm);
    let flat = Flat {
        llm,
        contract: &contract,
        texts: segs.iter().map(|s| s.text.trim().to_string()).collect(),
        budgets: segs.iter().map(|s| s.budget(tgt)).collect(),
        tgt_code: tgt,
        tgt_name: tgt_name.clone(),
        glossary: Vec::new(),
        names: Vec::new(),
        rewrite: true,
    };
    let out = flat.run(&idxs, &system, &sampling, &mut |m: &Note| eprintln!("[remix] {m}"))?;
    for &gi in &idxs {
        // строка, которую так и не удалось переписать, остаётся исходной (не пустая озвучка).
        segs[gi].tgt = match out.accepted.get(&gi) {
            Some(t) => crate::fix_translation(t, tgt),
            None => segs[gi].text.trim().to_string(),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_llm::test_http::{body_json, serve, Reply};

    fn reply(content: &str) -> Reply {
        Reply::json(200, &serde_json::json!({ "choices": [{ "message": { "content": content }, "finish_reason": "stop" }] }).to_string())
    }

    #[test]
    fn lang_names() {
        assert_eq!(lang_name("ru", "x"), "Russian");
        assert_eq!(name_src("auto"), "the source language");
        assert_eq!(lang_name("xx", "fallback"), "xx");
    }

    #[test]
    fn cjk_detect() {
        assert!(has_cjk("こんにちは"));
        assert!(has_cjk("中文"));
        assert!(!has_cjk("Hello"));
    }

    #[test]
    fn budget_from_duration_and_the_language_rate() {
        let timed = |end: f64| {
            let mut s = Seg::new("x", 0);
            s.end = end;
            s
        };
        assert_eq!(timed(3.0).budget("ru"), Some(39));
        assert_eq!(timed(3.0).budget("zh"), Some(17));
        assert_eq!(timed(0.0).budget("ru"), None);
        assert_eq!(timed(-1.0).budget("ru"), None);
        assert_eq!(timed(0.01).budget("ru"), Some(12)); // пол бюджета 12 (#116) — междометие не в «(≤1)»
    }

    #[test]
    fn strip_leading_budget_marker() {
        assert_eq!(strip_budget_marker("(≤45) Привет"), "Привет");
        assert_eq!(strip_budget_marker("(<=30)  Текст"), "Текст");
        // устойчивость (#116): пробелы после «(», перед числом, вариант =<
        assert_eq!(strip_budget_marker("( ≤ 45) Привет"), "Привет");
        assert_eq!(strip_budget_marker("(=< 30) Текст"), "Текст");
        assert_eq!(strip_budget_marker("Обычный текст"), "Обычный текст");
        // цифры/скобки внутри перевода не трогаем
        assert_eq!(strip_budget_marker("В 2024 (год) было"), "В 2024 (год) было");
    }

    #[test]
    fn numbered_block_has_budget_when_timed() {
        let texts = vec!["hello world".to_string(), "no timing".to_string()];
        let block = numbered_block(&texts, &[Some(42), None], &[0, 1], Format::Json);
        assert_eq!(block, "1. (≤42) hello world\n2. no timing");
    }

    fn segs(lines: &[&str]) -> Vec<Seg> {
        lines
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut s = Seg::new(*t, 0);
                s.start = i as f64 * 3.0;
                s.end = s.start + 2.5;
                s
            })
            .collect()
    }

    #[test]
    fn a_bad_line_is_asked_again_and_the_glossary_goes_with_its_lines() {
        let server = serve(vec![
            reply(r#"{"1":"Добро пожаловать в Хогвартс","2":"I am fine"}"#),
            reply(r#"{"1":"У меня всё хорошо"}"#),
        ]);
        let llm = ChatClient::new(server.base()).unwrap();
        let mut s = segs(&["Welcome to Hogwarts", "I am fine"]);
        let glossary = vec![GlossaryEntry { term: "Hogwarts".into(), translation: "Хогвартс".into(), lang: "ru".into(), ..GlossaryEntry::default() }];
        let mut log = Vec::new();
        run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: true, style: "", glossary: &glossary, contract: &Contract::for_client(&llm) }, &mut |m: &Note| log.push(m.to_string())).unwrap();
        assert_eq!(s[0].tgt, "Добро пожаловать в Хогвартс");
        assert_eq!(s[1].tgt, "У меня всё хорошо");
        let first = body_json(&server.request(0));
        let second = body_json(&server.request(1));
        assert_eq!(first["messages"][0], second["messages"][0], "the instruction is the same for every batch");
        assert!(first["messages"][1]["content"].as_str().unwrap().contains("=== GLOSSARY ===\nHogwarts → Хогвартс"));
        assert!(!second["messages"][1]["content"].as_str().unwrap().contains("GLOSSARY"), "only the terms of the batch");
        assert_eq!(second["response_format"]["json_schema"]["schema"]["required"], serde_json::json!(["1"]));
        assert!(log.iter().any(|l| l.contains("JSON")), "{log:?}");
        assert!(log.iter().any(|l| l.contains("failed the check")), "{log:?}");
    }

    #[test]
    fn a_line_that_stays_untranslated_falls_back_to_the_source() {
        let server = serve(vec![reply(r#"{"1":"Привет","2":"Bye"}"#), reply(r#"{"1":"Bye"}"#)]);
        let llm = ChatClient::new(server.base()).unwrap();
        let mut s = segs(&["Hello", "Bye"]);
        run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: false, style: "", glossary: &[], contract: &Contract::for_client(&llm) }, &mut |_: &Note| {}).unwrap();
        assert_eq!(s[0].tgt, "Привет");
        assert_eq!(s[1].tgt, "Bye");
        assert_eq!(server.count(), 2);
    }

    #[test]
    fn a_failing_name_glossary_does_not_stop_the_translation() {
        let server = serve(vec![
            Reply::json(200, r#"{"choices":[{"message":{"content":"Гар"},"finish_reason":"length"}]}"#),
            Reply::json(200, r#"{"choices":[{"message":{"content":"Гар"},"finish_reason":"length"}]}"#),
            reply(r#"{"1":"Гарри пришёл","2":"Гарри ушёл","3":"Гарри вернулся"}"#),
        ]);
        let llm = ChatClient::openrouter_at(&server.base(), "k", "vendor/thinker").unwrap().with_profile(Some(dub_llm::openrouter::ModelProfile {
            supported_parameters: vec!["structured_outputs".into()],
            ..Default::default()
        }));
        let mut s = segs(&["I saw Harry", "Then Harry left", "Harry is back"]);
        let mut log = Vec::new();
        run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: false, style: "", glossary: &[], contract: &Contract::for_client(&llm) }, &mut |m: &Note| log.push(m.to_string())).unwrap();
        assert_eq!(s[2].tgt, "Гарри вернулся");
        assert!(log.iter().any(|l| l.contains("automatic name glossary is skipped")), "{log:?}");
    }

    #[test]
    fn sentence_openers_are_not_names() {
        let server = serve(vec![reply("Гарри")]);
        let llm = ChatClient::new(server.base()).unwrap();
        let texts = ["You know it.", "You again?", "What? You did it.", "Well, Harry is here.", "I told Harry.", "Harry! What now?", "so what"];
        let pairs = glossary_pairs(&llm, texts.into_iter(), "English", "Russian", None, &[]).unwrap();
        assert_eq!(pairs, vec![("Harry".to_string(), "Гарри".to_string())]);
        assert_eq!(server.count(), 1);
        let at = |t: &str, w: &str| t.find(w).unwrap();
        assert!(sentence_start("«Stop!» You", at("«Stop!» You", "You")));
        assert!(sentence_start("  — You", at("  — You", "You")));
        assert!(!sentence_start("I told Harry", at("I told Harry", "Harry")));
    }

    #[test]
    fn auto_names_are_a_hint_and_never_a_required_term() {
        let server = serve(vec![
            reply("Гарри"),
            reply(r#"{"1":"Ты знаешь его","2":"Ты видел Гарри","3":"Снова ты и он"}"#),
        ]);
        let llm = ChatClient::new(server.base()).unwrap();
        let mut s = segs(&["You know Harry", "You saw Harry", "You and Harry again"]);
        run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: false, style: "", glossary: &[], contract: &Contract::for_client(&llm) }, &mut |_: &Note| {}).unwrap();
        assert_eq!(server.count(), 2, "one name asked, one batch, no retries");
        assert_eq!(body_json(&server.request(0))["messages"][1]["content"], "Harry");
        let batch = body_json(&server.request(1));
        assert!(batch["messages"][0]["content"].as_str().unwrap().contains("Keep these names consistent: Harry=Гарри."));
        assert!(!batch["messages"][1]["content"].as_str().unwrap().contains("GLOSSARY"));
        assert_eq!(s[0].tgt, "Ты знаешь его");
    }

    #[test]
    fn a_dead_or_refusing_server_stops_the_translation_with_its_cause() {
        let server = serve(vec![Reply::json(401, r#"{"error":"invalid key"}"#)]);
        let llm = ChatClient::new(server.base()).unwrap();
        let mut s = segs(&["one line", "two lines", "three lines"]);
        let err = run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: false, style: "", glossary: &[], contract: &Contract::for_client(&llm) }, &mut |_: &Note| {}).unwrap_err();
        assert!(err.to_string().contains("401") && err.to_string().contains("invalid key"), "{err}");
        assert_eq!(server.count(), 1);

        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let llm = ChatClient::new(format!("http://127.0.0.1:{port}")).unwrap().with_retries(0);
        let err = run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: false, style: "", glossary: &[], contract: &Contract::for_client(&llm) }, &mut |_: &Note| {}).unwrap_err();
        assert!(err.to_string().contains("chat failed"), "{err}");
    }

    #[test]
    fn nothing_translated_names_the_last_cause() {
        let server = serve(vec![reply(r#"{"1":"one line"}"#), reply(r#"{"1":"one line"}"#)]);
        let llm = ChatClient::new(server.base()).unwrap();
        let mut s = segs(&["one line"]);
        let err = run_with(&llm, &mut s, &FlatOpts { src: "en", tgt: "ru", spoken: false, style: "", glossary: &[], contract: &Contract::for_client(&llm) }, &mut |_: &Note| {}).unwrap_err();
        assert!(matches!(&err, TranslateError::Empty { lines: 1, last: Some(LineFailure::Rejected(crate::Reject::Echo)) }), "{err}");
    }
}
