//! Русский текст для синтеза: числа, даты, время, годы, века, порядковые, проценты, валюты и единицы — словами в
//! нужном падеже и роде. Один модуль для студий (Dub Studio и Speech Studio). Правила и их порядок — перенос ядра
//! ru-normalizr (MIT): падеж числа берётся из предлога слева (`PREP_CASE`), формы единиц — из таблицы
//! `data/ru_units.tsv` (сгенерирована из ru-normalizr и pymorphy3 скриптом `data/gen_ru_units.py`), сами
//! числительные — num2words2-core. Согласование с произвольным существительным без морфологии — по окончанию
//! следующего слова; что не разобрано, остаётся как есть и попадает в список «не распознано».

use std::collections::HashMap;
use std::sync::LazyLock;

use num2words2_core::base::{KwVal, Kwargs, Lang};
use num_bigint::BigInt;
use regex::Regex;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Nom,
    Gen,
    Dat,
    Acc,
    Ins,
    Loc,
}

impl Case {
    fn code(self) -> &'static str {
        match self {
            Case::Nom => "n",
            Case::Gen => "g",
            Case::Dat => "d",
            Case::Acc => "a",
            Case::Ins => "i",
            Case::Loc => "p",
        }
    }

    fn index(self) -> usize {
        match self {
            Case::Nom => 0,
            Case::Gen => 1,
            Case::Dat => 2,
            Case::Acc => 3,
            Case::Ins => 4,
            Case::Loc => 5,
        }
    }

    fn from_tag(tag: &str) -> Case {
        match tag {
            "gent" => Case::Gen,
            "datv" => Case::Dat,
            "accs" => Case::Acc,
            "ablt" => Case::Ins,
            "loct" => Case::Loc,
            _ => Case::Nom,
        }
    }

    /// Именительный и винительный: число управляет существительным (2 стола, 5 столов).
    fn direct(self) -> bool {
        matches!(self, Case::Nom | Case::Acc)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gender {
    Masc,
    Fem,
    Neut,
}

impl Gender {
    fn code(self) -> &'static str {
        match self {
            Gender::Masc => "m",
            Gender::Fem => "f",
            Gender::Neut => "n",
        }
    }

    fn from_tag(tag: &str) -> Gender {
        match tag {
            "femn" => Gender::Fem,
            "neut" => Gender::Neut,
            _ => Gender::Masc,
        }
    }
}

fn ru() -> &'static (dyn Lang + Sync) {
    num2words2_core::get_lang_by_key("ru").expect("num2words2 has Russian")
}

fn kwargs(case: Case, gender: Gender, plural: bool, animate: bool) -> Kwargs {
    Kwargs(vec![
        ("case".into(), KwVal::Str(case.code().into())),
        ("gender".into(), KwVal::Str(gender.code().into())),
        ("plural".into(), KwVal::Bool(plural)),
        ("animate".into(), KwVal::Bool(animate)),
    ])
}

/// Количественное числительное: «двух», «одну», «пятью».
pub fn cardinal(n: &BigInt, case: Case, gender: Gender, animate: bool) -> String {
    let negative = n.sign() == num_bigint::Sign::Minus;
    let abs = if negative { -n.clone() } else { n.clone() };
    let words = ru().to_cardinal_kw(&abs, &kwargs(case, gender, false, animate)).unwrap_or_else(|_| abs.to_string());
    if negative { format!("минус {words}") } else { words }
}

/// Порядковое числительное: «двадцать шестом», «десятого», «девяностых».
pub fn ordinal(n: &BigInt, case: Case, gender: Gender, plural: bool) -> String {
    ru().to_ordinal_kw(n, &kwargs(case, gender, plural, false)).unwrap_or_else(|_| n.to_string())
}

// ── Таблицы ────────────────────────────────────────────────────────────────

/// Предлог (или оборот) слева от числа → падеж числа (ru-normalizr `PREP_CASE`).
const PREP_CASE: &[(&str, &str)] = &[
    ("в", "accs"), ("о", "loct"), ("об", "loct"), ("на", "accs"), ("за", "accs"), ("из", "gent"), ("с", "gent"),
    ("к", "datv"), ("по", "datv"), ("для", "gent"), ("без", "gent"), ("при", "loct"), ("между", "ablt"), ("до", "gent"),
    ("от", "gent"), ("у", "gent"), ("во", "accs"), ("со", "gent"), ("ко", "datv"), ("обо", "loct"), ("около", "gent"),
    ("порядка", "gent"), ("не более", "gent"), ("не менее", "gent"), ("не больше", "gent"), ("не меньше", "gent"),
    ("более", "gent"), ("больше", "gent"), ("менее", "gent"), ("меньше", "gent"), ("свыше", "gent"), ("после", "gent"),
    ("против", "gent"), ("среди", "gent"), ("вместо", "gent"), ("возле", "gent"), ("вокруг", "gent"), ("внутри", "gent"),
    ("вне", "gent"), ("ради", "gent"), ("в течение", "gent"), ("течение", "gent"), ("в продолжение", "gent"),
    ("из-за", "gent"), ("из-под", "gent"), ("вблизи", "gent"), ("близ", "gent"), ("вроде", "gent"), ("наподобие", "gent"),
    ("касательно", "gent"), ("относительно", "gent"), ("насчет", "gent"), ("насчёт", "gent"), ("по поводу", "gent"),
    ("по причине", "gent"), ("ввиду", "gent"), ("в силу", "gent"), ("в целях", "gent"), ("в рамках", "gent"),
    ("в ходе", "gent"), ("в отсутствие", "gent"), ("во избежание", "gent"), ("по истечении", "gent"),
    ("исходя из", "gent"), ("независимо от", "gent"), ("в отличие от", "gent"), ("в зависимости от", "gent"),
    ("вследствие", "gent"), ("через", "accs"), ("вдоль", "gent"), ("благодаря", "datv"), ("вопреки", "datv"),
    ("согласно", "datv"), ("навстречу", "datv"), ("подобно", "datv"), ("сообразно", "datv"), ("применительно к", "datv"),
    ("по направлению к", "datv"), ("по мере", "gent"), ("над", "ablt"), ("надо", "ablt"), ("перед", "ablt"),
    ("передо", "ablt"), ("пред", "ablt"), ("меж", "ablt"), ("в связи с", "ablt"), ("рядом с", "ablt"),
    ("вместе с", "ablt"), ("наряду с", "ablt"), ("старше", "gent"), ("моложе", "gent"), ("младше", "gent"),
    ("выше", "gent"), ("ниже", "gent"), ("дольше", "gent"), ("короче", "gent"), ("позднее", "gent"), ("ранее", "gent"),
    ("позже", "gent"), ("раньше", "gent"),
];

const MONTHS_GEN: [&str; 12] = [
    "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря",
];

