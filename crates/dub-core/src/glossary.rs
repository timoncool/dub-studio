//! Глоссарий проекта и сериала: термин, его перевод или «не переводить», как произносить в озвучке и как
//! его ошибочно пишет распознавание речи. Здесь — тип записи и текстовые операции над ним (поиск термина в
//! строке, замены ASR и произношения, TSV); перевод и озвучка берут их отсюда.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Откуда запись: внесена человеком, собрана из текста моделью (ждёт подтверждения) или взята анализом из
/// глоссария сериала (повторный анализ берёт такие записи из профиля заново).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlossarySource {
    #[default]
    Manual,
    Auto,
    Series,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GlossaryEntry {
    pub term: String,
    #[serde(default)]
    pub translation: String,
    /// Не переводить: термин остаётся в переводе как есть.
    #[serde(default)]
    pub keep: bool,
    /// Как произносить в озвучке (на экране остаётся перевод).
    #[serde(default)]
    pub pronunciation: String,
    /// Как распознавание речи ошибочно пишет термин.
    #[serde(default)]
    pub asr_fix: Vec<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub source: GlossarySource,
    /// Язык перевода и произношения записи (код, как tgt_lang); пусто — любой язык.
    #[serde(default)]
    pub lang: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Потолки глоссария: строка таблицы, а не текст.
const MAX_ENTRIES: usize = 5000;
const MAX_FIELD: usize = 300;

impl GlossaryEntry {
    /// Что запись требует от перевода: «keep: term» или «term → translation»; None — перевода не касается.
    pub fn prompt_line(&self) -> Option<String> {
        if self.keep {
            Some(format!("keep: {}", self.term))
        } else if !self.translation.is_empty() {
            Some(format!("{} → {}", self.term, self.translation))
        } else {
            None
        }
    }

    /// Текст, который обязан оказаться в переводе строки с этим термином.
    pub fn required_in_translation(&self) -> Option<&str> {
        if self.keep {
            Some(&self.term)
        } else if !self.translation.is_empty() {
            Some(&self.translation)
        } else {
            None
        }
    }

    /// Та же запись по смыслу: тот же термин, и языки совпадают или один из них — «любой».
    fn overlaps(&self, other: &GlossaryEntry) -> bool {
        let (a, b) = (primary_lang(&self.lang), primary_lang(&other.lang));
        normalize(&self.term) == normalize(&other.term) && (a.is_empty() || b.is_empty() || a == b)
    }
}

/// Нормальная форма для сравнения: нижний регистр, ё как е, пробелы схлопнуты.
pub fn normalize(s: &str) -> String {
    let lower: String = s.to_lowercase().chars().map(|c| if c == 'ё' { 'е' } else { c }).collect();
    lower.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn primary_lang(code: &str) -> String {
    code.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase()
}

/// Перевод и произношение записи языка `entry_lang` годятся для целевого языка `target`.
pub fn lang_matches(entry_lang: &str, target: &str) -> bool {
    let e = primary_lang(entry_lang);
    e.is_empty() || e == primary_lang(target)
}

/// Записи для целевого языка: у записей другого языка перевод и произношение не применяются.
pub fn for_target(entries: &[GlossaryEntry], target: &str) -> Vec<GlossaryEntry> {
    entries
        .iter()
        .map(|e| {
            let mut e = e.clone();
            if !lang_matches(&e.lang, target) {
                e.translation.clear();
                e.pronunciation.clear();
            }
            e
        })
        .collect()
}

/// Ручные записи раньше собранных моделью (порядок внутри группы сохраняется).
pub fn manual_first(entries: &mut [GlossaryEntry]) {
    entries.sort_by_key(|e| e.source == GlossarySource::Auto);
}

/// Записи, которые задают перевод на язык `target` («keep: term» или «term → translation»), ручные раньше
/// собранных. Ровно этот список перевод берёт из глоссария: промпт, проверка ответа, term-lock, отпечаток
/// для кэша перевода. Произношение, варианты распознавания, примечание перевода не касаются.
pub fn for_translation(entries: &[GlossaryEntry], target: &str) -> Vec<GlossaryEntry> {
    let mut out: Vec<GlossaryEntry> = for_target(entries, target).into_iter().filter(|e| e.prompt_line().is_some()).collect();
    manual_first(&mut out);
    out
}

/// Отказ проверки глоссария. `code` и `args` — стабильный код и его аргументы (окно и сервер берут текст по
/// ним из локали); Display — тот же отказ по-английски для журнала.
#[derive(Clone, Debug, PartialEq)]
pub enum GlossaryError {
    OverLimit { total: usize, max: usize },
    EmptyTerm { entry: usize },
    FieldTooLong { entry: usize, term: String, max: usize },
    Duplicate { term: String, first: usize, second: usize },
    TsvKeep { line: usize, value: String },
    TsvEmptyTerm { line: usize },
    OneOf,
    Nothing,
}

impl GlossaryError {
    pub fn code(&self) -> &'static str {
        match self {
            GlossaryError::OverLimit { .. } => "glossary_over_limit",
            GlossaryError::EmptyTerm { .. } => "glossary_empty_term",
            GlossaryError::FieldTooLong { .. } => "glossary_field_too_long",
            GlossaryError::Duplicate { .. } => "glossary_duplicate",
            GlossaryError::TsvKeep { .. } => "glossary_tsv_keep",
            GlossaryError::TsvEmptyTerm { .. } => "glossary_tsv_empty_term",
            GlossaryError::OneOf => "glossary_one_of",
            GlossaryError::Nothing => "glossary_nothing",
        }
    }

    pub fn args(&self) -> Value {
        match self {
            GlossaryError::OverLimit { total, max } => json!({ "total": total, "max": max }),
            GlossaryError::EmptyTerm { entry } => json!({ "entry": entry }),
            GlossaryError::FieldTooLong { entry, term, max } => json!({ "entry": entry, "term": term, "max": max }),
            GlossaryError::Duplicate { term, first, second } => json!({ "term": term, "first": first, "second": second }),
            GlossaryError::TsvKeep { line, value } => json!({ "line": line, "value": value }),
            GlossaryError::TsvEmptyTerm { line } => json!({ "line": line }),
            GlossaryError::OneOf | GlossaryError::Nothing => json!({}),
        }
    }
}

impl std::fmt::Display for GlossaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlossaryError::OverLimit { total, max } => write!(f, "the glossary has {total} entries, more than {max}"),
            GlossaryError::EmptyTerm { entry } => write!(f, "entry {entry}: empty term"),
            GlossaryError::FieldTooLong { entry, term, max } => write!(f, "entry {entry} ({term:?}): a field is longer than {max} characters"),
            GlossaryError::Duplicate { term, first, second } => write!(f, "term {term:?} is given twice (entries {first} and {second})"),
            GlossaryError::TsvKeep { line, value } => write!(f, "line {line}: keep {value:?}, expected 1 or 0"),
            GlossaryError::TsvEmptyTerm { line } => write!(f, "line {line}: empty term"),
            GlossaryError::OneOf => write!(f, "give either entries or tsv, not both"),
            GlossaryError::Nothing => write!(f, "give entries (a list of entries) or tsv"),
        }
    }
}

