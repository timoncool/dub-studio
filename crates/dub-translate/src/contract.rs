//! Контракт ответа перевода и проверка каждой его строки.
//!
//! Формат выбирается по возможностям модели один раз на джобу: JSON-объект {"1": "...", …, "N": "..."} по
//! схеме (response_format json_schema: свой llama-server строит из неё грамматику; модель OpenRouter —
//! если заявляет structured_outputs) или прежние нумерованные строки «N. перевод». Свой OpenAI-совместимый
//! сервер пробует схему; отказ 400/422 переводит джобу на нумерованный формат со строкой в журнале. Так же
//! и OpenRouter, когда у модели не нашлось провайдера с ответом по схеме (404) или схему отвергли (400/422).

use std::cell::Cell;
use std::collections::HashMap;

use dub_core::glossary::{contains_in_translation, normalize, GlossaryEntry};
use dub_llm::{strip_think, ChatClient, Endpoint, LlmError, Message, Sampling, StructuredOutput};
use serde_json::{json, Map, Value};

use crate::{AnswerProblem, Note, TranslateError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Json,
    Numbered,
}

/// Формат ответа джобы перевода и что о нём уже сказано в журнале. Джоба создаёт его один раз и передаёт во
/// все свои проходы: схема на своём сервере пробуется один раз, строка о формате пишется один раз.
pub struct Contract {
    format: Cell<Format>,
    /// Схема ещё не проверена на этом сервере: отказ на неё — не ошибка, а смена формата.
    probing: Cell<bool>,
    announced: Cell<bool>,
}

/// Разобранный ответ: строки 1..n (None — модель строку не вернула) и причина остановки.
pub(crate) struct Answer {
    pub lines: Vec<Option<String>>,
    pub finish_reason: String,
}

impl Contract {
    pub fn for_client(llm: &ChatClient) -> Self {
        let (format, probing) = match llm.structured_output() {
            StructuredOutput::Supported => (Format::Json, false),
            StructuredOutput::Untested => (Format::Json, true),
            StructuredOutput::Unsupported => (Format::Numbered, false),
        };
        Contract { format: Cell::new(format), probing: Cell::new(probing), announced: Cell::new(false) }
    }

    #[cfg(test)]
    pub(crate) fn format(&self) -> Format {
        self.format.get()
    }

    /// Одна строка в журнал на джобу: каким форматом отвечает модель и почему.
    pub(crate) fn announce(&self, llm: &ChatClient, log: &mut dyn FnMut(&Note)) {
        if self.announced.replace(true) {
            return;
        }
        let model = llm.model().unwrap_or("Gemma (llama-server)");
        log(&match (self.format.get(), self.probing.get()) {
            (Format::Json, false) => Note::FormatJson { model },
            (Format::Json, true) => Note::FormatJsonProbe { model },
            (Format::Numbered, _) => Note::FormatNumbered { model },
        });
    }

    /// Запрос на n строк. `messages` строит сообщения под формат (правило формата в них меняется).
    pub(crate) fn ask(
        &self,
        llm: &ChatClient,
        messages: &dyn Fn(Format) -> Vec<Message>,
        s: &Sampling,
        n: usize,
        log: &mut dyn FnMut(&Note),
    ) -> Result<Answer, TranslateError> {
        if self.format.get() == Format::Json {
            let schema = schema(n);
            match llm.complete(&messages(Format::Json), s, Some(&schema)) {
                Ok(done) => {
                    let probing = self.probing.replace(false);
                    let raw = strip_think(&done.text);
                    let err = match parse_json(&raw, n) {
                        Ok(lines) => return Ok(Answer { lines, finish_reason: done.finish_reason }),
                        Err(e) => e,
                    };
                    // Сервер принял схему, но модель её не держит и пишет нумерованные строки.
                    let numbered = if probing && !raw.contains('{') { parse_numbered(&raw, n) } else { Vec::new() };
                    if numbered.iter().any(Option::is_some) {
                        self.format.set(Format::Numbered);
                        log(&Note::SchemaIgnored { model: llm.model() });
                        return Ok(Answer { lines: numbered, finish_reason: done.finish_reason });
                    }
                    return Err(TranslateError::Contract { problem: err, answer: Some(preview(&raw)) });
                }
                Err(LlmError::Rejected { code, status, body }) if schema_refused(llm, code, &body, self.probing.get()) => {
                    self.format.set(Format::Numbered);
                    self.probing.set(false);
                    log(&Note::SchemaRefused { model: llm.model(), status: &status, body: preview(&body) });
                }
                Err(e) => return Err(e.into()),
            }
        }
        let done = llm.complete(&messages(Format::Numbered), s, None)?;
        let raw = strip_think(&done.text);
        Ok(Answer { lines: parse_numbered(&raw, n), finish_reason: done.finish_reason })
    }
}