/// Слова-подписи, после которых число читается порядковым («глава пятая»): слово и его род.
const ORDINAL_LABELS: &[(&str, Gender)] = &[
    ("глава", Gender::Fem), ("часть", Gender::Fem), ("серия", Gender::Fem), ("сцена", Gender::Fem), ("книга", Gender::Fem),
    ("том", Gender::Masc), ("сезон", Gender::Masc), ("эпизод", Gender::Masc), ("урок", Gender::Masc),
    ("раздел", Gender::Masc), ("акт", Gender::Masc), ("параграф", Gender::Masc),
];

#[derive(Clone, Debug)]
struct Unit {
    gender: Gender,
    animate: bool,
    suffix: String,
    /// sing nomn..loct, plur nomn..loct.
    forms: [String; 12],
    /// Родительный единственного после 2–4 («три кубических метра»).
    paucal: String,
    minor: Option<String>,
}

struct Units {
    by_key: HashMap<String, Unit>,
    by_key_exact: HashMap<String, Unit>,
    by_form: HashMap<String, Unit>,
}

static UNITS: LazyLock<Units> = LazyLock::new(|| {
    let mut rows: Vec<(String, bool, String, Unit)> = Vec::new();
    let mut minors: HashMap<String, String> = HashMap::new();
    for line in include_str!("../data/ru_units.tsv").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 19 {
            continue;
        }
        let forms: [String; 12] = std::array::from_fn(|i| f[6 + i].to_string());
        let unit = Unit {
            gender: Gender::from_tag(f[3]),
            animate: f[2] == "человек",
            suffix: f[5].to_string(),
            forms,
            paucal: f[18].to_string(),
            minor: None,
        };
        if let Some(major) = f[0].strip_prefix("minor:") {
            minors.insert(major.to_string(), f[2].to_string());
            rows.push((format!("minor:{major}"), false, f[2].to_string(), unit));
        } else {
            rows.push((f[0].to_string(), f[1] == "1", f[2].to_string(), unit));
        }
    }
    let mut by_key = HashMap::new();
    let mut by_key_exact = HashMap::new();
    let mut by_form = HashMap::new();
    for (key, exact, lemma, mut unit) in rows {
        if key.starts_with("minor:") {
            continue;
        }
        unit.minor = minors.get(&lemma).cloned();
        for form in unit.forms.iter().chain([&unit.paucal]) {
            if form.chars().all(|c| c.is_alphabetic() || c == '-') {
                by_form.entry(form.clone()).or_insert_with(|| unit.clone());
            }
        }
        if exact {
            by_key_exact.insert(key, unit);
        } else {
            by_key.insert(key, unit);
        }
    }
    Units { by_key, by_key_exact, by_form }
});

fn minor_unit(name: &str) -> Option<Unit> {
    UNITS.by_form.get(name).cloned()
}

fn unit_for(token: &str) -> Option<Unit> {
    UNITS.by_key_exact.get(token).or_else(|| UNITS.by_key.get(&token.to_lowercase())).cloned()
}

/// Сколько «штук» называет число для формы существительного.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Count {
    One,
    Few,
    Many,
    Fraction,
}

fn count_of(n: &BigInt) -> Count {
    let last2 = (n % BigInt::from(100)).to_string().parse::<i64>().unwrap_or(0).abs();
    let last = last2 % 10;
    if (11..=14).contains(&last2) {
        Count::Many
    } else if last == 1 {
        Count::One
    } else if (2..=4).contains(&last) {
        Count::Few
    } else {
        Count::Many
    }
}

fn unit_form(u: &Unit, count: Count, case: Case) -> String {
    let form = match (count, case.direct()) {
        (Count::Fraction, _) => u.forms[1].clone(),
        (Count::One, _) => u.forms[case.index()].clone(),
        (Count::Few, true) => u.paucal.clone(),
        (Count::Many, true) => u.forms[7].clone(),
        (_, false) => u.forms[6 + case.index()].clone(),
    };
    if u.suffix.is_empty() { form } else { format!("{form} {}", u.suffix) }
}

// ── Числа ─────────────────────────────────────────────────────────────────

fn parse_int(s: &str) -> Option<BigInt> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// «три целых пять десятых» в нужном падеже (дробная часть десятичной записи).
fn decimal_words(whole: &BigInt, frac: &str, case: Case) -> String {
    let f = parse_int(frac).unwrap_or_default();
    let denom = BigInt::from(10u32).pow(frac.len() as u32);
    format!("{} {} {}", cardinal(whole, case, Gender::Fem, false), whole_word(whole, case), fraction_tail(&f, &denom, case))
}

fn whole_word(whole: &BigInt, case: Case) -> &'static str {
    let one = count_of(whole) == Count::One;
    match (case, one) {
        (Case::Nom, true) => "целая",
        (Case::Acc, true) => "целую",
        (_, true) => "целой",
        (Case::Nom | Case::Acc | Case::Gen | Case::Loc, false) => "целых",
        (Case::Dat, false) => "целым",
        (Case::Ins, false) => "целыми",
    }
}

/// Числитель и знаменатель дроби: «пять десятых», «одна вторая», «трёх четвёртых».
fn fraction_tail(num: &BigInt, denom: &BigInt, case: Case) -> String {
    let one = count_of(num) == Count::One;
    let den = if one {
        ordinal(denom, case, Gender::Fem, false)
    } else if case.direct() {
        ordinal(denom, Case::Gen, Gender::Fem, true)
    } else {
        ordinal(denom, case, Gender::Fem, true)
    };
    format!("{} {den}", cardinal(num, case, Gender::Fem, false))
}

/// Римское число: «XIX» → 19; `None` — не римское или с ошибкой записи.
pub fn roman(s: &str) -> Option<u32> {
    let value = |c: char| match c {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        'D' => Some(500),
        'M' => Some(1000),
        _ => None,
    };
    let vals: Option<Vec<u32>> = s.chars().map(value).collect();
    let vals = vals?;
    let mut total = 0i64;
    for (i, v) in vals.iter().enumerate() {
        if vals.get(i + 1).is_some_and(|n| n > v) {
            total -= *v as i64;
        } else {
            total += *v as i64;
        }
    }
    let n = u32::try_from(total).ok().filter(|n| *n > 0)?;
    (to_roman(n) == s).then_some(n)
}