/// Проверить и привести в порядок список: поля обрезаны, пустой термин и повтор термина того же языка
/// (или без языка) — ошибка с номером строки.
pub fn validate(entries: Vec<GlossaryEntry>) -> Result<Vec<GlossaryEntry>, GlossaryError> {
    if entries.len() > MAX_ENTRIES {
        return Err(GlossaryError::OverLimit { total: entries.len(), max: MAX_ENTRIES });
    }
    let mut out: Vec<GlossaryEntry> = Vec::with_capacity(entries.len());
    let mut seen: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, mut e) in entries.into_iter().enumerate() {
        let n = i + 1;
        e.term = one_line(&e.term);
        e.translation = one_line(&e.translation);
        e.pronunciation = one_line(&e.pronunciation);
        e.note = one_line(&e.note);
        e.lang = e.lang.trim().to_string();
        e.asr_fix = e.asr_fix.iter().map(|v| one_line(v)).filter(|v| !v.is_empty()).collect();
        if e.term.is_empty() {
            return Err(GlossaryError::EmptyTerm { entry: n });
        }
        for field in [&e.term, &e.translation, &e.pronunciation, &e.note] {
            if field.chars().count() > MAX_FIELD {
                return Err(GlossaryError::FieldTooLong { entry: n, term: e.term.clone(), max: MAX_FIELD });
            }
        }
        let same = seen.entry(normalize(&e.term)).or_default();
        if let Some(&first) = same.iter().find(|&&k| out[k].overlaps(&e)) {
            return Err(GlossaryError::Duplicate { term: e.term, first: first + 1, second: n });
        }
        same.push(out.len());
        out.push(e);
    }
    Ok(out)
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Слить: записи `primary` главнее записей `secondary` о том же термине (тот же язык или без языка);
/// порядок — сначала `primary`, затем новые из `secondary`.
pub fn merge_under(primary: &[GlossaryEntry], secondary: &[GlossaryEntry]) -> Vec<GlossaryEntry> {
    let mut out = primary.to_vec();
    out.extend(secondary.iter().filter(|e| !primary.iter().any(|p| p.overlaps(e))).cloned());
    out
}