/// Отказ сервера — отказ от ответа по схеме, а не сбой запроса. Свой сервер, пока схема не проверена: 400/422.
/// OpenRouter: 404 (ни один провайдер модели не принимает запрос со схемой) или 400/422 о response_format.
pub(crate) fn schema_refused(llm: &ChatClient, code: u16, body: &str, probing: bool) -> bool {
    match llm.endpoint() {
        Endpoint::OpenRouter => {
            let b = body.to_ascii_lowercase();
            code == 404 || (matches!(code, 400 | 422) && ["response_format", "json_schema", "structured"].iter().any(|k| b.contains(k)))
        }
        Endpoint::OpenAiCompatible | Endpoint::LlamaServer => probing && matches!(code, 400 | 422),
    }
}

/// Номер k-й строки (с 1) пакета из n строк — перед строкой в промпте и ключом ответа. В JSON номера одной
/// ширины ("01".."12"): serde_json без preserve_order пишет ключи объекта по алфавиту, а грамматика
/// llama-server и strict-схема требуют свойства в порядке схемы — так он совпадает с порядком строк.
pub(crate) fn label(fmt: Format, k: usize, n: usize) -> String {
    match fmt {
        Format::Json => format!("{k:0w$}", w = n.to_string().len()),
        Format::Numbered => k.to_string(),
    }
}

/// Правило формата ответа для инструкции модели.
pub(crate) fn rule(fmt: Format, lang: &str) -> String {
    match fmt {
        Format::Json => format!(
            "Reply with ONLY a JSON object whose keys are the line numbers exactly as written before the lines \
(\"1\", \"2\", … or \"01\", \"02\", …) and whose values are the {lang} lines — every number, nothing else."
        ),
        Format::Numbered => format!("Output ONLY 'N. <{lang} line>' per line, one per line, nothing else."),
    }
}

fn preview(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 160 {
        format!("{}…", one.chars().take(160).collect::<String>())
    } else {
        one
    }
}

/// Схема ответа на n строк: объект с ключами-номерами 1..n (label), все строки и все обязательны, лишних
/// ключей нет.
pub(crate) fn schema(n: usize) -> Value {
    let keys: Vec<String> = (1..=n).map(|k| label(Format::Json, k, n)).collect();
    let props: Map<String, Value> = keys.iter().map(|k| (k.clone(), json!({ "type": "string" }))).collect();
    json!({ "type": "object", "properties": props, "required": keys, "additionalProperties": false })
}