fn to_roman(mut n: u32) -> String {
    let table = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut out = String::new();
    for (v, s) in table {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

// ── Контекст ──────────────────────────────────────────────────────────────

static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\-]+").expect("word"));

/// Падеж числа по предлогу слева (до трёх слов: «в зависимости от»); `None` — предлога нет.
fn prep_case(before: &str) -> Option<Case> {
    let tail: String = before.chars().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect();
    if tail.trim_end().ends_with(|c: char| ".!?…".contains(c)) {
        return None;
    }
    let words: Vec<String> = WORD.find_iter(&tail).map(|m| m.as_str().to_lowercase()).collect();
    if !tail.trim_end().ends_with(|c: char| c.is_alphabetic()) {
        return None;
    }
    for n in (1..=3).rev() {
        if words.len() < n {
            continue;
        }
        let phrase = words[words.len() - n..].join(" ");
        if let Some((_, tag)) = PREP_CASE.iter().find(|(p, _)| *p == phrase) {
            return Some(Case::from_tag(tag));
        }
    }
    None
}

fn last_word(before: &str) -> Option<String> {
    let trimmed = before.trim_end();
    if !trimmed.ends_with(|c: char| c.is_alphabetic()) || trimmed.len() == before.len() && !before.is_empty() && !before.ends_with(char::is_whitespace) {
        return None;
    }
    WORD.find_iter(trimmed).last().map(|m| m.as_str().to_string())
}

fn next_word(after: &str) -> Option<String> {
    let t = after.trim_start();
    if t.len() == after.len() {
        return None;
    }
    WORD.find(t).filter(|m| m.start() == 0).map(|m| m.as_str().to_lowercase())
}

/// Род числа по следующему слову, когда он важен (1, 2 и составные на них) — без морфологии, по окончанию.
fn gender_from_next(n: &BigInt, case: Case, next: Option<&str>) -> (Gender, bool) {
    let Some(w) = next else { return (Gender::Masc, false) };
    if let Some(u) = UNITS.by_form.get(w) {
        return (u.gender, u.animate);
    }
    let ends = |set: &[&str]| set.iter().any(|e| w.ends_with(e));
    let g = match (count_of(n), case) {
        (Count::One, Case::Nom) if ends(&["а", "я"]) && !w.ends_with("ья") => Gender::Fem,
        (Count::One, Case::Nom) if ends(&["о", "е", "мя"]) => Gender::Neut,
        (Count::One, Case::Acc) if ends(&["у", "ю"]) => Gender::Fem,
        (Count::One, Case::Acc) if ends(&["о", "е"]) => Gender::Neut,
        (Count::Few, Case::Nom | Case::Acc) | (Count::One, Case::Gen) if ends(&["ы", "и"]) => Gender::Fem,
        (Count::One, Case::Ins) if ends(&["ой", "ей"]) => Gender::Fem,
        _ => Gender::Masc,
    };
    (g, false)
}

// ── Замены ────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Change {
    pub from: String,
    pub to: String,
    pub rule: &'static str,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Normalized {
    pub text: String,
    pub changes: Vec<Change>,
    /// Что осталось нечитаемым: цифры, которые ни одно правило не взяло, и слова латиницей.
    pub unknown: Vec<String>,
}

type Rule = fn(&regex::Captures, &str, &str) -> Option<String>;

/// Один проход правила по тексту: каждое совпадение заменяется тем, что вернуло правило (с контекстом до и после).
fn pass(text: &str, re: &Regex, rule_name: &'static str, rule: Rule, changes: &mut Vec<Change>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for c in re.captures_iter(text) {
        let m = c.get(0).expect("whole");
        let before = &text[..m.start()];
        let after = &text[m.end()..];
        if before.ends_with(|ch: char| ch.is_alphanumeric()) || after.starts_with(|ch: char| ch.is_ascii_digit()) {
            continue;
        }
        let Some(to) = rule(&c, &out_with(&out, &text[last..m.start()]), after) else { continue };
        let to = with_stop(m.as_str(), after, to);
        out.push_str(&text[last..m.start()]);
        changes.push(Change { from: m.as_str().to_string(), to: to.clone(), rule: rule_name });
        out.push_str(&to);
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

/// An abbreviation's dot a rule read out («руб.», «г.») that also ended the sentence comes back as its full stop.
fn with_stop(consumed: &str, after: &str, words: String) -> String {
    let rest = after.trim_start_matches([' ', '\u{a0}', '\u{202f}']);
    let ends = rest.is_empty() || rest.starts_with('\n') || after.len() > rest.len() && rest.starts_with(char::is_uppercase);
    if consumed.ends_with('.') && ends && !words.ends_with('.') { format!("{words}.") } else { words }
}

fn out_with(done: &str, gap: &str) -> String {
    format!("{done}{gap}")
}

fn cap_like(src_before: &str, words: String) -> String {
    let start = src_before.trim_end().is_empty() || src_before.trim_end().ends_with(|c: char| ".!?…\n".contains(c));
    if start {
        let mut ch = words.chars();
        match ch.next() {
            Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
            None => words,
        }
    } else {
        words
    }
}

static DATE_NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{1,2})\.(\d{1,2})\.(\d{4}|\d{2})(\s*(?:года|г\.))?").expect("date"));
/// Day and month without a year: the month in two digits, since a Russian decimal is written with a comma.
static DATE_DM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{1,2})\.(0[1-9]|1[0-2])").expect("date dm"));
static DATE_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{1,2})\s+(января|февраля|марта|апреля|мая|июня|июля|августа|сентября|октября|ноября|декабря)(?:\s+(\d{3,4})(\s*(?:года|г\.))?)?").expect("date word")
});
static YEAR_RANGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(с|от)\s+(\d{3,4})\s+(по|до)\s+(\d{3,4})\s+(год|года|году|г\.)").expect("year range"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{1,4})(?:\s*[-–—]\s*(\d{1,4}))?(?:-(х|е|ые|ых))?\s*(годами|годам|годах|годов|годом|годе|году|года|годы|год|гг\.|г\.)").expect("year")
});
static DECADE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{3,4})-(х|е|ые|ых)").expect("decade"));
static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([01]?\d|2[0-3]):([0-5]\d)").expect("time"));
/// A time written with a dot, as Russian also writes it («в 18.30»): only after a word that asks for a time.
static TIME_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([01]?\d|2[0-3])\.([0-5]\d)").expect("time dot"));
const TIME_WORDS: [&str; 8] = ["в", "во", "до", "с", "со", "к", "после", "около"];
static SCORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{1,3}):(\d{1,3})").expect("score"));
static CENTURY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([IVXLC]{1,7})\s+(векам|веками|веках|веков|веком|веке|веку|века|век|вв\.|в\.)").expect("century"));
static ROMAN_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([А-ЯЁ][а-яё]+)\s+([IVX]{1,5})\b").expect("roman name"));
static ORDINAL_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d+)-(?:ы|о|е|и|а)?(ыми|ими|ого|его|ому|ему|ыми|ую|ая|ое|ые|ых|ым|ой|ий|ый|го|му|ми|й|я|е|х|м|ю)\b").expect("ordinal")
});
static CURRENCY_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([$€£¥₽₴₸₺₹])\s?(\d+(?:[.,]\d+)?)").expect("currency"));
static FRACTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)/(\d+)").expect("fraction"));
static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([-−])?(\d{1,3}(?:[ \u{a0}\u{202f}]\d{3})+|\d+)(?:[.,](\d+))?(?:\s*[-–—]\s*(\d+)(?:[.,](\d+))?)?").expect("number")
});
static LATIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z][A-Za-z'’\-]*").expect("latin"));

fn day_month_year(day: u32, month: usize, year: Option<i64>, case: Case) -> String {
    let mut s = format!("{} {}", ordinal(&BigInt::from(day), case, Gender::Neut, false), MONTHS_GEN[month - 1]);
    if let Some(y) = year {
        s.push_str(&format!(" {} года", ordinal(&BigInt::from(y), Case::Gen, Gender::Masc, false)));
    }
    s
}

fn rule_date_num(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let day: u32 = c[1].parse().ok()?;
    let month: usize = c[2].parse().ok()?;
    if !(1..=31).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }
    let mut year: i64 = c[3].parse().ok()?;
    if c[3].len() == 2 {
        year += if year < 50 { 2000 } else { 1900 };
    }
    let case = prep_case(before).map(|c| if c == Case::Acc { Case::Nom } else { c }).unwrap_or(Case::Gen);
    let case = if prep_case(before).is_none() && before.trim_end().is_empty() { Case::Nom } else { case };
    Some(cap_like(before, day_month_year(day, month, Some(year), case)))
}