/// Слить импорт TSV в глоссарий: у записи о том же термине меняются только колонки TSV (перевод, keep,
/// произношение) — варианты распознавания, примечание, источник и язык остаются; новые термины — в конец.
pub fn merge_tsv(imported: &[GlossaryEntry], current: &[GlossaryEntry]) -> Vec<GlossaryEntry> {
    let mut out = current.to_vec();
    for new in imported {
        match out.iter_mut().find(|e| e.overlaps(new)) {
            Some(e) => {
                e.translation = new.translation.clone();
                e.keep = new.keep;
                e.pronunciation = new.pronunciation.clone();
            }
            None => out.push(new.clone()),
        }
    }
    out
}

// ── поиск термина в тексте ────────────────────────────────────────────────

/// Письмо без пробелов между словами: термин ищется подстрокой, а не по словам.
fn unspaced_script(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xAC00..=0xD7AF
        | 0x0E00..=0x0E7F | 0x0E80..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF)
}

/// Слова текста: (начало, конец в байтах, нормальная форма).
fn words(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push((s, i, normalize(&text[s..i])));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len(), normalize(&text[s..])));
    }
    out
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Слово текста — это слово термина: короче 5 букв — точно; от 5 — по основе (общее начало не короче 4 букв
/// и термина без 1-2 букв окончания), от 6 — ещё с одной опечаткой не в первой букве.
fn source_word_matches(w: &str, t: &str) -> bool {
    if w == t {
        return true;
    }
    let (wl, tl) = (w.chars().count(), t.chars().count());
    if tl < 5 {
        return false;
    }
    let stem = common_prefix(w, t) >= (tl - 2).max(4) && wl + 1 >= tl && wl <= tl + 3;
    let typo = tl >= 6 && w.chars().next() == t.chars().next() && levenshtein(w, t) <= 1;
    stem || typo
}

/// Слово перевода содержит слово термина с падежным окончанием («Рон» — «Рона», «Хогвартс» — «Хогвартсе»).
fn inflected_word_matches(w: &str, t: &str) -> bool {
    if source_word_matches(w, t) {
        return true;
    }
    let tl = t.chars().count();
    tl >= 3 && w.starts_with(t) && w.chars().count() <= tl + 3
}

fn phrase_found(text: &str, phrase: &str, word_eq: fn(&str, &str) -> bool) -> bool {
    let phrase_n = normalize(phrase);
    if phrase_n.is_empty() {
        return false;
    }
    if phrase_n.chars().any(unspaced_script) {
        return normalize(text).contains(&phrase_n);
    }
    let want: Vec<String> = words(&phrase_n).into_iter().map(|w| w.2).collect();
    if want.is_empty() {
        return false;
    }
    let have = words(text);
    have.windows(want.len()).any(|win| win.iter().zip(&want).all(|(h, t)| word_eq(&h.2, t)))
}

/// Термин встречается в исходной строке (регистр не важен; от 5 букв — по основе или с одной опечаткой).
pub fn term_in_source(text: &str, term: &str) -> bool {
    phrase_found(text, term, source_word_matches)
}

/// Перевод содержит требуемый текст — с учётом падежных окончаний.
pub fn contains_in_translation(text: &str, required: &str) -> bool {
    phrase_found(text, required, inflected_word_matches)
}

/// Записи, чей термин встречается в тексте.
pub fn hits<'a>(entries: &'a [GlossaryEntry], text: &str) -> Vec<&'a GlossaryEntry> {
    entries.iter().filter(|e| term_in_source(text, &e.term)).collect()
}

// ── замены ────────────────────────────────────────────────────────────────