/// Одна строка ответа: без маркера лимита «(≤NN)» и лишних пробелов.
fn clean_line(s: &str) -> String {
    crate::translate::strip_budget_marker(s.trim()).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// JSON-ответ -> строки 1..n; ключ — номер с ведущими нулями или без. Объект ищется между первой «{» и
/// последней «}» (модель могла обернуть его в блок кода json). Не объект — ошибка.
pub(crate) fn parse_json(raw: &str, n: usize) -> Result<Vec<Option<String>>, AnswerProblem> {
    let (Some(a), Some(b)) = (raw.find('{'), raw.rfind('}')) else {
        return Err(AnswerProblem::NoJsonObject);
    };
    if b < a {
        return Err(AnswerProblem::NoJsonObject);
    }
    let map: Map<String, Value> = serde_json::from_str(&raw[a..=b]).map_err(|e| AnswerProblem::NotJson(e.to_string()))?;
    let mut by_num: HashMap<usize, &Value> = HashMap::new();
    for (key, value) in &map {
        if let Ok(k) = key.trim().parse::<usize>() {
            by_num.entry(k).or_insert(value);
        }
    }
    Ok((1..=n)
        .map(|k| by_num.get(&k).and_then(|v| v.as_str()).map(clean_line).filter(|t| !t.is_empty()))
        .collect())
}

/// Нумерованный ответ «N. текст» -> строки 1..n; повтор номера — берётся первое вхождение.
pub(crate) fn parse_numbered(raw: &str, n: usize) -> Vec<Option<String>> {
    let re = regex::Regex::new(r"^\s*(\d+)\s*[.)\]:]\s*(.+)").unwrap();
    let mut got: Vec<Option<String>> = vec![None; n];
    for line in raw.lines() {
        if let Some(c) = re.captures(line) {
            let i: usize = c[1].parse().unwrap_or(0);
            if (1..=n).contains(&i) && got[i - 1].is_none() {
                let v = clean_line(&c[2]);
                if !v.is_empty() {
                    got[i - 1] = Some(v);
                }
            }
        }
    }
    got
}

// ── проверки строки ───────────────────────────────────────────────────────

/// Почему строка ответа не принята.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Reject {
    /// Строки в ответе нет.
    #[error("missing from the answer")]
    Missing,
    /// Ответ оборван лимитом токенов (finish_reason=length).
    #[error("the answer was cut by the token limit")]
    Cut,
    /// Не алфавит целевого языка.
    #[error("not in the target language")]
    Untranslated,
    /// Повтор исходника.
    #[error("repeats the source")]
    Echo,
    #[error("too short ({got} < {min})")]
    TooShort { got: usize, min: usize },
    #[error("too long ({got} > {max})")]
    TooLong { got: usize, max: usize },
    /// Одна n-грамма три раза подряд.
    #[error("loops on {0:?}")]
    Loop(String),
    /// Нет обязательного по глоссарию текста.
    #[error("the glossary term {0:?} is missing")]
    Term(String),
}

impl Reject {
    /// Жёсткий отказ: такой ответ не перевод вовсе. Мягкий — перевод с изъяном: после всех повторов его
    /// лучше оставить (с записью в журнал), чем озвучить исходник.
    pub fn hard(&self) -> bool {
        matches!(self, Reject::Missing | Reject::Cut | Reject::Untranslated | Reject::Echo)
    }

    /// Стабильный код отказа (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            Reject::Missing => "line_missing",
            Reject::Cut => "line_cut",
            Reject::Untranslated => "line_untranslated",
            Reject::Echo => "line_echo",
            Reject::TooShort { .. } => "line_too_short",
            Reject::TooLong { .. } => "line_too_long",
            Reject::Loop(_) => "line_loop",
            Reject::Term(_) => "line_term_missing",
        }
    }
}

/// Длина ответа к бюджету строки: не короче 0.3 от меньшего из бюджета и исходника (короткая реплика
/// «Yes.» — «Да.» не обрыв) и не длиннее 3.0 бюджета.
const MIN_RATIO: f64 = 0.3;
const MAX_RATIO: f64 = 3.0;

/// Всё, против чего проверяется одна строка.
pub(crate) struct LineCheck<'a> {
    pub src: &'a str,
    pub budget: Option<usize>,
    pub tgt_lang: &'a str,
    /// Глоссарий перевода (glossary::for_translation); проверяются записи, чей термин есть в исходной строке.
    pub glossary: &'a [GlossaryEntry],
    /// Творческий ремикс: содержание заменяется, глоссарий не требуется.
    pub rewrite: bool,
}

/// Иероглифы, кана и хангыль весят как два символа: одна такая буква несёт слог.
fn weighted_len(s: &str) -> usize {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF) { 2 } else { 1 })
        .sum()
}

fn words_lower(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(normalize).collect()
}

/// Токены для поиска повторов: слова, а у письма без пробелов — знаки.
fn loop_tokens(s: &str) -> Vec<String> {
    let unspaced = s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0x0E00..=0x0E7F));
    if unspaced {
        s.chars().filter(|c| c.is_alphanumeric()).map(|c| c.to_lowercase().collect()).collect()
    } else {
        words_lower(s)
    }
}

/// Первая n-грамма, повторённая три раза подряд.
pub(crate) fn repeated_ngram(s: &str) -> Option<String> {
    let t = loop_tokens(s);
    for n in 1..=t.len() / 3 {
        for i in 0..=t.len() - 3 * n {
            if t[i..i + n] == t[i + n..i + 2 * n] && t[i..i + n] == t[i + 2 * n..i + 3 * n] {
                return Some(t[i..i + n].join(" "));
            }
        }
    }
    None
}