fn rule_date_dm(c: &regex::Captures, before: &str, after: &str) -> Option<String> {
    let day: u32 = c[1].parse().ok()?;
    let month: usize = c[2].parse().ok()?;
    let version = before.ends_with(['.', ',']) || after.starts_with(['.', ',']) && after[1..].starts_with(|ch: char| ch.is_ascii_digit());
    let at = last_word(before).is_some_and(|w| matches!(w.to_lowercase().as_str(), "в" | "во"));
    if !(1..=31).contains(&day) || version || at || after.starts_with('%') || unit_after(after).is_some() {
        return None;
    }
    let case = prep_case(before).map(|c| if c == Case::Acc { Case::Nom } else { c }).unwrap_or(Case::Gen);
    let case = if prep_case(before).is_none() && before.trim_end().is_empty() { Case::Nom } else { case };
    Some(cap_like(before, day_month_year(day, month, None, case)))
}

fn rule_date_word(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let day: u32 = c[1].parse().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let month = MONTHS_GEN.iter().position(|m| *m == &c[2])? + 1;
    let year = c.get(3).and_then(|y| y.as_str().parse::<i64>().ok());
    let case = match prep_case(before) {
        Some(Case::Acc) | None => if before.trim_end().is_empty() { Case::Nom } else { Case::Gen },
        Some(c) => c,
    };
    Some(cap_like(before, day_month_year(day, month, year, case)))
}

fn year_noun_case(noun: &str, before: &str) -> (Case, bool, &'static str) {
    let loc_prep = matches!(last_word(before).map(|w| w.to_lowercase()).as_deref(), Some("в" | "во" | "о" | "об" | "при"));
    match noun {
        "год" => (Case::Nom, false, "год"),
        "года" => (Case::Gen, false, "года"),
        "году" => if loc_prep { (Case::Loc, false, "году") } else { (Case::Dat, false, "году") },
        "годом" => (Case::Ins, false, "годом"),
        "годе" => (Case::Loc, false, "годе"),
        "годы" => (Case::Nom, true, "годы"),
        "годов" => (Case::Gen, true, "годов"),
        "годам" => (Case::Dat, true, "годам"),
        "годами" => (Case::Ins, true, "годами"),
        "годах" => (Case::Loc, true, "годах"),
        "гг." => if loc_prep { (Case::Loc, true, "годах") } else { (Case::Gen, true, "годов") },
        _ => match last_word(before).map(|w| w.to_lowercase()).as_deref() {
            Some("в" | "во" | "о" | "об" | "при") => (Case::Loc, false, "году"),
            Some("к" | "по") => (Case::Dat, false, "году"),
            _ => (Case::Gen, false, "года"),
        },
    }
}

fn rule_year_range(c: &regex::Captures, _before: &str, _after: &str) -> Option<String> {
    let (case_noun, _, noun) = year_noun_case(&c[5].to_lowercase(), &format!("{} {} ", &c[3], ""));
    let a: i64 = c[2].parse().ok()?;
    let b: i64 = c[4].parse().ok()?;
    let first = ordinal(&BigInt::from(a), Case::Gen, Gender::Masc, false);
    let second_case = if c[3].to_lowercase() == "по" && noun == "год" { Case::Nom } else { case_noun };
    let second = ordinal(&BigInt::from(b), second_case, Gender::Masc, false);
    Some(format!("{} {first} {} {second} {noun}", &c[1], &c[3]))
}

fn rule_year(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let n: i64 = c[1].parse().ok()?;
    let noun = c[4].to_lowercase();
    let decade = c.get(3).is_some();
    let strong = matches!(noun.as_str(), "году" | "годом" | "годе" | "гг." | "годах" | "годам" | "годами") || decade;
    if n < 1000 && !strong && !(noun == "г." && prep_case(before).is_some()) {
        return None;
    }
    let (case, _, noun_out) = year_noun_case(&noun, before);
    let words = |v: i64| ordinal(&BigInt::from(v), case, Gender::Masc, decade);
    let mut s = words(n);
    if let Some(second) = c.get(2).and_then(|m| m.as_str().parse::<i64>().ok()) {
        s = format!("{s} — {}", words(second));
    }
    Some(cap_like(before, format!("{s} {noun_out}")))
}

fn rule_decade(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let n: i64 = c[1].parse().ok()?;
    let case = match &c[2] {
        "е" | "ые" => Case::Nom,
        _ => prep_case(before).filter(|c| *c != Case::Acc).unwrap_or(Case::Gen),
    };
    Some(cap_like(before, ordinal(&BigInt::from(n), case, Gender::Masc, true)))
}

fn hours_minutes(h: u32, m: u32, case: Case) -> String {
    let hb = BigInt::from(h);
    if case.direct() {
        let minutes = match m {
            0 => "ноль-ноль".to_string(),
            1..=9 => format!("ноль {}", cardinal(&BigInt::from(m), Case::Nom, Gender::Fem, false)),
            _ => cardinal(&BigInt::from(m), Case::Nom, Gender::Fem, false),
        };
        return format!("{} {minutes}", cardinal(&hb, Case::Nom, Gender::Masc, false));
    }
    let hour = unit_for("ч").expect("час in units");
    let minute = unit_for("мин").expect("минута in units");
    let mut s = format!("{} {}", cardinal(&hb, case, Gender::Masc, false), unit_form(&hour, count_of(&hb), case));
    if m > 0 {
        let mb = BigInt::from(m);
        s.push_str(&format!(" {} {}", cardinal(&mb, case, Gender::Fem, false), unit_form(&minute, count_of(&mb), case)));
    }
    s
}

fn rule_time(c: &regex::Captures, before: &str, after: &str) -> Option<String> {
    if after.starts_with(':') {
        return None;
    }
    let h: u32 = c[1].parse().ok()?;
    let m: u32 = c[2].parse().ok()?;
    let case = prep_case(before).unwrap_or(Case::Nom);
    Some(cap_like(before, hours_minutes(h, m, case)))
}

fn rule_time_dot(c: &regex::Captures, before: &str, after: &str) -> Option<String> {
    let asks = last_word(before).is_some_and(|w| TIME_WORDS.contains(&w.to_lowercase().as_str()));
    if !asks || after.starts_with(['.', ',']) && after[1..].starts_with(|ch: char| ch.is_ascii_digit()) || after.starts_with('%') || unit_after(after).is_some() {
        return None;
    }
    rule_time(c, before, after)
}

