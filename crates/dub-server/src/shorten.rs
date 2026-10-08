//! Сокращение перевода под слот: LLM перевода переписывает реплику короче (исходник, текущий перевод и
//! по две соседние реплики в контексте, лимит символов из слота и темпа голоса), ответ проходит проверки
//! (алфавит целевого языка, строго короче, не эхо исходника), прошедший становится новым tgt_text с
//! отметкой `shortened` {from, to}. Два входа: джоба POST /projects/{pid}/shorten (кнопки редактора и
//! агент) и замкнутый цикл рендера (фразы, не влезшие в слот после озвучки).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use audiocpp::AudiocppEngine;
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use dub_core::fit;
use dub_core::Project;
use dub_llm::{strip_think, ChatClient, Message, Sampling};
use serde_json::{json, Value};

use crate::fitplan::{self, Calibration};
use crate::{jobs, AppState};

/// Реплика к сокращению.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub src: String,
    pub tgt: String,
    /// Слот, в который надо уложиться, сек.
    pub target: f64,
    pub limit: usize,
    /// Соседние реплики (исходник, перевод): по две до и после.
    pub before: Vec<(String, String)>,
    pub after: Vec<(String, String)>,
}

/// Сокращённая реплика.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub id: String,
    pub from: String,
    pub to: String,
}

/// Почему ответ LLM не принят.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    Empty,
    Echo,
    Alphabet,
    NotShorter,
}

impl Reject {
    pub fn code(self) -> &'static str {
        match self {
            Reject::Empty => "empty",
            Reject::Echo => "echo",
            Reject::Alphabet => "alphabet",
            Reject::NotShorter => "not_shorter",
        }
    }
}

/// Сколько соседних реплик с каждой стороны идут в контекст.
const CONTEXT: usize = 2;
const TEMPERATURE: f32 = 0.15;

/// Лимит длины: влезть в слот при темпе голоса, и в любом случае короче текущей строки.
pub fn limit_for(tgt: &str, target: f64, cps: f64) -> usize {
    fit::shorten_limit(target, cps).min(fit::text_units(tgt).saturating_sub(1)).max(1)
}

/// Реплики к сокращению: (индекс в проекте, слот, темп голоса).
pub fn items(proj: &Project, picks: &[(usize, f64, f64)]) -> Vec<Item> {
    let voiced: Vec<usize> = (0..proj.segments.len()).filter(|&i| fitplan::voiced(&proj.segments[i])).collect();
    let pair = |i: usize| (proj.segments[i].src_text.trim().to_string(), proj.segments[i].tgt_text.trim().to_string());
    picks
        .iter()
        .map(|&(i, target, cps)| {
            let s = &proj.segments[i];
            let pos = voiced.partition_point(|&v| v < i);
            let before = voiced[pos.saturating_sub(CONTEXT)..pos].iter().map(|&v| pair(v)).collect();
            let after_from = if voiced.get(pos) == Some(&i) { pos + 1 } else { pos };
            let after = voiced[after_from.min(voiced.len())..(after_from + CONTEXT).min(voiced.len())].iter().map(|&v| pair(v)).collect();
            Item {
                id: s.id.clone(),
                src: s.src_text.trim().to_string(),
                tgt: s.tgt_text.trim().to_string(),
                target,
                limit: limit_for(&s.tgt_text, target, cps),
                before,
                after,
            }
        })
        .collect()
}

fn lang_name(code: &str) -> String {
    let lc = code.trim().to_lowercase();
    dub_translate::WHISPER_LANGS.iter().find(|(k, _)| *k == lc).map(|(_, v)| v.to_string()).unwrap_or_else(|| code.to_string())
}