/// Термин в регистре найденного текста: КАПСОМ — капсом, с заглавной — с заглавной, иначе как записан.
fn cased_like(found: &str, term: &str) -> String {
    let letters: Vec<char> = found.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return term.to_uppercase();
    }
    let mut chars = term.chars();
    match (found.chars().next(), chars.next()) {
        (Some(f), Some(t)) if f.is_uppercase() && t.is_lowercase() => t.to_uppercase().chain(chars).collect(),
        _ => term.to_string(),
    }
}

/// Заменить целые вхождения `from` (слова в нормальной форме, подряд) на `to`, регистр — по найденному.
fn replace_phrase(text: &str, from: &str, to: &str) -> (String, usize) {
    let want: Vec<String> = words(from).into_iter().map(|w| w.2).collect();
    if want.is_empty() {
        return (text.to_string(), 0);
    }
    if normalize(from).chars().any(unspaced_script) {
        let n = text.matches(from).count();
        return (text.replace(from, to), n);
    }
    let have = words(text);
    let mut out = String::with_capacity(text.len());
    let (mut pos, mut i, mut count) = (0usize, 0usize, 0usize);
    while i + want.len() <= have.len() {
        if have[i..i + want.len()].iter().zip(&want).all(|(h, w)| h.2 == *w) {
            let (s, e) = (have[i].0, have[i + want.len() - 1].1);
            let found = &text[s..e];
            let replaced = cased_like(found, to);
            if found != replaced {
                out.push_str(&text[pos..s]);
                out.push_str(&replaced);
                pos = e;
                count += 1;
            }
            i += want.len();
        } else {
            i += 1;
        }
    }
    out.push_str(&text[pos..]);
    (out, count)
}

/// Исправить ошибки распознавания: варианты asr_fix (целыми словами, регистр не важен) -> термин.
/// Возвращает текст и число замен.
pub fn apply_asr_fix(text: &str, entries: &[GlossaryEntry]) -> (String, usize) {
    let mut out = text.to_string();
    let mut total = 0;
    for e in entries {
        for variant in &e.asr_fix {
            if normalize(variant) == normalize(&e.term) {
                continue;
            }
            let (next, n) = replace_phrase(&out, variant, &e.term);
            out = next;
            total += n;
        }
    }
    (out, total)
}

/// Текст для синтеза с произношением глоссария: перевод термина (или сам термин) целыми словами ->
/// pronunciation. Записи другого языка не применяются.
pub fn apply_pronunciation(text: &str, entries: &[GlossaryEntry], target: &str) -> String {
    let mut out = text.to_string();
    for e in entries.iter().filter(|e| !e.pronunciation.is_empty() && lang_matches(&e.lang, target)) {
        for said in [e.translation.as_str(), e.term.as_str()] {
            if said.is_empty() || normalize(said) == normalize(&e.pronunciation) {
                continue;
            }
            out = replace_phrase(&out, said, &e.pronunciation).0;
        }
    }
    out
}

// ── TSV ───────────────────────────────────────────────────────────────────

pub const TSV_HEADER: &str = "term\ttranslation\tkeep\tpronunciation";

/// Глоссарий как TSV: term, translation, keep (1/0), pronunciation.
pub fn to_tsv(entries: &[GlossaryEntry]) -> String {
    let mut out = String::from(TSV_HEADER);
    out.push('\n');
    for e in entries {
        let cell = |s: &str| s.replace(['\t', '\r', '\n'], " ");
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            cell(&e.term),
            cell(&e.translation),
            if e.keep { "1" } else { "0" },
            cell(&e.pronunciation)
        ));
    }
    out
}