/// Целевой язык пишется НЕлатиницей (кириллица/CJK/RTL/индийские/…).
pub fn tgt_expects_non_latin(lang: &str) -> bool {
    let l = lang.split(['-', '_']).next().unwrap_or(lang).to_ascii_lowercase();
    matches!(
        l.as_str(),
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" | "kk" | "ky" | "tg" | "mn" | "ab" | "os"
            | "zh" | "ja" | "ko"
            | "ar" | "fa" | "ur" | "he" | "ps" | "sd"
            | "el" | "hy" | "ka" | "hi" | "bn" | "pa" | "gu" | "ta" | "te" | "kn" | "ml"
            | "th" | "lo" | "km" | "my" | "si" | "am"
    )
}

/// Слова ответа, которые законно остаются как в исходнике: числа, термины «не переводить» и имена —
/// слово с заглавной не в начале строки, которое есть и в исходнике.
fn kept_as_is(out: &str, src: &str, glossary: &[GlossaryEntry]) -> Vec<bool> {
    let src_words = words_lower(src);
    let keep_words: Vec<String> = glossary.iter().filter(|e| e.keep).flat_map(|e| words_lower(&e.term)).collect();
    out.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .enumerate()
        .map(|(i, w)| {
            let n = normalize(w);
            w.chars().all(|c| c.is_numeric())
                || keep_words.contains(&n)
                || (i > 0 && w.chars().next().is_some_and(char::is_uppercase) && src_words.contains(&n))
        })
        .collect()
}

/// Строка выглядит непереведённой: пусто, повтор исходника или латиница при нелатинском целевом языке.
/// Числа, имена из исходника и термины «не переводить» не в счёт; однословная реплика на латинском
/// целевом языке может совпасть с исходником («No» — «No»).
pub fn looks_untranslated(src: &str, tgt: &str, tgt_lang: &str, glossary: &[GlossaryEntry]) -> bool {
    untranslated_reason(src, tgt, tgt_lang, glossary).is_some()
}

fn untranslated_reason(src: &str, tgt: &str, tgt_lang: &str, glossary: &[GlossaryEntry]) -> Option<Reject> {
    let t = tgt.trim();
    if t.is_empty() {
        return Some(Reject::Missing);
    }
    let kept = kept_as_is(t, src, glossary);
    let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let only_kept = !kept.is_empty() && kept.iter().all(|k| *k);
    let (sw, tw) = (words_lower(src), words_lower(t));
    let single_latin_word = sw.len() <= 1 && !tgt_expects_non_latin(tgt_lang);
    if !only_kept && !single_latin_word && !sw.is_empty() && sw == tw {
        return Some(Reject::Echo);
    }
    if tgt_expects_non_latin(tgt_lang) {
        let (mut letters, mut latin) = (0usize, 0usize);
        for (w, k) in words.iter().zip(&kept) {
            if *k {
                continue;
            }
            letters += w.chars().filter(|c| c.is_alphabetic()).count();
            latin += w.chars().filter(|c| c.is_ascii_alphabetic()).count();
        }
        if letters > 0 && latin as f64 / letters as f64 > 0.5 {
            return Some(Reject::Untranslated);
        }
    }
    None
}