/// Сообщения запроса одной реплики.
pub fn prompt(lang: &str, style: &str, item: &Item) -> Vec<Message> {
    let name = lang_name(lang);
    let style = style.trim();
    let style = if style.is_empty() { String::new() } else { format!(" Keep this translation style: {style}.") };
    let system = format!(
        "You are a dubbing script editor. Rewrite ONE translated line shorter so the same voice can say it within its \
         time slot. Keep the meaning, every name, number and term, the tone and the language ({name}). Drop filler \
         words, repetitions and what the picture or the neighbouring lines already say; do not add anything.{style} \
         Reply with the shortened {name} line only, without quotes, numbering or comments."
    );
    let ctx = |rows: &[(String, String)]| -> String {
        if rows.is_empty() {
            return "- (none)".to_string();
        }
        rows.iter().map(|(s, t)| format!("- {s} => {t}")).collect::<Vec<_>>().join("\n")
    };
    let user = format!(
        "Time slot: {:.2} s\nLimit: at most {} characters, spaces included\nSource line: {}\nCurrent translation ({} characters): {}\n\
         Previous lines (context, keep as they are):\n{}\nNext lines (context, keep as they are):\n{}",
        item.target,
        item.limit,
        item.src,
        fit::text_units(&item.tgt),
        item.tgt,
        ctx(&item.before),
        ctx(&item.after),
    );
    vec![Message::system(system), Message::user_text(user)]
}

/// Строка без ведущей нумерации «1.» / «1)».
fn without_number(s: &str) -> &str {
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    let rest = &s[digits..];
    match rest.strip_prefix(['.', ')']) {
        Some(after) if digits > 0 && after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => s,
    }
}

/// Строка без ведущего маркера лимита «(≤NN)», если модель его повторила.
fn without_limit_marker(s: &str) -> &str {
    let Some(inner) = s.strip_prefix('(') else { return s };
    let Some(close) = inner.find(')') else { return s };
    let mark = inner[..close].trim();
    let num = ["\u{2264}", "<=", "=<"].iter().find_map(|m| mark.strip_prefix(m)).map(str::trim);
    match num {
        Some(n) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => inner[close + 1..].trim_start(),
        _ => s,
    }
}

/// Первая непустая строка ответа без кавычек, нумерации и маркера лимита.
fn clean(answer: &str) -> String {
    let text = strip_think(answer);
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let line = without_limit_marker(without_number(line));
    let pairs = [('"', '"'), ('«', '»'), ('“', '”'), ('„', '“'), ('\'', '\''), ('「', '」')];
    let mut s = line.trim().to_string();
    for (a, b) in pairs {
        if s.chars().count() >= 2 && s.starts_with(a) && s.ends_with(b) {
            s = s[a.len_utf8()..s.len() - b.len_utf8()].trim().to_string();
        }
    }
    s
}

fn letters_only(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Доля латиницы среди букв текста (None — букв нет).
fn latin_share(text: &str) -> Option<f64> {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return None;
    }
    let latin = letters.iter().filter(|c| matches!(**c as u32, 0..=0x24F | 0x1E00..=0x1EFF)).count();
    Some(latin as f64 / letters.len() as f64)
}

/// Написано ли письмом целевого языка: у языка с нелатинским письмом (по коду языка или по самому
/// текущему переводу) латиница — не больше половины букв, у латинского — не меньше половины.
fn script_ok(text: &str, cur: &str, lang: &str) -> bool {
    let Some(latin) = latin_share(text) else { return true };
    let non_latin = dub_translate::tgt_expects_non_latin(lang) || latin_share(cur).is_some_and(|l| l < 0.5);
    if non_latin {
        latin <= 0.5
    } else {
        latin >= 0.5
    }
}

/// Проверить ответ: не пустой, не эхо исходника, письмом целевого языка, строго короче текущего.
pub fn check(src: &str, cur: &str, answer: &str, lang: &str) -> Result<String, Reject> {
    let cand = clean(answer);
    if cand.is_empty() {
        return Err(Reject::Empty);
    }
    let src_n = letters_only(src);
    if !src_n.is_empty() && letters_only(&cand) == src_n && letters_only(cur) != src_n {
        return Err(Reject::Echo);
    }
    if !script_ok(&cand, cur, lang) {
        return Err(Reject::Alphabet);
    }
    if fit::text_units(&cand) >= fit::text_units(cur) {
        return Err(Reject::NotShorter);
    }
    Ok(cand)
}

/// Итог прохода по репликам.
#[derive(Debug, Default)]
pub struct Outcome {
    pub done: Vec<Change>,
    pub rejected: Vec<(String, Reject)>,
    pub failed: Vec<(String, String)>,
}