fn rule_score(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let a = parse_int(&c[1])?;
    let b = parse_int(&c[2])?;
    Some(cap_like(before, format!("{} — {}", cardinal(&a, Case::Nom, Gender::Masc, false), cardinal(&b, Case::Nom, Gender::Masc, false))))
}

fn rule_century(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let n = roman(&c[1])?;
    let noun = &c[2];
    let loc_prep = matches!(last_word(before).map(|w| w.to_lowercase()).as_deref(), Some("в" | "во" | "о" | "об" | "при"));
    let (case, out) = match noun {
        "век" => (Case::Nom, "век"),
        "века" => (Case::Gen, "века"),
        "веку" => (Case::Dat, "веку"),
        "веком" => (Case::Ins, "веком"),
        "веке" => (Case::Loc, "веке"),
        "веков" => (Case::Gen, "веков"),
        "векам" => (Case::Dat, "векам"),
        "веками" => (Case::Ins, "веками"),
        "веках" => (Case::Loc, "веках"),
        "вв." => if loc_prep { (Case::Loc, "веках") } else { (Case::Gen, "веков") },
        _ => if loc_prep { (Case::Loc, "веке") } else { (Case::Gen, "века") },
    };
    Some(cap_like(before, format!("{} {out}", ordinal(&BigInt::from(n), case, Gender::Masc, false))))
}

fn rule_roman_name(c: &regex::Captures, _before: &str, _after: &str) -> Option<String> {
    let n = roman(&c[2])?;
    let name = &c[1];
    let gender = if name.ends_with('а') || name.ends_with('я') { Gender::Fem } else { Gender::Masc };
    Some(format!("{name} {}", ordinal(&BigInt::from(n), Case::Nom, gender, false)))
}

fn rule_ordinal_suffix(c: &regex::Captures, before: &str, after: &str) -> Option<String> {
    let n = parse_int(&c[1])?;
    let suffix = &c[2];
    let next = next_word(after);
    let loc_prep = matches!(last_word(before).map(|w| w.to_lowercase()).as_deref(), Some("в" | "во" | "о" | "об" | "на" | "при"));
    let (case, gender, plural) = match suffix {
        "й" | "ий" | "ый" => (Case::Nom, Gender::Masc, false),
        "я" | "ая" => (Case::Nom, Gender::Fem, false),
        "е" | "ое" => match next.as_deref() {
            Some(w) if w.ends_with('ы') || w.ends_with('и') => (Case::Nom, Gender::Masc, true),
            _ => (Case::Nom, Gender::Neut, false),
        },
        "ые" => (Case::Nom, Gender::Masc, true),
        "го" | "ого" | "его" => (Case::Gen, Gender::Masc, false),
        "му" | "ому" | "ему" => (Case::Dat, Gender::Masc, false),
        "м" => if loc_prep { (Case::Loc, Gender::Masc, false) } else { (Case::Ins, Gender::Masc, false) },
        "ым" => (Case::Ins, Gender::Masc, false),
        "ой" => match prep_case(before) {
            Some(Case::Loc) => (Case::Loc, Gender::Fem, false),
            Some(Case::Dat) => (Case::Dat, Gender::Fem, false),
            Some(Case::Ins) => (Case::Ins, Gender::Fem, false),
            _ => if loc_prep { (Case::Loc, Gender::Fem, false) } else { (Case::Gen, Gender::Fem, false) },
        },
        "ю" | "ую" => (Case::Acc, Gender::Fem, false),
        "х" | "ых" => if loc_prep { (Case::Loc, Gender::Masc, true) } else { (Case::Gen, Gender::Masc, true) },
        "ми" | "ыми" | "ими" => (Case::Ins, Gender::Masc, true),
        _ => return None,
    };
    Some(cap_like(before, ordinal(&n, case, gender, plural)))
}

fn rule_currency_prefix(c: &regex::Captures, before: &str, _after: &str) -> Option<String> {
    let unit = unit_for(&c[1])?;
    let raw = &c[2];
    let case = prep_case(before).unwrap_or(Case::Nom);
    Some(cap_like(before, amount_with_unit(raw, &unit, case)))
}

/// Сумма или величина с единицей: «сто рублей пятьдесят копеек», «три целых пять десятых километра».
fn amount_with_unit(raw: &str, unit: &Unit, case: Case) -> String {
    let (whole, frac) = match raw.split_once([',', '.']) {
        Some((w, f)) => (w, Some(f)),
        None => (raw, None),
    };
    let w = parse_int(whole).unwrap_or_default();
    match frac {
        Some(f) if unit.minor.is_some() && f.len() == 2 => {
            let minor = minor_unit(unit.minor.as_deref().unwrap_or("")).expect("minor unit in table");
            let cents = parse_int(f).unwrap_or_default();
            let mut s = format!("{} {}", cardinal(&w, case, unit.gender, unit.animate), unit_form(unit, count_of(&w), case));
            if cents != BigInt::from(0) {
                s.push_str(&format!(" {} {}", cardinal(&cents, case, minor.gender, false), unit_form(&minor, count_of(&cents), case)));
            }
            s
        }
        Some(f) => format!("{} {}", decimal_words(&w, f, case), unit_form(unit, Count::Fraction, case)),
        None => format!("{} {}", cardinal(&w, case, unit.gender, unit.animate), unit_form(unit, count_of(&w), case)),
    }
}

fn rule_fraction(c: &regex::Captures, before: &str, after: &str) -> Option<String> {
    if after.starts_with('/') || before.ends_with('/') {
        return None;
    }
    let a = parse_int(&c[1])?;
    let b = parse_int(&c[2])?;
    if b == BigInt::from(0) {
        return None;
    }
    let case = prep_case(before).unwrap_or(Case::Nom);
    Some(cap_like(before, fraction_tail(&a, &b, case)))
}

/// Единица сразу после числа: самый длинный токен из таблицы (`км/ч`, `°C`, `руб.`).
fn unit_after(after: &str) -> Option<(Unit, usize)> {
    let skip = after.len() - after.trim_start_matches([' ', '\u{a0}', '\u{202f}']).len();
    if skip > 1 {
        return None;
    }
    let rest = &after[skip..];
    let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let token = &rest[..token_end];
    let mut candidates: Vec<&str> = Vec::new();
    let mut t = token;
    loop {
        candidates.push(t);
        let trimmed = t.trim_end_matches([',', ';', ':', '!', '?', ')', '»', '"', '.']);
        let trimmed = if trimmed.len() == t.len() { t.strip_suffix('.').unwrap_or(t) } else { trimmed };
        if trimmed.len() == t.len() || trimmed.is_empty() {
            break;
        }
        t = trimmed;
    }
    for cand in candidates {
        let with_dot = cand.ends_with('.');
        let key_used = if with_dot && unit_for(cand).is_none() { cand.trim_end_matches('.') } else { cand };
        if let Some(u) = unit_for(key_used) {
            let next = rest[key_used.len()..].chars().next();
            if next.is_some_and(|c| c.is_alphanumeric()) {
                continue;
            }
            return Some((u, skip + key_used.len()));
        }
    }
    None
}