/// Разобрать TSV (строка заголовка необязательна). Записи получают язык `lang` и source=manual.
pub fn from_tsv(text: &str, lang: &str) -> Result<Vec<GlossaryEntry>, GlossaryError> {
    let mut out = Vec::new();
    for (i, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if i == 0 && cols[0].trim().eq_ignore_ascii_case("term") {
            continue;
        }
        let col = |k: usize| cols.get(k).map(|s| s.trim()).unwrap_or("");
        let keep = match col(2).to_lowercase().as_str() {
            "" | "0" | "false" | "no" | "нет" => false,
            "1" | "true" | "yes" | "да" | "keep" => true,
            other => return Err(GlossaryError::TsvKeep { line: n, value: other.to_string() }),
        };
        if col(0).is_empty() {
            return Err(GlossaryError::TsvEmptyTerm { line: n });
        }
        out.push(GlossaryEntry {
            term: col(0).to_string(),
            translation: col(1).to_string(),
            keep,
            pronunciation: col(3).to_string(),
            lang: lang.to_string(),
            ..GlossaryEntry::default()
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, translation: &str) -> GlossaryEntry {
        GlossaryEntry { term: term.into(), translation: translation.into(), ..GlossaryEntry::default() }
    }

    #[test]
    fn a_term_is_found_by_its_stem_or_one_typo_only_when_long() {
        assert!(term_in_source("Where is Hermione?", "hermione"));
        assert!(term_in_source("Hermiones wand", "Hermione"), "stem");
        assert!(term_in_source("Hermiane said", "Hermione"), "one typo");
        for other in ["It's hard", "no harm", "carry it", "hurry up", "marry me", "a harsh harvest"] {
            assert!(!term_in_source(other, "Harry"), "{other}");
        }
        assert!(term_in_source("Harrys owl", "Harry"), "stem");
        assert!(!term_in_source("Rons", "Ron"), "short words match exactly");
        assert!(term_in_source("Ron's here", "Ron"));
        assert!(!term_in_source("Samples of it", "Sam"));
        assert!(term_in_source("the Dark Lord rises", "dark lord"));
        assert!(!term_in_source("dark and lordly", "dark lord"));
        assert!(term_in_source("我喜欢哈利波特", "哈利"), "unspaced script: substring");
        assert!(term_in_source("Всё о ёлке", "елке"), "ё is е");
    }

    #[test]
    fn a_translation_is_found_with_its_case_ending() {
        assert!(contains_in_translation("Где Рона?", "Рон"));
        assert!(contains_in_translation("в Хогвартсе", "Хогвартс"));
        assert!(contains_in_translation("Тёмного Лорда нет", "Тёмный Лорд"), "ы/ого differ by stem");
        assert!(!contains_in_translation("Где Гарри?", "Рон"));
        assert!(contains_in_translation("Купи Nvidia", "nvidia"));
    }

    #[test]
    fn asr_mistakes_become_the_term_in_the_case_of_the_line() {
        let mut e = entry("Hermione", "");
        e.asr_fix = vec!["her mooney".into(), "Hermoine".into()];
        let (t, n) = apply_asr_fix("Her mooney, wait! I said hermoine. HER MOONEY!", std::slice::from_ref(&e));
        assert_eq!(t, "Hermione, wait! I said Hermione. HERMIONE!");
        assert_eq!(n, 3);
        let (t, n) = apply_asr_fix("her moon", &[e]);
        assert_eq!((t.as_str(), n), ("her moon", 0), "only whole words");
    }

    #[test]
    fn pronunciation_changes_only_what_is_said_and_only_for_its_language() {
        let mut e = entry("Nvidia", "");
        e.keep = true;
        e.pronunciation = "Энвидиа".into();
        e.lang = "ru".into();
        let entries = [e];
        assert_eq!(apply_pronunciation("Карта Nvidia лучше", &entries, "ru"), "Карта Энвидиа лучше");
        assert_eq!(apply_pronunciation("La tarjeta Nvidia", &entries, "es"), "La tarjeta Nvidia");
        assert_eq!(apply_pronunciation("NvidiaX", &entries, "ru"), "NvidiaX");
    }

    #[test]
    fn another_languages_translation_is_not_applied() {
        let mut e = entry("Hermione", "Гермиона");
        e.lang = "ru".into();
        e.pronunciation = "Гер-ми-о-на".into();
        let es = for_target(std::slice::from_ref(&e), "es");
        assert!(es[0].translation.is_empty() && es[0].pronunciation.is_empty());
        let ru = for_target(&[e], "ru-RU");
        assert_eq!(ru[0].translation, "Гермиона");
    }

    #[test]
    fn the_translation_takes_only_the_entries_that_set_it_manual_first() {
        let mut auto = entry("Ron", "Рон");
        auto.source = GlossarySource::Auto;
        let mut spoken_only = entry("Nvidia", "");
        spoken_only.pronunciation = "Энвидиа".into();
        let mut other_lang = entry("Hermione", "Гермиона");
        other_lang.lang = "es".into();
        let mut kept = entry("GPU", "");
        kept.keep = true;
        let got = for_translation(&[auto, spoken_only, other_lang, entry("Harry", "Гарри"), kept], "ru");
        assert_eq!(got.iter().map(|e| e.term.as_str()).collect::<Vec<_>>(), vec!["Harry", "GPU", "Ron"]);
    }

    #[test]
    fn validation_refuses_an_empty_or_repeated_term() {
        let empty = validate(vec![entry(" ", "x")]).unwrap_err();
        assert_eq!((empty.code(), empty.args()["entry"].as_u64()), ("glossary_empty_term", Some(1)));
        assert!(empty.to_string().contains("empty term"));
        let err = validate(vec![entry("Harry", "Гарри"), entry("harry ", "Гарри")]).unwrap_err();
        assert_eq!(err.code(), "glossary_duplicate");
        assert_eq!((err.args()["term"].as_str(), err.args()["first"].as_u64(), err.args()["second"].as_u64()), (Some("harry"), Some(1), Some(2)));
        assert!(err.to_string().contains("given twice"), "{err}");
        let mut ru = entry("Harry", "Гарри");
        ru.lang = "ru".into();
        let mut es = entry("Harry", "Harry");
        es.lang = "es".into();
        let ok = validate(vec![es, ru.clone(), entry("  Dark   Lord ", "Тёмный Лорд")]).unwrap();
        assert_eq!(ok[2].term, "Dark Lord");
        assert!(validate(vec![entry("Harry", "x"), ru]).is_err(), "an entry of any language covers each one");
    }

    #[test]
    fn the_project_wins_over_the_series() {
        let merged = merge_under(&[entry("Harry", "Гарри")], &[entry("harry", "Хэрри"), entry("Ron", "Рон")]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].translation, "Гарри");
        assert_eq!(merged[1].term, "Ron");
        let mut ru = entry("Harry", "Гарри");
        ru.lang = "ru".into();
        let mut es = entry("Harry", "Harry");
        es.lang = "es".into();
        assert_eq!(merge_under(&[ru.clone()], &[es.clone()]).len(), 2, "other languages live side by side");
        assert_eq!(merge_under(&[ru], &[entry("Harry", "")]).len(), 1);
    }

    #[test]
    fn a_tsv_import_changes_only_its_columns() {
        let mut harry = entry("Harry", "Гарри");
        harry.asr_fix = vec!["hairy".into()];
        harry.note = "hero".into();
        harry.source = GlossarySource::Auto;
        harry.lang = "ru".into();
        let merged = merge_tsv(&[entry("harry", "Гарри Поттер"), entry("Ron", "Рон")], std::slice::from_ref(&harry));
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], GlossaryEntry { translation: "Гарри Поттер".into(), ..harry });
        assert_eq!(merged[1].term, "Ron");
    }

    #[test]
    fn tsv_round_trips_and_names_the_bad_line() {
        let mut keep = entry("Nvidia", "");
        keep.keep = true;
        keep.pronunciation = "Энвидиа".into();
        let tsv = to_tsv(&[entry("Harry", "Гарри"), keep]);
        assert!(tsv.starts_with(TSV_HEADER));
        let back = from_tsv(&tsv, "ru").unwrap();
        assert_eq!(back.len(), 2);
        assert!(back[1].keep && back[1].pronunciation == "Энвидиа" && back[1].lang == "ru");
        let keep = from_tsv("a\tb\tmaybe", "ru").unwrap_err();
        assert_eq!((keep.code(), keep.args()["line"].as_u64(), keep.args()["value"].as_str()), ("glossary_tsv_keep", Some(1), Some("maybe")));
        assert!(keep.to_string().contains("line 1"));
        assert_eq!(from_tsv("\tb", "ru").unwrap_err().code(), "glossary_tsv_empty_term");
    }

    #[test]
    fn old_records_and_unknown_fields_survive() {
        let e: GlossaryEntry = serde_json::from_str(r#"{"term":"Harry","future":1}"#).unwrap();
        assert_eq!(e.source, GlossarySource::Manual);
        assert!(serde_json::to_string(&e).unwrap().contains("future"));
        let a: GlossaryEntry = serde_json::from_str(r#"{"term":"x","source":"auto"}"#).unwrap();
        assert_eq!(a.source, GlossarySource::Auto);
    }
}