/// Попросить LLM сократить каждую реплику. Ошибка вызова одной реплики не рвёт остальные.
pub fn run(client: &ChatClient, lang: &str, style: &str, items: &[Item], log: &dyn Fn(String)) -> Result<Outcome, String> {
    let mut out = Outcome::default();
    let s = Sampling::new(TEMPERATURE, 0.9, 256);
    for (k, item) in items.iter().enumerate() {
        jobs::check_cancelled()?;
        match client.chat(&prompt(lang, style, item), &s) {
            Ok(answer) => match check(&item.src, &item.tgt, &answer, lang) {
                Ok(to) => {
                    log(t!("shorten-line-done", n = k + 1, total = items.len(), from = fit::text_units(&item.tgt), to = fit::text_units(&to)));
                    out.done.push(Change { id: item.id.clone(), from: item.tgt.clone(), to });
                }
                Err(r) => {
                    log(t!("shorten-line-rejected", n = k + 1, total = items.len(), reason = r.code()));
                    out.rejected.push((item.id.clone(), r));
                }
            },
            Err(e) => {
                log(t!("shorten-line-no-answer", n = k + 1, total = items.len(), error = e.to_string()));
                out.failed.push((item.id.clone(), e.to_string()));
            }
        }
    }
    Ok(out)
}

/// Отметка реплики о сокращении.
pub fn mark(seg: &mut dub_core::Segment, c: &Change) {
    seg.tgt_text = c.to.clone();
    seg.dirty = true;
    seg.extra.insert("shortened".into(), json!({ "from": c.from, "to": c.to }));
}

/// Сокращена ли реплика уже до её текущего текста (цикл рендера сокращает фразу один раз).
pub fn already_shortened(seg: &dub_core::Segment) -> bool {
    seg.extra.get("shortened").and_then(|v| v.get("to")).and_then(Value::as_str) == Some(seg.tgt_text.trim())
}

/// Записать сокращения в project.json каталога: только в реплики, чей текст всё ещё тот, что сокращали
/// (правки, пришедшие за время джобы, не затираются). Закрепление дубля с прежним текстом снимается.
/// Возвращает применённые сокращения и id реплик, с которых снято закрепление.
pub fn persist(dir: &Path, changes: &[Change]) -> Result<(Vec<Change>, Vec<String>), String> {
    let path = dir.join("project.json");
    let held = crate::project_writes();
    let text = std::fs::read_to_string(&path).map_err(|e| t!("common-read", path = path.display().to_string(), error = e.to_string()))?;
    let mut proj = Project::from_json(&text).map_err(|e| t!("common-parse", path = path.display().to_string(), error = e.to_string()))?;
    let mut applied = Vec::new();
    for c in changes {
        if let Some(seg) = proj.segments.iter_mut().find(|s| s.id == c.id && s.tgt_text.trim() == c.from.trim()) {
            mark(seg, c);
            applied.push(c.clone());
        }
    }
    if applied.is_empty() {
        return Ok((applied, Vec::new()));
    }
    crate::write_project(dir, &proj)?;
    drop(held);
    let mut unpinned = Vec::new();
    for c in &applied {
        if let Some(sid) = crate::render::seg_file_id(&c.id) {
            if crate::takes::unpin_if_stale(dir, &sid, &c.to)? {
                unpinned.push(c.id.clone());
            }
        }
    }
    Ok((applied, unpinned))
}

fn open_llm(llama_bin: &Path, mt_model: &Path, models_root: &Path) -> Result<crate::llm_provider::LlmProvider, String> {
    crate::llm_provider::open(
        &crate::llm_provider::LlmOpen { llama_bin, mt_model, mmproj: Path::new(""), models_root },
        crate::llm_provider::LlmMode::Text,
    )
}

// ---------------------------------------------------------------- замкнутый цикл рендера

/// Что первый проход озвучки передаёт циклу сокращения.
#[derive(Debug, Default)]
pub struct Overflow {
    /// Не влезшие фразы: (id реплики, слот рендера).
    pub picks: Vec<(String, f64)>,
    /// Клипы этого прохода для калибровки темпа: (спикер, text_units, сырые секунды).
    pub samples: Vec<(String, usize, f64)>,
    /// Сегменты, где синтез провалился и стоит оригинал: второй проход их не синтезирует снова.
    pub kept: HashSet<String>,
}