impl LineCheck<'_> {
    /// Проверить строку ответа; cut — строку оборвал лимит токенов (см. cut_line).
    pub fn check(&self, out: Option<&str>, cut: bool) -> Result<(), Reject> {
        let Some(out) = out else { return Err(Reject::Missing) };
        if cut {
            return Err(Reject::Cut);
        }
        if let Some(r) = untranslated_reason(self.src, out, self.tgt_lang, self.glossary) {
            return Err(r);
        }
        if let Some(g) = repeated_ngram(out) {
            if repeated_ngram(self.src).is_none() {
                return Err(Reject::Loop(g));
            }
        }
        if let Some(budget) = self.budget {
            let got = weighted_len(out);
            let base = budget.min(weighted_len(self.src).max(1));
            let min = (MIN_RATIO * base as f64).floor() as usize;
            let max = (MAX_RATIO * budget as f64).ceil() as usize;
            if got < min {
                return Err(Reject::TooShort { got, min });
            }
            if got > max {
                return Err(Reject::TooLong { got, max });
            }
        }
        if !self.rewrite {
            for e in dub_core::glossary::hits(self.glossary, self.src) {
                if let Some(req) = e.required_in_translation() {
                    if !contains_in_translation(out, req) {
                        return Err(Reject::Term(req.to_string()));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Какая строка ответа оборвана: при finish_reason=length в нумерованном ответе — последняя вернувшаяся
/// (JSON, оборванный лимитом, не разбирается вовсе).
pub(crate) fn cut_line(answer: &Answer) -> Option<usize> {
    if answer.finish_reason != "length" {
        return None;
    }
    answer.lines.iter().rposition(Option::is_some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_llm::test_http::{body_json, serve, Reply};

    fn entry(term: &str, translation: &str, keep: bool) -> GlossaryEntry {
        GlossaryEntry { term: term.into(), translation: translation.into(), keep, ..GlossaryEntry::default() }
    }

    fn check<'a>(src: &'a str, budget: Option<usize>, lang: &'a str, glossary: &'a [GlossaryEntry]) -> LineCheck<'a> {
        LineCheck { src, budget, tgt_lang: lang, glossary, rewrite: false }
    }

    #[test]
    fn the_schema_asks_for_every_number_and_nothing_else() {
        let s = schema(3);
        assert_eq!(s["required"], json!(["1", "2", "3"]));
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["properties"]["2"]["type"], "string");
    }

    #[test]
    fn the_schema_keys_go_in_the_order_of_the_lines() {
        let server = serve(vec![Reply::json(200, r#"{"choices":[{"message":{"content":"{}"},"finish_reason":"stop"}]}"#)]);
        let llm = ChatClient::new(server.base()).unwrap();
        let msgs = |_: Format| vec![Message::user_text("x")];
        Contract::for_client(&llm).ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 12, &mut |_: &Note| {}).unwrap();
        let raw = server.request(0);
        let keys: Vec<usize> = regex::Regex::new(r#""(\d+)":\{"type":"string"\}"#)
            .unwrap()
            .captures_iter(&raw)
            .map(|c| c[1].parse().unwrap())
            .collect();
        assert_eq!(keys, (1..=12).collect::<Vec<_>>());
        assert_eq!((label(Format::Json, 3, 12), label(Format::Json, 3, 9), label(Format::Numbered, 3, 12)), ("03".into(), "3".into(), "3".into()));
        assert_eq!(parse_json(r#"{"01":"Раз","2":"Два"}"#, 2).unwrap(), vec![Some("Раз".into()), Some("Два".into())], "with or without zeros");
    }

    #[test]
    fn a_json_answer_is_read_even_in_a_fence() {
        let raw = "```json\n{\"1\": \" (≤20) Привет,   мир \", \"2\": \"\", \"3\": \"Пока\"}\n```";
        let lines = parse_json(raw, 3).unwrap();
        assert_eq!(lines, vec![Some("Привет, мир".into()), None, Some("Пока".into())]);
        assert_eq!(parse_json("1. Привет", 1).unwrap_err(), AnswerProblem::NoJsonObject);
        assert!(matches!(parse_json("{\"1\": oops}", 1).unwrap_err(), AnswerProblem::NotJson(_)));
    }

    #[test]
    fn numbered_answers_keep_the_first_of_a_repeated_number() {
        let lines = parse_numbered("1. A\n1. B\n3) C\nnoise", 3);
        assert_eq!(lines, vec![Some("A".into()), None, Some("C".into())]);
    }

    #[test]
    fn each_check_refuses_its_own_fault() {
        let g: Vec<GlossaryEntry> = vec![entry("Hogwarts", "Хогвартс", false), entry("Nvidia", "", true)];
        let c = check("Welcome to Hogwarts, the school", Some(40), "ru", &g);
        assert_eq!(c.check(Some("Добро пожаловать в Хогвартс, школу"), false), Ok(()));
        assert_eq!(c.check(Some("Добро пожаловать в Хогвартсе"), false), Ok(()), "a case ending");
        assert_eq!(c.check(None, false), Err(Reject::Missing));
        assert_eq!(c.check(Some("Добро пожаловать в школу"), false), Err(Reject::Term("Хогвартс".into())));
        assert_eq!(c.check(Some("Welcome to Hogwarts, the school"), false), Err(Reject::Echo));
        assert_eq!(c.check(Some("Welcome to the magic school Hogwarts"), false), Err(Reject::Untranslated));
        assert_eq!(c.check(Some("Добро пожаловать в Хогвартс"), true), Err(Reject::Cut));
        let long = check("Welcome to Hogwarts, the best school of magic in the whole world", Some(60), "ru", &g);
        assert!(matches!(long.check(Some("Хогвартс"), false), Err(Reject::TooShort { .. })));
        assert!(matches!(c.check(Some(&"Хогвартс очень большой. ".repeat(8)), false), Err(Reject::TooLong { .. } | Reject::Loop(_))));
        assert!(matches!(c.check(Some("Добро пожаловать в Хогвартс школа школа школа"), false), Err(Reject::Loop(g)) if g == "школа"));

        let keep = check("Buy an Nvidia card", Some(30), "ru", &g);
        assert_eq!(keep.check(Some("Купи карту Nvidia"), false), Ok(()));
        assert_eq!(keep.check(Some("Купи карту Энвидиа"), false), Err(Reject::Term("Nvidia".into())));
    }

    #[test]
    fn names_numbers_and_short_replies_are_not_faults() {
        let g = [entry("Nvidia", "", true)];
        assert!(!looks_untranslated("Nvidia", "Nvidia", "ru", &g), "a kept term alone");
        assert!(!looks_untranslated("2024", "2024", "ru", &[]));
        assert!(!looks_untranslated("No", "No", "es", &[]), "one word may coincide in a Latin language");
        assert!(looks_untranslated("No", "No", "ru", &[]));
        assert!(looks_untranslated("Hello", "Hello", "ru", &[]), "a first word is not a name");
        assert!(!looks_untranslated("I met Potter", "Я встретил Potter", "ru", &[]), "a name kept from the source");
        let c = check("Yes.", Some(14), "ru", &[]);
        assert_eq!(c.check(Some("Да."), false), Ok(()), "a short reply to a short source");
        let echo = check("We love it here", Some(20), "es", &[]);
        assert_eq!(echo.check(Some("we love it here!"), false), Err(Reject::Echo), "echo is normalized");
        assert_eq!(check("ha ha ha", Some(14), "ru", &[]).check(Some("ха ха ха"), false), Ok(()), "a loop the source has");
    }

    #[test]
    fn cjk_counts_double_for_length() {
        let c = check("I don't know what you're talking about", Some(35), "zh", &[]);
        assert_eq!(c.check(Some("我不知道你在说什么"), false), Ok(()));
    }

    #[test]
    fn a_rewrite_needs_no_glossary_term() {
        let g = [entry("Hogwarts", "Хогвартс", false)];
        let c = LineCheck { src: "Welcome to Hogwarts", budget: Some(30), tgt_lang: "ru", glossary: &g, rewrite: true };
        assert_eq!(c.check(Some("Пицца уже в пути"), false), Ok(()));
    }

    #[test]
    fn the_last_returned_line_of_a_cut_answer_is_the_cut_one() {
        let a = Answer { lines: vec![Some("a".into()), Some("b".into()), None], finish_reason: "length".into() };
        assert_eq!(cut_line(&a), Some(1));
        let ok = Answer { lines: vec![Some("a".into())], finish_reason: "stop".into() };
        assert_eq!(cut_line(&ok), None);
    }

    #[test]
    fn the_format_follows_the_model_and_a_refusal_switches_it_once() {
        let msgs = |f: Format| vec![Message::user_text(if f == Format::Json { "json" } else { "numbered" })];
        let mut log: Vec<String> = vec![];
        let server = serve(vec![
            Reply::json(400, r#"{"error":"unsupported response_format"}"#),
            Reply::json(200, r#"{"choices":[{"message":{"content":"1. Привет"},"finish_reason":"stop"}]}"#),
            Reply::json(200, r#"{"choices":[{"message":{"content":"1. Пока"},"finish_reason":"stop"}]}"#),
        ]);
        let llm = ChatClient::openai_compatible(&server.base(), "m", None).unwrap();
        let c = Contract::for_client(&llm);
        c.announce(&llm, &mut |m: &Note| log.push(m.to_string()));
        c.announce(&llm, &mut |m: &Note| log.push(m.to_string()));
        assert_eq!(log.len(), 1, "one line per job");
        let a = c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |m: &Note| log.push(m.to_string())).unwrap();
        assert_eq!(a.lines, vec![Some("Привет".into())]);
        assert_eq!(c.format(), Format::Numbered);
        assert!(log[1].contains("400"), "{log:?}");
        assert!(body_json(&server.request(0)).get("response_format").is_some());
        assert!(body_json(&server.request(1)).get("response_format").is_none());
        c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).unwrap();
        assert!(body_json(&server.request(2)).get("response_format").is_none(), "stays numbered");

        let own = serve(vec![Reply::json(200, r#"{"choices":[{"message":{"content":"{\"1\":\"Hola\"}"},"finish_reason":"stop"}]}"#)]);
        let llm = ChatClient::new(own.base()).unwrap();
        let c = Contract::for_client(&llm);
        assert_eq!(c.format(), Format::Json);
        let a = c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).unwrap();
        assert_eq!(a.lines, vec![Some("Hola".into())]);
        assert_eq!(body_json(&own.request(0))["response_format"]["json_schema"]["schema"]["required"], json!(["1"]));

        let cloud = serve(vec![
            Reply::json(404, r#"{"error":{"message":"No endpoints found that can handle the requested parameters."}}"#),
            Reply::json(200, r#"{"choices":[{"message":{"content":"1. Hola"},"finish_reason":"stop"}]}"#),
        ]);
        let profile = dub_llm::openrouter::ModelProfile { supported_parameters: vec!["structured_outputs".into()], ..Default::default() };
        let llm = ChatClient::openrouter_at(&cloud.base(), "k", "vendor/m").unwrap().with_profile(Some(profile.clone()));
        let c = Contract::for_client(&llm);
        let mut log: Vec<String> = vec![];
        let a = c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |m: &Note| log.push(m.to_string())).unwrap();
        assert_eq!(a.lines, vec![Some("Hola".into())]);
        assert_eq!(c.format(), Format::Numbered);
        assert!(log.len() == 1 && log[0].contains("404") && log[0].contains("vendor/m"), "{log:?}");
        assert!(body_json(&cloud.request(1)).get("response_format").is_none());

        let other = serve(vec![Reply::json(400, r#"{"error":{"message":"maximum context length exceeded"}}"#)]);
        let llm = ChatClient::openrouter_at(&other.base(), "k", "vendor/m").unwrap().with_profile(Some(profile));
        let c = Contract::for_client(&llm);
        assert!(c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).is_err(), "not about the schema");
        assert_eq!(c.format(), Format::Json);

        let bad = serve(vec![Reply::json(400, r#"{"error":"bad"}"#)]);
        let llm = ChatClient::new(bad.base()).unwrap();
        let c = Contract::for_client(&llm);
        assert!(c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).is_err(), "llama-server's refusal is an error");
    }

    #[test]
    fn a_server_taking_the_schema_but_answering_numbered_lines_switches_while_probing() {
        let msgs = |f: Format| vec![Message::user_text(if f == Format::Json { "json" } else { "numbered" })];
        let server = serve(vec![
            Reply::json(200, r#"{"choices":[{"message":{"content":"1. Привет\n2. Мир"},"finish_reason":"stop"}]}"#),
            Reply::json(200, r#"{"choices":[{"message":{"content":"1. Пока"},"finish_reason":"stop"}]}"#),
        ]);
        let llm = ChatClient::openai_compatible(&server.base(), "m", None).unwrap();
        let c = Contract::for_client(&llm);
        let mut log: Vec<String> = vec![];
        let a = c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 2, &mut |m: &Note| log.push(m.to_string())).unwrap();
        assert_eq!(a.lines, vec![Some("Привет".into()), Some("Мир".into())]);
        assert_eq!(c.format(), Format::Numbered);
        assert!(log.len() == 1 && log[0].contains("numbered lines"), "{log:?}");
        c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).unwrap();
        assert!(body_json(&server.request(1)).get("response_format").is_none());

        let proven = serve(vec![
            Reply::json(200, r#"{"choices":[{"message":{"content":"{\"1\":\"Hola\"}"},"finish_reason":"stop"}]}"#),
            Reply::json(200, r#"{"choices":[{"message":{"content":"1. Hola"},"finish_reason":"stop"}]}"#),
        ]);
        let llm = ChatClient::openai_compatible(&proven.base(), "m", None).unwrap();
        let c = Contract::for_client(&llm);
        c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).unwrap();
        assert!(c.ask(&llm, &msgs, &Sampling::new(0.2, 0.9, 50), 1, &mut |_: &Note| {}).is_err(), "after the probe a broken answer is an error");
        assert_eq!(c.format(), Format::Json);
    }
}
