//! Прогон перевода пакетами с проверкой каждой строки: не прошедшие строки — один повтор меньшим пакетом,
//! затем деление пополам до одной строки, затем итог. Строку с мягким изъяном (длина, повтор, термин)
//! после всех попыток оставляем с записью в журнал; жёсткий отказ (нет строки, не тот язык, эхо, обрыв)
//! оставляет строку без перевода — решение о провале стадии принимает вызывающий. Сбой запроса, который
//! меньший пакет не лечит (сеть, отказ в доступе), останавливает прогон с исходной ошибкой.

use std::collections::HashMap;

use dub_llm::LlmError;

use crate::contract::{cut_line, Answer, Reject};
use crate::{LineFailure, Note, TranslateError};

/// Сбой зависит от размера пакета, меньший пакет может пройти: ответ не по контракту, обрезанный промпт или
/// ответ, отказ 400/413/422 (запрос не влез в контекст модели). Сеть, 5xx и 429 после повторов, 401/402/403/404,
/// пустой ответ модели делением не лечатся.
fn size_bound(e: &TranslateError) -> bool {
    match e {
        TranslateError::Contract { .. } => true,
        TranslateError::Llm(LlmError::PromptCut { .. } | LlmError::CutShort { .. }) => true,
        TranslateError::Llm(LlmError::Rejected { code, .. }) => matches!(code, 400 | 413 | 422),
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    First,
    Retry,
    Split,
}

/// Итог: принятые строки (ключ — индекс вызывающего), строки без перевода с причиной и строки,
/// принятые с изъяном.
#[derive(Default)]
pub(crate) struct Outcome {
    pub accepted: HashMap<usize, String>,
    pub failed: Vec<(usize, Option<LineFailure>)>,
    pub flawed: Vec<(usize, Reject)>,
}

fn halves(v: &[usize]) -> [Vec<usize>; 2] {
    let mid = v.len() / 2;
    [v[..mid].to_vec(), v[mid..].to_vec()]
}

/// Спросить модель о строках idx (ответ по порядку idx); второй аргумент — уже принятые строки.
pub(crate) type Ask<'a> = dyn FnMut(&[usize], &HashMap<usize, String>) -> Result<Answer, TranslateError> + 'a;
/// Проверить строку i ответа; третий аргумент — строку оборвал лимит токенов.
pub(crate) type Check<'a> = dyn Fn(usize, Option<&str>, bool) -> Result<(), Reject> + 'a;

/// `label(i)` — номер строки для журнала. Ошибка — сбой запроса, который не зависит от размера пакета.
pub(crate) fn drive(
    chunks: Vec<Vec<usize>>,
    ask: &mut Ask,
    check: &Check,
    label: &dyn Fn(usize) -> usize,
    log: &mut dyn FnMut(&Note),
) -> Result<Outcome, TranslateError> {
    let mut out = Outcome::default();
    let mut best: HashMap<usize, (String, Reject)> = HashMap::new();
    let mut last_hard: HashMap<usize, LineFailure> = HashMap::new();
    let mut stack: Vec<(Vec<usize>, Pass)> = chunks.into_iter().rev().filter(|c| !c.is_empty()).map(|c| (c, Pass::First)).collect();

    let finalize = |i: usize, out: &mut Outcome, best: &mut HashMap<usize, (String, Reject)>, why: Option<LineFailure>, log: &mut dyn FnMut(&Note)| {
        if let Some((line, r)) = best.remove(&i) {
            log(&Note::LineFlawed { line: label(i), reason: &r });
            out.flawed.push((i, r));
            out.accepted.insert(i, line);
        } else {
            if let Some(why) = &why {
                log(&Note::LineFailed { line: label(i), reason: why });
            }
            out.failed.push((i, why));
        }
    };

    while let Some((idx, pass)) = stack.pop() {
        let bad: Vec<usize> = match ask(&idx, &out.accepted) {
            Ok(answer) => {
                let cut = cut_line(&answer);
                let mut bad = Vec::new();
                for (k, &i) in idx.iter().enumerate() {
                    let line = answer.lines.get(k).and_then(|l| l.as_deref());
                    match check(i, line, cut == Some(k)) {
                        Ok(()) => {
                            best.remove(&i);
                            out.accepted.insert(i, line.unwrap_or_default().to_string());
                        }
                        Err(r) => {
                            if r.hard() {
                                last_hard.insert(i, LineFailure::Rejected(r));
                            } else if let Some(line) = line {
                                best.insert(i, (line.to_string(), r));
                            }
                            bad.push(i);
                        }
                    }
                }
                if !bad.is_empty() {
                    let reasons: Vec<(usize, LineFailure)> = bad
                        .iter()
                        .filter_map(|i| {
                            let why = best.get(i).map(|b| LineFailure::Rejected(b.1.clone())).or_else(|| last_hard.get(i).cloned())?;
                            Some((label(*i), why))
                        })
                        .collect();
                    log(&Note::LinesRejected { bad: bad.len(), total: idx.len(), reasons: &reasons });
                }
                bad
            }
            Err(e) if !size_bound(&e) => {
                log(&Note::BatchStopped { first: label(idx[0]), last: label(idx[idx.len() - 1]), error: &e });
                return Err(e);
            }
            Err(e) => {
                log(&Note::BatchFailed { first: label(idx[0]), last: label(idx[idx.len() - 1]), error: &e });
                let why = LineFailure::Error(Box::new(e));
                for &i in &idx {
                    last_hard.insert(i, why.clone());
                }
                idx.clone()
            }
        };
        if bad.is_empty() {
            continue;
        }
        if idx.len() == 1 {
            if pass == Pass::First {
                stack.push((bad, Pass::Retry));
            } else {
                let why = last_hard.get(&bad[0]).cloned();
                finalize(bad[0], &mut out, &mut best, why, log);
            }
        } else if pass == Pass::First && bad.len() < idx.len() {
            stack.push((bad, Pass::Retry));
        } else {
            for half in halves(&bad).into_iter().rev().filter(|h| !h.is_empty()) {
                stack.push((half, Pass::Split));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnswerProblem;

    fn answer(lines: &[Option<&str>]) -> Answer {
        Answer { lines: lines.iter().map(|l| l.map(str::to_string)).collect(), finish_reason: "stop".into() }
    }

    /// Модель по сценарию: строка i отвечает `good`, пока не исчерпана её квота плохих ответов.
    fn run(bad_answers: HashMap<usize, (usize, &'static str)>, n: usize, chunk: usize) -> (Outcome, Vec<Vec<usize>>) {
        let calls = std::cell::RefCell::new(Vec::new());
        let left = std::cell::RefCell::new(bad_answers);
        let mut ask = |idx: &[usize], _: &HashMap<usize, String>| -> Result<Answer, TranslateError> {
            calls.borrow_mut().push(idx.to_vec());
            let lines: Vec<Option<&str>> = idx
                .iter()
                .map(|i| match left.borrow_mut().get_mut(i) {
                    Some((k, bad)) if *k > 0 => {
                        *k -= 1;
                        Some(*bad)
                    }
                    _ => Some("хорошо"),
                })
                .collect();
            Ok(answer(&lines))
        };
        let check = |_: usize, line: Option<&str>, _: bool| -> Result<(), Reject> {
            match line {
                Some("хорошо") => Ok(()),
                Some("длинно") => Err(Reject::TooLong { got: 99, max: 10 }),
                _ => Err(Reject::Untranslated),
            }
        };
        let chunks: Vec<Vec<usize>> = (0..n).collect::<Vec<_>>().chunks(chunk).map(<[usize]>::to_vec).collect();
        let out = drive(chunks, &mut ask, &check, &|i| i + 1, &mut |_: &Note| {}).unwrap();
        (out, calls.into_inner())
    }

    #[test]
    fn failed_lines_are_asked_again_in_a_smaller_batch() {
        let (out, calls) = run(HashMap::from([(1, (1, "english"))]), 4, 4);
        assert_eq!(out.accepted.len(), 4);
        assert_eq!(calls, vec![vec![0, 1, 2, 3], vec![1]]);
    }

    #[test]
    fn a_stubborn_line_is_split_down_then_left_untranslated() {
        let (out, calls) = run(HashMap::from([(1, (9, "english")), (2, (9, "english"))]), 4, 4);
        assert_eq!(out.accepted.len(), 2);
        assert_eq!(out.failed.iter().map(|f| f.0).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(calls, vec![vec![0, 1, 2, 3], vec![1, 2], vec![1], vec![2]]);
    }

    #[test]
    fn a_flawed_translation_is_kept_after_the_last_try() {
        let (out, _) = run(HashMap::from([(0, (9, "длинно"))]), 2, 2);
        assert_eq!(out.accepted[&0], "длинно");
        assert_eq!(out.flawed.len(), 1);
        assert!(out.failed.is_empty());
    }

    #[test]
    fn a_whole_bad_chunk_is_halved_at_once() {
        let (out, calls) = run(HashMap::from([(0, (1, "x")), (1, (1, "x")), (2, (1, "x")), (3, (1, "x"))]), 4, 4);
        assert_eq!(out.accepted.len(), 4);
        assert_eq!(calls, vec![vec![0, 1, 2, 3], vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn a_failing_call_is_halved_down_to_single_lines() {
        let calls = std::cell::RefCell::new(0usize);
        let mut ask = |idx: &[usize], _: &HashMap<usize, String>| -> Result<Answer, TranslateError> {
            *calls.borrow_mut() += 1;
            if idx.len() > 1 {
                return Err(TranslateError::Contract { problem: AnswerProblem::NoJsonObject, answer: Some("не влезло".into()) });
            }
            Err(TranslateError::Contract { problem: AnswerProblem::NoJsonObject, answer: Some("нет".into()) })
        };
        let out = drive(vec![vec![0, 1]], &mut ask, &|_, _, _| Ok(()), &|i| i + 1, &mut |_: &Note| {}).unwrap();
        assert_eq!(out.failed.len(), 2);
        assert!(out.failed[0].1.as_ref().is_some_and(|why| why.to_string().contains("нет")));
        assert_eq!(*calls.borrow(), 3, "the pair, then each line alone");
        let once = std::cell::RefCell::new(0usize);
        let mut ask = |_: &[usize], _: &HashMap<usize, String>| -> Result<Answer, TranslateError> {
            *once.borrow_mut() += 1;
            Err(TranslateError::Contract { problem: AnswerProblem::NoJsonObject, answer: Some("нет".into()) })
        };
        drive(vec![vec![0]], &mut ask, &|_, _, _| Ok(()), &|i| i + 1, &mut |_: &Note| {}).unwrap();
        assert_eq!(*once.borrow(), 2, "a one-line chunk gets one more try");
        let too_big = std::cell::RefCell::new(0usize);
        let mut ask = |idx: &[usize], _: &HashMap<usize, String>| -> Result<Answer, TranslateError> {
            *too_big.borrow_mut() += 1;
            if idx.len() > 1 {
                return Err(LlmError::Rejected { code: 400, status: "400".into(), body: "exceeds the context size".into() }.into());
            }
            Ok(answer(&[Some("хорошо")]))
        };
        let out = drive(vec![vec![0, 1]], &mut ask, &|_, _, _| Ok(()), &|i| i + 1, &mut |_: &Note| {}).unwrap();
        assert_eq!((out.accepted.len(), *too_big.borrow()), (2, 3), "a request too big for the context is halved");
    }

    #[test]
    fn a_failure_no_smaller_batch_cures_stops_the_run_with_its_cause() {
        for error in [
            LlmError::Rejected { code: 401, status: "401 Unauthorized".into(), body: "invalid key".into() },
            LlmError::Api("chat failed after 3 retries: connection refused".into()),
            LlmError::Http("error sending request".into()),
        ] {
            let text = error.to_string();
            let calls = std::cell::RefCell::new(0usize);
            let error = std::cell::RefCell::new(Some(error));
            let mut ask = |_: &[usize], _: &HashMap<usize, String>| -> Result<Answer, TranslateError> {
                *calls.borrow_mut() += 1;
                Err(error.borrow_mut().take().expect("asked once").into())
            };
            let got = drive(vec![vec![0, 1, 2, 3], vec![4]], &mut ask, &|_, _, _| Ok(()), &|i| i + 1, &mut |_: &Note| {});
            assert_eq!(*calls.borrow(), 1);
            assert_eq!(got.err().map(|e| e.to_string()), Some(format!("llm: {text}")));
        }
    }
}