/// Переписать не влезшие фразы. Своя Gemma или локальный сервер делят видеокарту с Higgs, поэтому
/// Higgs выгружается на время LLM (второй проход загрузит его снова только для этих фраз); облачный
/// LLM видеокарту не занимает. Some — проект с сокращёнными репликами (записан в project.json).
pub fn render_overflow(
    proj: &Project,
    paths: &crate::render::RenderPaths,
    over: &Overflow,
    engine: &mut Option<Arc<AudiocppEngine>>,
    log: &dyn Fn(String),
) -> Result<Option<Project>, String> {
    let calib = Calibration::from_samples(&proj.tgt_lang, over.samples.iter().cloned());
    let picks: Vec<(usize, f64, f64)> = over
        .picks
        .iter()
        .filter_map(|(id, target)| {
            let i = proj.segments.iter().position(|s| &s.id == id)?;
            Some((i, *target, calib.cps(&fitplan::speaker_of(&proj.segments[i])).0))
        })
        .collect();
    let list = items(proj, &picks);
    if list.is_empty() {
        return Ok(None);
    }
    log(t!("shorten-auto-start", count = list.len()));
    if crate::models::llm_backend(&paths.models_root, "llm") != crate::models::LlmBackend::OpenRouter && engine.take().is_some() {
        log(t!("shorten-higgs-unloaded"));
    }
    let prov = match open_llm(&paths.llama_bin, &paths.mt_model, &paths.models_root) {
        Ok(p) => p,
        Err(e) => {
            log(t!("shorten-auto-no-llm", error = e));
            return Ok(None);
        }
    };
    let outcome = run(prov.client(), &proj.tgt_lang, &proj.audio.translate_style, &list, log)?;
    drop(prov);
    if outcome.done.is_empty() {
        log(t!("shorten-none-shortened", count = list.len()));
        return Ok(None);
    }
    let (applied, _) = persist(&paths.work_dir, &outcome.done)?;
    let mut updated = proj.clone();
    for c in &applied {
        if let Some(seg) = updated.segments.iter_mut().find(|s| s.id == c.id) {
            mark(seg, c);
        }
    }
    log(t!("shorten-done", count = applied.len(), total = list.len()));
    Ok((!applied.is_empty()).then_some(updated))
}

// ---------------------------------------------------------------- джоба

/// POST /projects/{pid}/shorten {ids} | {all_over: true} — сократить перевод реплик под их слот.
pub async fn shorten_project(State(st): State<AppState>, AxPath(pid): AxPath<String>, Json(body): Json<Value>) -> Response {
    match shorten_enqueue(&st, &pid, &body).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(resp) => *resp,
    }
}