fn label_before(before: &str) -> Option<Gender> {
    let w = last_word(before)?.to_lowercase();
    ORDINAL_LABELS.iter().find(|(l, _)| *l == w).map(|(_, g)| *g)
}

/// Число без особого правила: падеж — по предлогу, род — по следующему слову или единице; диапазоны и минус.
fn number_pass(text: &str, changes: &mut Vec<Change>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut pos = 0;
    let mut prev: Option<(usize, Case)> = None;
    while let Some(c) = NUMBER.captures_at(text, pos) {
        let m = c.get(0).expect("whole");
        pos = m.end();
        let before_all = &text[..m.start()];
        if before_all.ends_with(|ch: char| ch.is_alphabetic() || ch == '_') {
            continue;
        }
        let neg = c.get(1).is_some() && (before_all.is_empty() || before_all.ends_with(char::is_whitespace) || before_all.ends_with('('));
        let start = if c.get(1).is_some() && !neg { m.start() + 1 } else { m.start() };
        let after_num = &text[m.end()..];
        let before = format!("{out}{}", &text[last..start]);
        let raw_whole = &c[2];
        if raw_whole.len() > 1 && raw_whole.starts_with('0') && c.get(3).is_none() && c.get(4).is_none() {
            let digits: Vec<String> = raw_whole.chars().map(|d| cardinal(&BigInt::from(d.to_digit(10).unwrap_or(0)), Case::Nom, Gender::Masc, false)).collect();
            push_change(&mut out, &text[last..start], &text[start..m.end()], digits.join(" "), "digits", changes);
            last = m.end();
            continue;
        }
        let n = parse_int(raw_whole).unwrap_or_default();
        let next = next_word(after_num);
        let coordinated = prev.filter(|(end, _)| COORDINATED.is_match(&before[*end..])).map(|(_, c)| c);
        let case = match (prep_case(&before).or(coordinated), next.as_deref()) {
            (Some(c), _) => c,
            (None, Some(w)) if count_of(&n) == Count::One && c.get(3).is_none() && (w.ends_with('у') || w.ends_with('ю')) && !UNITS.by_form.contains_key(w) => Case::Acc,
            _ => Case::Nom,
        };
        let unit = unit_after(after_num);
        let n = if neg { -n } else { n };
        let words = if let Some(range_end) = c.get(4) {
            let n2 = parse_int(range_end.as_str()).unwrap_or_default();
            match &unit {
                Some((u, _)) => format!("{} — {}", cardinal(&n, case, u.gender, u.animate), amount_with_unit(range_end.as_str(), u, case)),
                None => {
                    let (g, anim) = gender_from_next(&n2, case, next_word(after_num).as_deref());
                    format!("{} — {}", cardinal(&n, case, g, anim), cardinal(&n2, case, g, anim))
                }
            }
        } else if let Some((u, _)) = &unit {
            let raw = match c.get(3) {
                Some(f) => format!("{raw_whole},{}", f.as_str()),
                None => raw_whole.to_string(),
            };
            let s = amount_with_unit(&raw, u, case);
            if neg { format!("минус {s}") } else { s }
        } else if let Some(f) = c.get(3) {
            decimal_words(&n, f.as_str(), case)
        } else if let Some(g) = label_before(&before).filter(|_| prep_case(&before).is_none()) {
            ordinal(&n, Case::Nom, g, false)
        } else if (1000..=2100).contains(&n.to_string().parse::<i64>().unwrap_or(0))
            && matches!(last_word(&before).map(|w| w.to_lowercase()).as_deref(), Some("с" | "от"))
            && YEAR_AHEAD.is_match(after_num)
        {
            ordinal(&n, Case::Gen, Gender::Masc, false)
        } else {
            let (g, anim) = gender_from_next(&n, case, next_word(after_num).as_deref());
            cardinal(&n, case, g, anim)
        };
        let consumed_unit = unit.as_ref().map(|(_, len)| *len).unwrap_or(0);
        let end = m.end() + consumed_unit;
        let glued = text[end..].starts_with(|ch: char| ch.is_alphabetic());
        let words = if glued { format!("{words} ") } else { words };
        let words = with_stop(&text[start..end], &text[end..], words);
        push_change(&mut out, &text[last..start], &text[start..end], cap_like(&before, words), "number", changes);
        prev = Some((out.len(), case));
        last = end;
        pos = end;
    }
    out.push_str(&text[last..]);
    out
}

static COORDINATED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:,\s*|\s+(?:и|или|либо)\s+)$").expect("coordinated"));
static YEAR_AHEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s+(?:по|до)\s+(?:\d{3,4}|[а-яё ]+)\s+(?:год|года|году|г\.)").expect("year ahead"));

fn push_change(out: &mut String, gap: &str, from: &str, to: String, rule: &'static str, changes: &mut Vec<Change>) {
    out.push_str(gap);
    changes.push(Change { from: from.to_string(), to: to.clone(), rule });
    out.push_str(&to);
}

/// Markup of the synthesis engines (Higgs tags, Speech Studio's pauses and rate, CosyVoice-style breaks) and a
/// caller's own placeholders of kept markup (a private-use mark around a number, as Dub's text adapter makes): the
/// rules must not read them.
static MARKUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\|[^|\n]*\|>|\[\[[^\[\]]*\]\]|\[\s*pause[^\]]*\]|\[\s*rate\s*:[^\]]*\]|sil\s*<\[[^\]]*\]>|<\[[^\]]*\]>|\u{E000}\d+\u{E001}")
        .expect("markup")
});

/// Разметка синтеза и ударения — в приватные символы, чтобы правила их не трогали; цифр в заместителях нет.
fn protect(text: &str) -> (String, Vec<String>) {
    let mut kept = Vec::new();
    let out = MARKUP.replace_all(text, |c: &regex::Captures| {
        kept.push(c[0].to_string());
        format!("\u{E000}{}\u{E001}", char::from_u32(0xF0000 + kept.len() as u32 - 1).unwrap_or('\u{E002}'))
    });
    (out.into_owned(), kept)
}

fn restore(text: &str, kept: &[String]) -> String {
    let mut out = text.to_string();
    for (i, k) in kept.iter().enumerate() {
        let mark = format!("\u{E000}{}\u{E001}", char::from_u32(0xF0000 + i as u32).unwrap_or('\u{E002}'));
        out = out.replace(&mark, k);
    }
    out
}

static EMAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,}").expect("email"));
static URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:https?://|www\.)[^\s,;()<>«»]*[^\s,;()<>«».!?:]|\b[a-z0-9\-]+(?:\.[a-z0-9\-]+)*\.(?:com|ru|org|net|io|ai|dev|app|studio|info|me|tv|co|edu|gov|su|by|kz|ua)\b(?:/(?:[^\s,;()<>«»]*[^\s,;()<>«».!?:])?)?").expect("url")
});
static PHONE_RU: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\+7|8)[\s\-]?\(?(\d{3})\)?[\s\-]?(\d{3})[\s\-]?(\d{2})[\s\-]?(\d{2})").expect("phone"));
static PHONE_INTL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\+(\d{1,3})[\s\-]?\(?(\d{1,4})\)?((?:[\s\-]?\d{2,4}){2,4})").expect("phone"));
static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([A-Z]{1,4})-?(\d{2,})\b").expect("code"));

/// Латинские буквы так, как их называют по-русски, читая код или аббревиатуру.
fn latin_letter(c: char) -> &'static str {
    match c.to_ascii_uppercase() {
        'A' => "эй", 'B' => "би", 'C' => "си", 'D' => "ди", 'E' => "и", 'F' => "эф", 'G' => "джи", 'H' => "эйч", 'I' => "ай",
        'J' => "джей", 'K' => "кей", 'L' => "эл", 'M' => "эм", 'N' => "эн", 'O' => "оу", 'P' => "пи", 'Q' => "кью", 'R' => "ар",
        'S' => "эс", 'T' => "ти", 'U' => "ю", 'V' => "ви", 'W' => "дабл-ю", 'X' => "экс", 'Y' => "уай", _ => "зед",
    }
}

fn digit_words(digits: &str) -> String {
    digits.chars().filter_map(|c| c.to_digit(10)).map(|d| cardinal(&BigInt::from(d), Case::Nom, Gender::Masc, false)).collect::<Vec<_>>().join(" ")
}

/// Группа цифр телефона: с нуля — цифрами, иначе числом («сорок пять», «восемьсот»).
fn phone_group(g: &str) -> String {
    if g.len() > 1 && g.starts_with('0') {
        return digit_words(g);
    }
    g.parse::<BigInt>().map(|n| cardinal(&n, Case::Nom, Gender::Masc, false)).unwrap_or_else(|_| digit_words(g))
}

fn rule_phone_ru(c: &regex::Captures, _: &str, _: &str) -> Option<String> {
    let head = if &c[1] == "+7" { "плюс семь".to_string() } else { "восемь".to_string() };
    Some(std::iter::once(head).chain((2..=5).map(|i| phone_group(&c[i]))).collect::<Vec<_>>().join(", "))
}

fn rule_phone_intl(c: &regex::Captures, _: &str, _: &str) -> Option<String> {
    let mut parts = vec![format!("плюс {}", phone_group(&c[1])), phone_group(&c[2])];
    parts.extend(c[3].split([' ', '-']).filter(|g| !g.is_empty()).map(phone_group));
    Some(parts.join(", "))
}

/// Знаки адреса словами: точка, слэш, дефис, подчёркивание; латинские слова остаются как есть.
fn spoken_address(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '@' => out.push_str(" собака "),
            '.' => out.push_str(" точка "),
            '/' => out.push_str(" слэш "),
            '-' => out.push_str(" дефис "),
            '_' => out.push_str(" подчёркивание "),
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn rule_email(c: &regex::Captures, _: &str, _: &str) -> Option<String> {
    Some(spoken_address(&c[0]))
}

/// Адрес сайта без схемы и www, без параметров и якоря; точка в конце предложения — не часть адреса.
fn rule_url(c: &regex::Captures, _: &str, _: &str) -> Option<String> {
    let whole = &c[0];
    let lower = whole.to_ascii_lowercase();
    let mut rest = &whole[lower.find("://").map_or(0, |i| i + 3)..];
    if rest.to_ascii_lowercase().starts_with("www.") {
        rest = &rest[4..];
    }
    let rest = rest.split(['?', '#']).next().unwrap_or("").trim_end_matches(['.', '!', '?', '/', ':']);
    (!rest.is_empty()).then(|| spoken_address(rest))
}

fn rule_code(c: &regex::Captures, _: &str, _: &str) -> Option<String> {
    let letters = c[1].chars().map(latin_letter).collect::<Vec<_>>().join(" ");
    Some(format!("{letters}, {}", digit_words(&c[2])))
}

/// Группы правил, которые проект может выключить: что из них не читать словами.
pub const GROUPS: [&str; 8] = ["numbers", "dates", "time", "money", "phones", "web", "codes", "roman"];

fn group_of(rule: &str) -> &'static str {
    match rule {
        "date" | "year" | "century" => "dates",
        "time" | "score" => "time",
        "money" => "money",
        "phone" => "phones",
        "email" | "url" => "web",
        "code" => "codes",
        "roman" => "roman",
        _ => "numbers",
    }
}

/// Нормализовать русский текст: что заменено каким правилом и что осталось нечитаемым.
pub fn normalize(text: &str) -> Normalized {
    normalize_with(text, &[])
}

