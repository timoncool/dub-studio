//! «Собрать из текста»: кандидаты в глоссарий из транскрипта — проход модели «имена, термины, бренды с
//! переводом» (JSON по схеме, где модель её держит) и повторяющиеся имена glossary_pairs без потолка.
//! Записи source:auto с языком цели; их подтверждает человек.

use dub_core::glossary::{normalize, GlossaryEntry, GlossarySource};
use dub_llm::{strip_think, ChatClient, LlmError, Message, Sampling, StructuredOutput};
use serde_json::{json, Value};

use crate::{AnswerProblem, Note, TranslateError};

/// Сколько символов транскрипта в одном запросе прохода модели.
const PART_CHARS: usize = 6000;

/// Ключи записи идут по алфавиту в порядке, в котором их пишет модель (сериализация сортирует ключи):
/// сначала термин, потом перевод.
fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "terms": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "term": { "type": "string" },
                        "translation": { "type": "string" },
                        "verbatim": { "type": "boolean" },
                        "what": { "type": "string" }
                    },
                    "required": ["term", "translation", "verbatim", "what"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["terms"],
        "additionalProperties": false
    })
}

/// Части транскрипта по PART_CHARS символов (строка целиком в одной части).
fn parts(texts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for t in texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !cur.is_empty() && cur.chars().count() + t.chars().count() > PART_CHARS {
            out.push(std::mem::take(&mut cur));
        }
        cur.push_str(t);
        cur.push('\n');
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Ответ прохода -> записи. Объект ищется между первой «{» и последней «}».
fn parse_terms(raw: &str) -> Result<Vec<GlossaryEntry>, AnswerProblem> {
    let (Some(a), Some(b)) = (raw.find('{'), raw.rfind('}')) else {
        return Err(AnswerProblem::NoJsonObject);
    };
    if b < a {
        return Err(AnswerProblem::NoJsonObject);
    }
    let v: Value = serde_json::from_str(&raw[a..=b]).map_err(|e| AnswerProblem::NotJson(e.to_string()))?;
    let items = v.get("terms").and_then(Value::as_array).ok_or(AnswerProblem::NoTerms)?;
    Ok(items
        .iter()
        .filter_map(|it| {
            let term = it.get("term")?.as_str()?.trim().to_string();
            if term.is_empty() {
                return None;
            }
            let text = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
            Some(GlossaryEntry {
                translation: text("translation"),
                keep: it.get("verbatim").and_then(Value::as_bool).unwrap_or(false),
                note: text("what"),
                term,
                ..GlossaryEntry::default()
            })
        })
        .collect())
}

/// Кандидаты в глоссарий из строк транскрипта. `known` — уже записанные термины: их не предлагаем.
pub fn extract_glossary(
    llm: &ChatClient,
    texts: &[String],
    src_lang: &str,
    tgt_lang: &str,
    known: &[GlossaryEntry],
    log: &mut dyn FnMut(&Note),
) -> Result<Vec<GlossaryEntry>, TranslateError> {
    let tgt = crate::translate::lang_name(tgt_lang, tgt_lang);
    let mut with_schema = llm.structured_output() != StructuredOutput::Unsupported;
    let mut found: Vec<GlossaryEntry> = Vec::new();
    let all = parts(texts);
    for (k, part) in all.iter().enumerate() {
        log(&Note::GlossaryPass { pass: k + 1, passes: all.len() });
        let prompt = format!(
            "Below is a video transcript that will be dubbed into {tgt}. List the proper names (people, places, \
organisations), recurring special terms, brands and titles a dubbing translator must render the same way every time. \
For each give its {tgt} rendering in `translation`, or set `verbatim` true when it must stay exactly as written \
(brands, product names); `what` says briefly what it is. Leave out ordinary words. Reply with ONLY a JSON object \
{{\"terms\": [{{\"term\": …, \"translation\": …, \"verbatim\": …, \"what\": …}}]}}.\n\n=== TRANSCRIPT ===\n{part}"
        );
        let s = Sampling::new(0.2, 0.9, 1536);
        let messages = [Message::user_text(prompt)];
        let schema = schema();
        let done = match llm.complete(&messages, &s, with_schema.then_some(&schema)) {
            Err(LlmError::Rejected { code, status, body })
                if with_schema && crate::contract::schema_refused(llm, code, &body, llm.structured_output() == StructuredOutput::Untested) =>
            {
                log(&Note::GlossarySchemaRefused { status: &status });
                with_schema = false;
                llm.complete(&messages, &s, None)?
            }
            other => other?,
        };
        let terms = parse_terms(&strip_think(&done.text)).map_err(|problem| TranslateError::Contract { problem, answer: None })?;
        for t in terms {
            if !found.iter().any(|f| normalize(&f.term) == normalize(&t.term)) {
                found.push(t);
            }
        }
    }
    // Повторяющиеся имена, которых проход не назвал: glossary_pairs без потолка «6 самых частых».
    let mut seen = known.to_vec();
    seen.extend(found.iter().cloned());
    let pairs = crate::translate::glossary_pairs(llm, texts.iter().map(String::as_str), &crate::translate::name_src(src_lang), &tgt, None, &seen)?;
    found.extend(pairs.into_iter().map(|(term, translation)| GlossaryEntry { term, translation, ..GlossaryEntry::default() }));
    let known_terms: Vec<String> = known.iter().map(|e| normalize(&e.term)).collect();
    Ok(found
        .into_iter()
        .filter(|e| !known_terms.contains(&normalize(&e.term)))
        .map(|mut e| {
            e.source = GlossarySource::Auto;
            e.lang = tgt_lang.to_string();
            e
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_llm::test_http::{body_json, serve, Reply};

    fn reply(content: &str) -> Reply {
        Reply::json(200, &json!({ "choices": [{ "message": { "content": content }, "finish_reason": "stop" }] }).to_string())
    }

    #[test]
    fn candidates_come_from_the_model_and_the_repeated_names_minus_what_is_known() {
        let server = serve(vec![
            reply(r#"{"terms":[{"term":"Hogwarts","translation":"Хогвартс","verbatim":false,"what":"school"},{"term":"Nimbus","translation":"","verbatim":true,"what":"brand"},{"term":"Harry","translation":"Гарри","verbatim":false,"what":""}]}"#),
            reply("Рон"),
        ]);
        let llm = ChatClient::new(server.base()).unwrap();
        let texts: Vec<String> = ["Harry and Ron", "Ron at Hogwarts", "Ron, the Nimbus!"].iter().map(|s| s.to_string()).collect();
        let known = [GlossaryEntry { term: "Harry".into(), ..GlossaryEntry::default() }];
        let got = extract_glossary(&llm, &texts, "en", "ru", &known, &mut |_: &Note| {}).unwrap();
        let terms: Vec<&str> = got.iter().map(|e| e.term.as_str()).collect();
        assert_eq!(terms, vec!["Hogwarts", "Nimbus", "Ron"]);
        assert!(got.iter().all(|e| e.source == GlossarySource::Auto && e.lang == "ru"));
        assert!(got[1].keep);
        assert_eq!((got[0].note.as_str(), got[1].note.as_str()), ("school", "brand"));
        assert_eq!(got[2].translation, "Рон");
        assert_eq!(body_json(&server.request(0))["response_format"]["json_schema"]["schema"]["required"], json!(["terms"]));
        let raw = server.request(0);
        let at = |k: &str| raw.find(&format!(r#""{k}":{{"type""#)).unwrap_or_else(|| panic!("{k} in {raw}"));
        assert!(at("term") < at("translation") && at("translation") < at("verbatim") && at("verbatim") < at("what"), "{raw}");
    }

    #[test]
    fn a_long_transcript_goes_in_parts() {
        let texts: Vec<String> = (0..300).map(|i| format!("line number {i} with some words in it")).collect();
        let p = parts(&texts);
        assert!(p.len() > 1);
        assert!(p.iter().all(|x| x.chars().count() <= PART_CHARS + 60));
    }

    #[test]
    fn an_answer_without_terms_is_an_error() {
        assert!(parse_terms("no json").is_err());
        assert_eq!(parse_terms(r#"{"x":1}"#).unwrap_err(), AnswerProblem::NoTerms);
    }
}