fn arg_ids(args: &Value) -> Vec<String> {
    args.get("ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

pub(crate) async fn shorten_enqueue(st: &AppState, pid: &str, args: &Value) -> Result<String, Box<Response>> {
    let dir = st.proj_dir(pid).map_err(Box::new)?;
    let proj = st.load_project(pid).map_err(Box::new)?;
    let ids = arg_ids(args);
    let all_over = args.get("all_over").and_then(Value::as_bool).unwrap_or(false);
    if ids.is_empty() && !all_over {
        return Err(Box::new((StatusCode::BAD_REQUEST, Json(json!({ "error": "nothing_to_shorten", "detail": "give ids or all_over: true" }))).into_response()));
    }
    let unknown: Vec<&String> = ids.iter().filter(|id| !proj.segments.iter().any(|s| &s.id == *id)).collect();
    if !unknown.is_empty() {
        return Err(Box::new((StatusCode::NOT_FOUND, Json(json!({ "error": "segment_not_found", "detail": unknown }))).into_response()));
    }
    if !matches!(proj.mode.as_str(), "dub" | "voiceover") {
        return Err(Box::new((StatusCode::CONFLICT, Json(json!({ "error": "not_dubbed", "detail": proj.mode }))).into_response()));
    }
    let llama_bin = st.llama_bin.clone();
    let (mt_model, _) = crate::models::resolve_mt(&st.models_root, &crate::models::load_selection(&st.models_root));
    let models_root = st.models_root.clone();
    let max_stretch = st.opts.max_stretch as f64;
    let dir_for_job = dir.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        let log = |m: String| progress(json!({ "stage": "shorten", "msg": m }));
        let text = std::fs::read_to_string(dir_for_job.join("project.json")).map_err(|e| e.to_string())?;
        let proj = Project::from_json(&text).map_err(|e| e.to_string())?;
        let rules = fitplan::rules(&models_root, max_stretch);
        let fits = fitplan::project_fits(&dir_for_job, &proj, &rules)?;
        let mut picks: Vec<(usize, f64, f64)> = Vec::new();
        let mut skipped: Vec<Value> = Vec::new();
        for (i, (s, f)) in proj.segments.iter().zip(&fits).enumerate() {
            let named = ids.iter().any(|id| id == &s.id);
            match f {
                Some(f) if named || (all_over && f.over()) => picks.push((i, f.target(), f.cps)),
                None if named => skipped.push(json!({ "id": s.id, "reason": "not_voiced" })),
                _ => {}
            }
        }
        if picks.is_empty() {
            log(t!("shorten-nothing"));
            return Ok(json!({ "shortened": [], "rejected": skipped, "failed": [], "unpinned": [] }));
        }
        let list = items(&proj, &picks);
        let prov = open_llm(&llama_bin, &mt_model, &models_root).map_err(|e| t!("shorten-no-llm", error = e))?;
        log(t!("shorten-start", count = list.len(), provider = prov.describe()));
        let outcome = run(prov.client(), &proj.tgt_lang, &proj.audio.translate_style, &list, &log)?;
        drop(prov);
        if outcome.done.is_empty() && outcome.rejected.is_empty() {
            let (id, e) = &outcome.failed[0];
            return Err(t!("shorten-all-failed", id = id.clone(), error = e.clone()));
        }
        let (applied, unpinned) = persist(&dir_for_job, &outcome.done)?;
        log(t!("shorten-done", count = applied.len(), total = list.len()));
        skipped.extend(outcome.rejected.iter().map(|(id, r)| json!({ "id": id, "reason": r.code() })));
        Ok(json!({
            "shortened": applied.iter().map(|c| json!({ "id": c.id, "from": c.from, "to": c.to })).collect::<Vec<_>>(),
            "rejected": skipped,
            "failed": outcome.failed.iter().map(|(id, e)| json!({ "id": id, "error": e })).collect::<Vec<_>>(),
            "unpinned": unpinned,
        }))
    });
    st.jobs
        .enqueue(jobs::JobMeta::persistent(jobs::JobKind::Shorten, pid, dir, args.clone()), job)
        .await
        .map_err(|e| Box::new(crate::enqueue_error(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dub_core::Segment;
    use dub_llm::test_http::{body_json, serve, Reply};

    fn seg(id: &str, src: &str, tgt: &str) -> Segment {
        Segment { id: id.into(), src_text: src.into(), tgt_text: tgt.into(), ..Default::default() }
    }

    #[test]
    fn answers_are_checked_before_they_replace_the_line() {
        let cur = "Я думаю, что нам, наверное, пора уже идти домой";
        assert_eq!(check("I think we should go home", cur, "Нам пора домой", "ru"), Ok("Нам пора домой".to_string()));
        assert_eq!(check("x", cur, "«Нам пора домой»\nпояснение", "ru"), Ok("Нам пора домой".to_string()));
        assert_eq!(check("x", cur, "(≤20) Нам пора", "ru"), Ok("Нам пора".to_string()));
        assert_eq!(check("x", cur, "1. (<= 20) Нам пора", "ru"), Ok("Нам пора".to_string()));
        assert_eq!(check("x", cur, "2024 год прошёл", "ru"), Ok("2024 год прошёл".to_string()));
        assert_eq!(check("x", cur, "  \n ", "ru"), Err(Reject::Empty));
        assert_eq!(check("We should go home", cur, "We should go home!", "ru"), Err(Reject::Echo));
        assert_eq!(check("x", cur, "Time to go", "ru"), Err(Reject::Alphabet));
        assert_eq!(check("x", "Time to leave now", "Пора идти", "en"), Err(Reject::Alphabet));
        assert_eq!(check("x", "Пора", "Нам пора", "ru"), Err(Reject::NotShorter));
        assert_eq!(check("x", "Hello there, my friend", "Hi, friend", "en"), Ok("Hi, friend".to_string()));
        assert_eq!(check("x", "Chúng ta phải về nhà ngay bây giờ", "Về nhà thôi", "vi"), Ok("Về nhà thôi".to_string()));
        assert_eq!(check("x", "आपण आता घरी जायला हवे", "घरी चला", "mr"), Ok("घरी चला".to_string()), "a script the language table does not list");
        assert_eq!(check("x", "आपण आता घरी जायला हवे", "Go home", "mr"), Err(Reject::Alphabet));
    }

    #[test]
    fn items_carry_two_voiced_neighbours_each_side_and_a_limit_below_the_line() {
        let mut p = Project { segments: (0..6).map(|i| seg(&format!("s{i}"), &format!("src{i}"), &format!("перевод номер {i}"))).collect(), ..Default::default() };
        p.segments[1].extra.insert("hidden".into(), json!(true));
        let it = items(&p, &[(3, 1.0, 13.0)]);
        assert_eq!(it[0].before, vec![("src0".to_string(), "перевод номер 0".to_string()), ("src2".to_string(), "перевод номер 2".to_string())]);
        assert_eq!(it[0].after.len(), 2);
        assert_eq!(it[0].after[0].0, "src4");
        assert_eq!(it[0].limit, 12, "floor(1.0 * 13 * 0.95)");
        let it = items(&p, &[(5, 10.0, 13.0)]);
        assert!(it[0].after.is_empty());
        assert_eq!(it[0].limit, fit::text_units("перевод номер 5") - 1, "never longer than the line itself");
    }

    #[test]
    fn prompt_names_the_language_limit_and_context() {
        let it = Item {
            id: "s1".into(), src: "Hello".into(), tgt: "Здравствуйте".into(), target: 0.8, limit: 9,
            before: vec![("Hi".into(), "Привет".into())], after: Vec::new(),
        };
        let text: String = prompt("ru", "formal", &it).iter().filter_map(|m| m.parts_text.clone()).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Russian") && text.contains("at most 9 characters") && text.contains("Hi => Привет"));
        assert!(text.contains("formal") && text.contains("(none)"));
    }

    #[test]
    fn a_run_asks_once_per_line_at_low_temperature_and_keeps_going_after_a_rejection() {
        let server = serve(vec![
            Reply::json(200, &json!({ "choices": [{ "message": { "content": "Нам пора" } }] }).to_string()),
            Reply::json(200, &json!({ "choices": [{ "message": { "content": "Too long reply in English" } }] }).to_string()),
        ]);
        let client = ChatClient::new(server.base()).unwrap();
        let list = vec![
            Item { id: "a".into(), src: "We must go".into(), tgt: "Нам уже точно пора идти".into(), target: 1.0, limit: 12, before: vec![], after: vec![] },
            Item { id: "b".into(), src: "Stay".into(), tgt: "Останься со мной, пожалуйста".into(), target: 1.0, limit: 12, before: vec![], after: vec![] },
        ];
        let out = run(&client, "ru", "", &list, &|_| {}).unwrap();
        assert_eq!(out.done, vec![Change { id: "a".into(), from: "Нам уже точно пора идти".into(), to: "Нам пора".into() }]);
        assert_eq!(out.rejected, vec![("b".to_string(), Reject::Alphabet)]);
        let sent = body_json(&server.request(0));
        assert!((sent["temperature"].as_f64().unwrap() - 0.15).abs() < 1e-6, "{sent}");
    }

    fn render_paths(dir: &Path, models_root: &Path) -> crate::render::RenderPaths {
        let none = dir.join("missing");
        crate::render::RenderPaths {
            input: none.clone(),
            work_dir: dir.to_path_buf(),
            output: dir.join("output.mp4"),
            bsroformer_cli: none.clone(),
            bsroformer_model: none.clone(),
            higgs_dll: none.clone(),
            higgs_model_root: none.clone(),
            higgs_quant: "q8_0".into(),
            fonts_dir: none.clone(),
            higgs_backend: "cuda".into(),
            higgs_device: 0,
            higgs_threads: 1,
            max_stretch: 1.25,
            voices_dir: none.clone(),
            asr: crate::models::AsrChoice::Parakeet(none.clone()),
            bench: false,
            ref_secs: 12.0,
            models_root: models_root.to_path_buf(),
            llama_bin: none.clone(),
            mt_model: none,
        }
    }

    #[test]
    fn the_render_loop_rewrites_the_lines_that_did_not_fit_and_saves_them() {
        let _language = crate::i18n::test_language("ru");
        let d = std::env::temp_dir().join(format!("dub_overflow_{}_{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        let models = d.join("models");
        std::fs::create_dir_all(&models).unwrap();
        let server = serve(vec![Reply::json(200, &json!({ "choices": [{ "message": { "content": "Нам пора" }, "finish_reason": "stop" }] }).to_string())]);
        crate::models::set_selection(&models, "llm_provider", "server").unwrap();
        crate::models::set_selection(&models, "srv_url", &server.base()).unwrap();
        crate::models::set_selection(&models, "srv_llm", "fake").unwrap();
        let p = Project {
            tgt_lang: "ru".into(),
            segments: vec![seg("s1", "We must go home right now", "Нам прямо сейчас уже точно пора идти домой"), seg("s2", "Yes", "Да")],
            ..Default::default()
        };
        crate::save_project_atomic(&d, &p).unwrap();
        let over = Overflow {
            picks: vec![("s1".into(), 1.0)],
            samples: vec![("0".into(), 20, 2.0), ("0".into(), 30, 3.0), ("0".into(), 10, 1.0)],
            kept: Default::default(),
        };
        let logs = std::cell::RefCell::new(Vec::new());
        let mut engine = None;
        let updated = render_overflow(&p, &render_paths(&d, &models), &over, &mut engine, &|m| logs.borrow_mut().push(m)).unwrap().unwrap();
        assert_eq!(updated.segments[0].tgt_text, "Нам пора");
        assert!(already_shortened(&updated.segments[0]) && updated.segments[0].dirty);
        assert_eq!(updated.segments[1].tgt_text, "Да");
        let sent = body_json(&server.request(0));
        assert!(sent.to_string().contains("at most 9 characters"), "the voice's calibrated 10 characters/s over a 1 s slot: {sent}");
        let disk = Project::from_json(&std::fs::read_to_string(d.join("project.json")).unwrap()).unwrap();
        assert_eq!(disk.segments[0].tgt_text, "Нам пора");
        assert!(logs.borrow().iter().any(|l| l.contains("сокращено 1")), "{:?}", logs.borrow());

        crate::models::set_selection(&models, "llm_provider", "local").unwrap();
        let logs = std::cell::RefCell::new(Vec::new());
        let again = render_overflow(&disk, &render_paths(&d, &models), &over, &mut engine, &|m| logs.borrow_mut().push(m)).unwrap();
        assert!(again.is_none(), "without a translation model the render goes on with the lines as voiced");
        assert!(logs.borrow().iter().any(|l| l.contains("LLM недоступен")), "{:?}", logs.borrow());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn persist_writes_only_lines_that_still_hold_the_shortened_text() {
        let d = std::env::temp_dir().join(format!("dub_shorten_{}_{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        let p = Project { segments: vec![seg("a", "x", "Длинная строка"), seg("b", "y", "Уже правили")], ..Default::default() };
        crate::save_project_atomic(&d, &p).unwrap();
        let changes = vec![
            Change { id: "a".into(), from: "Длинная строка".into(), to: "Строка".into() },
            Change { id: "b".into(), from: "Другой текст".into(), to: "Короче".into() },
        ];
        let (applied, unpinned) = persist(&d, &changes).unwrap();
        assert_eq!(applied, changes[..1].to_vec());
        assert!(unpinned.is_empty());
        let back = Project::from_json(&std::fs::read_to_string(d.join("project.json")).unwrap()).unwrap();
        assert_eq!(back.segments[0].tgt_text, "Строка");
        assert!(back.segments[0].dirty && already_shortened(&back.segments[0]));
        assert_eq!(back.segments[0].extra["shortened"]["from"], "Длинная строка");
        assert_eq!(back.segments[1].tgt_text, "Уже правили");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