/// То же без групп правил `off` (из `GROUPS`): их совпадения остаются как написаны.
pub fn normalize_with(text: &str, off: &[String]) -> Normalized {
    let on = |rule: &str| !off.iter().any(|g| g == group_of(rule));
    let pass = |t: &str, re: &Regex, name: &'static str, rule: Rule, changes: &mut Vec<Change>| if on(name) { pass(t, re, name, rule, changes) } else { t.to_string() };
    let (mut t, kept) = protect(text);
    let mut changes = Vec::new();
    t = pass(&t, &EMAIL, "email", rule_email, &mut changes);
    t = pass(&t, &URL, "url", rule_url, &mut changes);
    t = pass(&t, &PHONE_RU, "phone", rule_phone_ru, &mut changes);
    t = pass(&t, &PHONE_INTL, "phone", rule_phone_intl, &mut changes);
    t = pass(&t, &CODE, "code", rule_code, &mut changes);
    t = pass(&t, &YEAR_RANGE, "year", rule_year_range, &mut changes);
    t = pass(&t, &DATE_NUM, "date", rule_date_num, &mut changes);
    t = pass(&t, &DATE_DM, "date", rule_date_dm, &mut changes);
    t = pass(&t, &DATE_WORD, "date", rule_date_word, &mut changes);
    t = pass(&t, &YEAR, "year", rule_year, &mut changes);
    t = pass(&t, &DECADE, "year", rule_decade, &mut changes);
    t = pass(&t, &TIME, "time", rule_time, &mut changes);
    t = pass(&t, &TIME_DOT, "time", rule_time_dot, &mut changes);
    t = pass(&t, &SCORE, "score", rule_score, &mut changes);
    t = pass(&t, &CENTURY, "century", rule_century, &mut changes);
    t = pass(&t, &ROMAN_NAME, "roman", rule_roman_name, &mut changes);
    t = pass(&t, &ORDINAL_SUFFIX, "ordinal", rule_ordinal_suffix, &mut changes);
    t = pass(&t, &CURRENCY_PREFIX, "money", rule_currency_prefix, &mut changes);
    t = pass(&t, &FRACTION, "fraction", rule_fraction, &mut changes);
    if on("number") {
        t = number_pass(&t, &mut changes);
    }
    let unknown: Vec<String> = t
        .split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_ascii_digit()) || LATIN.is_match(w))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect();
    Normalized { text: restore(&t, &kept), changes, unknown }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(n: i64) -> BigInt {
        BigInt::from(n)
    }

    #[test]
    fn phones_mail_sites_and_codes_are_read_as_people_say_them() {
        let n = |t: &str| normalize(t).text;
        assert_eq!(n("Звоните +7 (495) 123-45-67."), "Звоните плюс семь, четыреста девяносто пять, сто двадцать три, сорок пять, шестьдесят семь.");
        assert_eq!(n("или 8 800 555-35-35"), "или восемь, восемьсот, пятьсот пятьдесят пять, тридцать пять, тридцать пять");
        assert_eq!(n("+44 20 7946 0958"), "плюс сорок четыре, двадцать, семь тысяч девятьсот сорок шесть, ноль девять пять восемь");
        assert_eq!(n("пишите info@example.com, ответим"), "пишите info собака example точка com, ответим");
        assert_eq!(n("сайт https://www.speech.studio/docs?x=1."), "сайт speech точка studio слэш docs.");
        assert_eq!(n("на example.ru и всё"), "на example точка ru и всё");
        assert_eq!(n("Код заказа AB-1234"), "Код заказа эй би, один два три четыре");
    }

    #[test]
    fn a_group_turned_off_stays_as_written() {
        let off = |groups: &[&str], t: &str| normalize_with(t, &groups.iter().map(|g| g.to_string()).collect::<Vec<_>>()).text;
        assert_eq!(off(&["codes"], "Код AB-1234, 5 штук"), "Код AB-1234, пять штук");
        assert_eq!(off(&["numbers"], "Код AB-1234, 5 штук"), "Код эй би, один два три четыре, 5 штук");
        assert_eq!(off(&["web"], "пишите на info@example.com"), "пишите на info@example.com");
    }

    #[test]
    fn num2words_gives_cases_genders_and_ordinals() {
        assert_eq!(cardinal(&b(2), Case::Gen, Gender::Masc, false), "двух");
        assert_eq!(cardinal(&b(1), Case::Acc, Gender::Fem, false), "одну");
        assert_eq!(cardinal(&b(2), Case::Acc, Gender::Masc, false), "два");
        assert_eq!(cardinal(&b(21), Case::Nom, Gender::Fem, false), "двадцать одна");
        assert_eq!(ordinal(&b(2026), Case::Loc, Gender::Masc, false), "две тысячи двадцать шестом");
        assert_eq!(ordinal(&b(10), Case::Nom, Gender::Neut, false), "десятое");
        assert_eq!(ordinal(&b(1990), Case::Gen, Gender::Masc, true), "тысяча девятьсот девяностых");
        assert_eq!(ordinal(&b(10), Case::Gen, Gender::Fem, true), "десятых");
    }

    /// Расхождения с ru-normalizr, сделанные нарочно: там это его ошибки.
    const INTENDED: &[(&str, &str)] = &[
        ("Поезд уходит в 15:30.", "Поезд уходит в пятнадцать тридцать."),
        ("Работаем с 9:00 до 18:00.", "Работаем с девяти часов до восемнадцати часов."),
        ("Встреча в 7:05 утра.", "Встреча в семь ноль пять утра."),
        ("XX век был жестоким.", "Двадцатый век был жестоким."),
        ("Цена $5 за штуку.", "Цена пять долларов за штуку."),
        ("Цена 100,50 ₽.", "Цена сто рублей пятьдесят копеек."),
    ];

    #[test]
    fn the_core_reads_like_ru_normalizr() {
        let golden = include_str!("../data/ru_norm_golden.tsv");
        let mut wrong = Vec::new();
        for line in golden.lines().filter(|l| !l.is_empty()) {
            let (src, want) = line.split_once('\t').expect("tab");
            let want = INTENDED.iter().find(|(s, _)| *s == src).map(|(_, w)| *w).unwrap_or(want);
            let got = normalize(src).text;
            if got != want {
                wrong.push(format!("{src}\n  want: {want}\n  got:  {got}"));
            }
        }
        assert!(wrong.is_empty(), "{} of {}:\n{}", wrong.len(), golden.lines().count(), wrong.join("\n"));
    }

    #[test]
    fn markup_and_stress_marks_stay_and_unknown_is_listed() {
        let n = normalize("В 5 км <|emotion:awe|> [pause 300ms] [rate:1.5] з+амок, iPhone 15X");
        assert_eq!(n.text, "В пять километров <|emotion:awe|> [pause 300ms] [rate:1.5] з+амок, iPhone пятнадцать X");
        assert!(n.unknown.contains(&"iPhone".to_string()));
    }

    /// A caller's placeholder of kept markup (a private-use mark around its number) is not read as a number.
    #[test]
    fn a_placeholder_of_kept_markup_stays_as_it_is() {
        let n = normalize("\u{E000}0\u{E001} Через 5 минут \u{E000}12\u{E001}");
        assert_eq!(n.text, "\u{E000}0\u{E001} Через пять минут \u{E000}12\u{E001}");
    }

    #[test]
    fn a_day_and_month_a_typographic_minus_and_an_abbreviation_ending_a_sentence() {
        assert_eq!(normalize("Встреча 12.03 в 18:30.").text, "Встреча двенадцатого марта в восемнадцать тридцать.");
        assert_eq!(normalize("12.03 — день рождения").text, "Двенадцатое марта — день рождения");
        assert_eq!(normalize("Встреча 12 марта в 18.30.").text, "Встреча двенадцатого марта в восемнадцать тридцать.");
        assert_eq!(normalize("Работаем с 9.00 до 21.00, приём до 01.04.").text, "Работаем с девяти часов до двадцати одного часа, приём до первого апреля.");
        assert_eq!(normalize("Начало в 12.03").text, "Начало в двенадцать ноль три");
        assert_eq!(normalize("Курс 18.30 рубля").text, "Курс восемнадцать целых тридцать сотых рубля");
        for kept in ["версия 1.12.03", "12.05 кг", "рост 12.05%", "v2.10"] {
            assert!(!normalize(kept).text.contains("мая") && !normalize(kept).text.contains("октября"), "{kept}: {}", normalize(kept).text);
        }
        assert_eq!(normalize("На улице −5 °C.").text, "На улице минус пять градусов Цельсия.");
        assert_eq!(normalize("Купите за 150 руб.").text, "Купите за сто пятьдесят рублей.");
        assert_eq!(normalize("Это было в 1945 г. Потом мир изменился.").text, "Это было в тысяча девятьсот сорок пятом году. Потом мир изменился.");
        assert_eq!(normalize("Цена 150 руб. за штуку").text, "Цена сто пятьдесят рублей за штуку");
    }

    #[test]
    fn roman_numbers_are_strict() {
        assert_eq!(roman("XIX"), Some(19));
        assert_eq!(roman("IIII"), None);
        assert_eq!(roman("MCMXC"), Some(1990));
    }
}
