//! Текст, уходящий в синтез: без тегов неречевых звуков, меток спикеров, разметки HTML/ASS и петель, с нормальными
//! пробелами и кавычками и с произношением глоссария. Теги экспрессии, которые читает выбранный движок, доходят до
//! него (Higgs `<|emotion:anger|>`, Drama 3 `[laughs]` и `<whisper>…</whisper>`, Supertonic `<laugh>`); теги других
//! движков и ударение «+» для движка, который его не читает, снимаются и называются в segment.extra.tts_ignored.
//! Отображаемый tgt_text не меняется. Фраза, в которой после чистки не осталось слов, не синтезируется (тишина),
//! причина — в segment.extra.tts_skip.

use std::sync::LazyLock;

use dub_core::glossary::{apply_pronunciation, GlossaryEntry};
use dub_core::Project;
use regex::Regex;
use serde_json::Value;

/// Почему фраза не озвучивается: только звуки/музыка в разметке или ни одного слова.
pub const SKIP_SOUND_ONLY: &str = "sound_only";
pub const SKIP_NO_WORDS: &str = "no_words";

#[derive(Debug, PartialEq)]
pub struct TtsText {
    pub text: String,
    pub skip: Option<&'static str>,
    /// Разметка, которую движок не читает и которая снята: теги других движков, «+».
    pub ignored: Vec<String>,
}

/// Движок озвучки проекта: от него зависит, какая разметка экспрессии доходит до синтеза.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Higgs,
    Drama,
    Supertonic,
    Qwen,
    Audio8,
    Cloud,
}

impl Engine {
    pub fn load(models_root: &std::path::Path) -> Self {
        if crate::models::cloud_tts_on(models_root) {
            return Engine::Cloud;
        }
        let sel = crate::models::load_selection(models_root);
        match crate::models::resolve_tts(models_root, &sel).1.as_str() {
            crate::models::DRAMA_TTS => Engine::Drama,
            crate::models::SUPERTONIC_TTS => Engine::Supertonic,
            crate::models::AUDIO8_TTS => Engine::Audio8,
            q if crate::models::qwen_quant(q).is_some() => Engine::Qwen,
            _ => Engine::Higgs,
        }
    }

    /// Ударение «+» перед гласной: Supertonic и Qwen3-TTS переводят его в знак ударения при синтезе.
    fn reads_stress(self) -> bool {
        matches!(self, Engine::Supertonic | Engine::Qwen)
    }
}

/// Правки текста для синтеза, которые включает пользователь.
#[derive(Debug, Clone, Copy)]
pub struct Speak {
    /// Слова капсом — обычными буквами (модель читает капс по буквам), кроме аббревиатур.
    pub caps: bool,
    /// Служебные знаки, оставшиеся после разметки, — убрать.
    pub symbols: bool,
    pub engine: Engine,
}

impl Speak {
    pub fn load(models_root: &std::path::Path) -> Self {
        let sel = crate::models::load_selection(models_root);
        Speak {
            caps: crate::checks::on(&sel, "tts_caps"),
            symbols: crate::checks::on(&sel, "tts_symbols"),
            engine: Engine::load(models_root),
        }
    }
}

static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?[A-Za-z][^<>]{0,200}>").expect("html tag"));
static ASS_OVERRIDE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\\[^{}]*\}").expect("ass override"));
static ASS_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[Nn]").expect("ass break"));
static SQUARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\[\]]{0,80}\]|【[^【】]{0,80}】").expect("square"));
static ROUND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(([^()]{1,60})\)|（([^（）]{1,60})）").expect("round"));
static NOTES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[♪♫♬♩][^♪♫♬♩]*[♪♫♬♩]|[♪♫♬♩]").expect("notes"));
static ASTERISK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*([^*\n]{1,40})\*").expect("asterisk"));
static SPEAKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*(?:([\p{Lu}][\p{Lu}\d .'#-]{1,30}):\s+|[-–—]\s+)").expect("speaker label"));
static STRAY_MARKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[_|~^\\{}<>#*]+").expect("stray marks"));
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{L}+").expect("word"));
static HIGGS_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<\|[^<>\n]{0,48}\|>").expect("higgs tag"));
static HIGGS_PARTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<\|(\w+):(\w+)\|>$").expect("higgs parts"));
static ANGLE_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?\s*([a-z][a-z -]{0,30}?)\s*>").expect("angle tag"));
static DRAMA_POINT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([a-z][a-z '-]{0,38})\]").expect("drama point"));
static STRESS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\+([АаЕеЁёИиОоУуЫыЭэЮюЯяAaEeIiOoUuYy])").expect("stress"));
static KEPT: LazyLock<Regex> = LazyLock::new(|| Regex::new("\u{E000}(\\d+)\u{E001}").expect("kept tag"));

/// Теги экспрессии Higgs: категория -> значения (белый список HiggsAudio-Studio `director_worker.py`, как в Speech
/// Studio).
pub const HIGGS_TAGS: [(&str, &[&str]); 4] = [
    ("emotion", &["affection", "amusement", "anger", "arousal", "awe", "bitterness", "confusion", "contemplation", "contentment", "determination", "disgust", "elation", "enthusiasm", "fear", "helplessness", "longing", "pride", "relief", "sadness", "shame", "surprise"]),
    ("prosody", &["speed_very_slow", "speed_slow", "speed_fast", "speed_very_fast", "pitch_low", "pitch_high", "expressive_high", "expressive_low", "pause", "long_pause"]),
    ("style", &["singing", "shouting", "whispering"]),
    ("sfx", &["cough", "laughter", "crying", "screaming", "burping", "humming", "sigh", "sniff", "sneeze"]),
];

/// Палитра Drama 3 — примеры приложения C техотчёта (Table 10): события по месту и подача отрезка.
pub const DRAMA_EVENTS: &[&str] = &["laughs", "sighs", "gasps", "clears throat", "hesitates", "pause"];
pub const DRAMA_SCOPES: &[&str] = &["whisper", "slow", "emphasis"];

/// Теги Supertonic 3 по карточке `Supertone/supertonic-3`: по месту.
pub const SUPERTONIC_TAGS: &[&str] = &["laugh", "breath", "sigh"];

/// Разметка экспрессии: что читает `engine`, прячется за метку и вернётся после чистки; разметка других движков и
/// непрочитанное «+» снимаются в `ignored`. Теги звуков субтитров (`[music]`) и HTML субтитров здесь не трогаются.
fn engine_markup(text: &str, engine: Engine, kept: &mut Vec<String>, ignored: &mut Vec<String>) -> String {
    let keep = |kept: &mut Vec<String>, tag: &str| {
        kept.push(tag.to_string());
        format!("\u{E000}{}\u{E001}", kept.len() - 1)
    };
    let t = HIGGS_TAG.replace_all(text, |c: &regex::Captures| {
        let known = HIGGS_PARTS
            .captures(&c[0])
            .is_some_and(|h| HIGGS_TAGS.iter().any(|(cat, values)| *cat == &h[1] && values.contains(&&h[2])));
        if engine == Engine::Higgs && known {
            keep(kept, &c[0])
        } else {
            ignored.push(c[0].to_string());
            String::new()
        }
    });
    let t = ANGLE_TAG.replace_all(&t, |c: &regex::Captures| {
        let name = c[1].trim();
        let closing = c[0].starts_with("</");
        let ours = match engine {
            Engine::Drama => DRAMA_SCOPES.contains(&name),
            Engine::Supertonic => !closing && SUPERTONIC_TAGS.contains(&name),
            _ => false,
        };
        if ours {
            keep(kept, &c[0])
        } else if DRAMA_SCOPES.contains(&name) || SUPERTONIC_TAGS.contains(&name) {
            ignored.push(c[0].to_string());
            String::new()
        } else {
            c[0].to_string()
        }
    });
    let t = if engine == Engine::Drama {
        DRAMA_POINT.replace_all(&t, |c: &regex::Captures| {
            if DRAMA_EVENTS.contains(&c[1].trim()) { keep(kept, &c[0]) } else { c[0].to_string() }
        })
    } else {
        t
    };
    if engine.reads_stress() {
        return t.into_owned();
    }
    let mut stress = false;
    let t = STRESS.replace_all(&t, |c: &regex::Captures| {
        stress = true;
        c[1].to_string()
    });
    if stress {
        ignored.push("+".to_string());
    }
    t.into_owned()
}

const VOWELS: &str = "aeiouyаеёиоуыэюяіїє";

fn is_caps(w: &str) -> bool {
    w.chars().count() >= 2 && w.chars().all(|c| !c.is_lowercase()) && w.chars().any(char::is_uppercase)
}

/// Слова капсом обычными буквами. Строка капсом целиком (крик или субтитры капсом) — каждое слово, кроме
/// коротких без гласных (ФБР, МВД); в обычной строке — слова от 5 букв, короче — аббревиатуры (США, НАТО).
fn ease_caps(t: &str) -> String {
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    let upper = t.chars().filter(|c| c.is_uppercase()).count();
    let shouting = WORD.find_iter(t).filter(|m| is_caps(m.as_str())).count() >= 2 && upper * 5 >= letters * 4;
    WORD.replace_all(t, |c: &regex::Captures| {
        let w = &c[0];
        let n = w.chars().count();
        let abbreviation = if shouting {
            n <= 3 && !w.chars().flat_map(char::to_lowercase).any(|ch| VOWELS.contains(ch))
        } else {
            n < 5
        };
        if is_caps(w) && !abbreviation { w.to_lowercase() } else { w.to_string() }
    })
    .into_owned()
}

/// Основы слов (от 4 букв), которыми субтитры помечают неречевые звуки: английский и языки интерфейса.
const SOUND_STEMS: &[&str] = &[
    "laugh", "chuckl", "giggl", "sigh", "applau", "music", "singing", "cries", "crying", "sobs", "sobbing", "scream",
    "cough", "gasp", "groan", "whisper", "inaudib", "sniff", "grunt", "moan", "yawn", "shout", "clap", "cheer",
    "noise", "silence", "static", "beep", "ringing", "knock", "footstep", "explosion", "gunshot", "thunder",
    "bark", "growl", "sneez", "humming", "whistl", "panting", "breath", "screech", "crash",
    "смех", "смеёт", "смеет", "хохот", "вздох", "вздых", "музык", "аплодис", "плач", "крик", "кричит", "кашл",
    "шёпот", "шепот", "шепч", "стон", "тишин", "звон", "стук", "выстрел", "взрыв", "гром", "дыхан", "всхлип",
    "risa", "suspir", "música", "aplaus", "llant", "grita", "grito", "susurr",
    "rire", "rires", "soupir", "musique", "applaudi", "pleur", "touss", "chuchot",
    "riso", "risad", "choro", "tosse", "sussurr",
];

/// Короткие слова-звуки: только целым словом.
const SOUND_WORDS: &[&str] = &["ríe", "ríen", "rit", "rient", "cri", "cris", "tos", "sob", "hum", "hums"];

/// Звуки в письме без пробелов: ищутся подстрокой.
const SOUND_CJK: &[&str] = &["笑", "音乐", "叹", "掌声", "哭", "咳", "尖叫", "低语", "音楽", "拍手", "泣"];

fn is_sound_tag(inner: &str) -> bool {
    let words: Vec<&str> = inner.split_whitespace().collect();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    let letters: Vec<char> = inner.chars().filter(|c| c.is_alphabetic()).collect();
    let shouted = words.len() > 1 && letters.len() > 1 && letters.iter().all(|c| c.is_uppercase());
    let lower = inner.to_lowercase();
    let word_hit = lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| SOUND_STEMS.iter().any(|s| w.starts_with(s)) || SOUND_WORDS.contains(&w));
    shouted || word_hit || SOUND_CJK.iter().any(|s| lower.contains(s))
}

fn has_words(s: &str) -> bool {
    s.chars().any(char::is_alphanumeric)
}

/// Слова текста с их хвостом-разделителем: ("нет", ", ").
fn units(s: &str) -> (String, Vec<(String, String)>) {
    let mut head = String::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            match out.last_mut() {
                Some((w, sep)) if sep.is_empty() => w.push(c),
                _ => out.push((c.to_string(), String::new())),
            }
        } else {
            match out.last_mut() {
                Some((_, sep)) => sep.push(c),
                None => head.push(c),
            }
        }
    }
    (head, out)
}

/// Петля: n-грамма слов три и более раз подряд -> один раз (с пунктуацией после последнего повтора).
fn collapse_loops(s: &str) -> String {
    let (head, u) = units(s);
    let key: Vec<String> = u.iter().map(|(w, _)| w.to_lowercase()).collect();
    let mut out = head;
    let mut i = 0;
    'next: while i < u.len() {
        for n in 1..=(u.len() - i) / 3 {
            let mut reps = 1;
            while i + (reps + 1) * n <= u.len() && key[i..i + n] == key[i + reps * n..i + (reps + 1) * n] {
                reps += 1;
            }
            if reps >= 3 {
                for (k, (w, sep)) in u[i..i + n].iter().enumerate() {
                    out.push_str(w);
                    out.push_str(if k + 1 == n { &u[i + reps * n - 1].1 } else { sep });
                }
                i += reps * n;
                continue 'next;
            }
        }
        out.push_str(&u[i].0);
        out.push_str(&u[i].1);
        i += 1;
    }
    out
}

fn normalize_quotes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '“' | '”' | '„' | '‟' | '«' | '»' | '″' | '＂' => '"',
            '‘' | '’' | '‚' | '‛' | '‹' | '›' => '\'',
            other => other,
        })
        .collect()
}

/// Текст для синтеза. `src` — исходная реплика: петля, которую сказал сам говорящий («no, no, no»),
/// озвучивается как сказана; схлопывается только петля перевода.
pub fn tts_text(tgt: &str, src: &str, lang: &str, glossary: &[GlossaryEntry], speak: Speak) -> TtsText {
    let original_has_words = has_words(tgt);
    let mut kept: Vec<String> = Vec::new();
    let mut ignored: Vec<String> = Vec::new();
    let t = engine_markup(tgt, speak.engine, &mut kept, &mut ignored);
    let t = ASS_OVERRIDE.replace_all(&t, "");
    let t = ASS_BREAK.replace_all(&t, "\n").replace("\\h", " ");
    let t = HTML_TAG.replace_all(&t, "");
    let t = SQUARE.replace_all(&t, " ");
    let t = ROUND.replace_all(&t, |c: &regex::Captures| {
        let inner = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()).unwrap_or("");
        if is_sound_tag(inner) { " ".to_string() } else { c[0].to_string() }
    });
    let t = NOTES.replace_all(&t, " ");
    let t = ASTERISK.replace_all(&t, |c: &regex::Captures| {
        if is_sound_tag(&c[1]) { " ".to_string() } else { c[1].to_string() }
    });
    let t = SPEAKER.replace_all(&t, |c: &regex::Captures| match c.get(1) {
        Some(label) if label.as_str().chars().filter(|ch| ch.is_alphabetic()).count() < 3 => c[0].to_string(),
        _ => String::new(),
    });
    let mut t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if loops_in(&t) && !loops_in(src) {
        t = collapse_loops(&t);
    }
    let mut t = apply_pronunciation(&t, glossary, lang);
    if crate::stress::is_russian(lang) {
        t = dub_core::normalize_ru::normalize(&t).text;
    }
    if speak.caps {
        t = ease_caps(&t);
    }
    if speak.symbols {
        t = STRAY_MARKS.replace_all(&t, " ").split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let t = normalize_quotes(&t);
    let t = t.trim().trim_start_matches([',', ';', ':']).trim().to_string();
    if has_words(&KEPT.replace_all(&t, "")) {
        let text = KEPT.replace_all(&t, |c: &regex::Captures| c[1].parse::<usize>().ok().and_then(|i| kept.get(i)).cloned().unwrap_or_default());
        return TtsText { text: text.into_owned(), skip: None, ignored };
    }
    let skip = if original_has_words { SKIP_SOUND_ONLY } else { SKIP_NO_WORDS };
    TtsText { text: String::new(), skip: Some(skip), ignored }
}

fn loops_in(s: &str) -> bool {
    collapse_loops(s) != s
}

/// Показ в окне и в project_get: `tts_skip` у фраз, которые не озвучиваются, `tts_text` у фраз, чей текст
/// для синтеза отличается от показанного, `tts_ignored` — разметка, которую движок не прочтёт. Не хранится —
/// считается при чтении.
pub fn annotate(proj: &mut Project, speak: Speak) {
    let glossary = dub_core::glossary::for_target(&proj.glossary, &proj.tgt_lang);
    let lang = proj.tgt_lang.clone();
    for s in &mut proj.segments {
        s.extra.remove("tts_skip");
        s.extra.remove("tts_text");
        s.extra.remove("tts_ignored");
        if s.tgt_text.trim().is_empty() {
            continue;
        }
        let t = tts_text(&s.tgt_text, &s.src_text, &lang, &glossary, speak);
        if !t.ignored.is_empty() {
            s.extra.insert("tts_ignored".into(), Value::from(t.ignored.clone()));
        }
        match t.skip {
            Some(reason) => {
                s.extra.insert("tts_skip".into(), Value::String(reason.into()));
            }
            None if t.text != s.tgt_text.split_whitespace().collect::<Vec<_>>().join(" ") => {
                s.extra.insert("tts_text".into(), Value::String(t.text));
            }
            None => {}
        }
    }
}

/// Проект для синтеза: tgt_text каждой фразы — текст для синтеза; фразы без слов после чистки — с пустым
/// текстом (синтез их пропускает) и extra.tts_skip. Отсчёт — строкой в журнал.
pub fn synthesis_view(proj: &Project, speak: Speak, progress: &crate::analyze::Progress) -> Project {
    let mut view = proj.clone();
    let glossary = dub_core::glossary::for_target(&proj.glossary, &proj.tgt_lang);
    let mut skipped: Vec<String> = Vec::new();
    let mut ignored: Vec<String> = Vec::new();
    for s in &mut view.segments {
        let keep_original = s.extra.get("keep_original").and_then(Value::as_bool).unwrap_or(false);
        if keep_original || s.tgt_text.trim().is_empty() {
            continue;
        }
        let t = tts_text(&s.tgt_text, &s.src_text, &proj.tgt_lang, &glossary, speak);
        if !t.ignored.is_empty() {
            ignored.push(format!("{} {}", s.id, t.ignored.join(" ")));
        }
        if let Some(reason) = t.skip {
            skipped.push(format!("{} «{}»", s.id, s.tgt_text.trim().chars().take(40).collect::<String>()));
            s.extra.insert("tts_skip".into(), Value::String(reason.into()));
        }
        s.tgt_text = t.text;
    }
    if !skipped.is_empty() {
        progress(serde_json::json!({
            "stage": "tts",
            "msg": t!("tts-silent-after-cleanup", count = skipped.len(), lines = skipped.join("; ")),
        }));
    }
    if !ignored.is_empty() {
        progress(serde_json::json!({
            "stage": "tts",
            "msg": t!("tts-markup-ignored", count = ignored.len(), lines = ignored.join("; ")),
        }));
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Speak = Speak { caps: true, symbols: true, engine: Engine::Cloud };
    const NONE: Speak = Speak { caps: false, symbols: false, engine: Engine::Cloud };

    fn ru(t: &str, speak: Speak) -> String {
        tts_text(t, "", "ru", &[], speak).text
    }

    #[test]
    fn shouted_words_are_said_as_words_and_abbreviations_stay() {
        assert_eq!(ru("Это было ПОТРЯСАЮЩЕ", ALL), "Это было потрясающе");
        assert_eq!(ru("Агенты ФБР и НАТО уже здесь", ALL), "Агенты ФБР и НАТО уже здесь");
        assert_eq!(ru("Я СКАЗАЛ НЕТ!", ALL), "Я сказал нет!");
        assert_eq!(ru("ФБР, ОТКРЫВАЙТЕ!", ALL), "ФБР, открывайте!");
        assert_eq!(ru("Это было ПОТРЯСАЮЩЕ", NONE), "Это было ПОТРЯСАЮЩЕ");
    }

    #[test]
    fn stray_marks_are_not_read() {
        assert_eq!(ru("Привет_мир | ну ~ да ^ #1", ALL), "Привет мир ну да один");
        assert_eq!(ru("Привет_мир", NONE), "Привет_мир");
    }

    #[test]
    fn russian_numbers_dates_and_money_are_said_in_words() {
        assert_eq!(ru("Это было в 1945 году", ALL), "Это было в тысяча девятьсот сорок пятом году");
        assert_eq!(ru("Встреча 12 марта в 18:30.", ALL), "Встреча двенадцатого марта в восемнадцать тридцать.");
        assert_eq!(ru("Цена 150 руб. за штуку", ALL), "Цена сто пятьдесят рублей за штуку");
        assert_eq!(tts_text("It was 1945", "", "en", &[], ALL).text, "It was 1945");
        let higgs = tts_text("<|emotion:awe|> Через 5 минут", "", "ru", &[], Speak { engine: Engine::Higgs, ..ALL }).text;
        assert!(higgs.ends_with("Через пять минут"), "{higgs}");
    }

    fn clean(t: &str) -> TtsText {
        tts_text(t, "", "en", &[], ALL)
    }

    #[test]
    fn sound_tags_go_away_and_speech_stays() {
        assert_eq!(clean("[music] Hello there").text, "Hello there");
        assert_eq!(clean("(laughs) You got me!").text, "You got me!");
        assert_eq!(clean("(LAUGHING) Fine.").text, "Fine.");
        assert_eq!(clean("I was (quite honestly) tired").text, "I was (quite honestly) tired", "a remark in brackets is speech");
        assert_eq!(clean("*sigh* Again?").text, "Again?");
        assert_eq!(clean("♪ la la la ♪ Next").text, "Next");
        assert_eq!(tts_text("[МУЗЫКА] Привет", "", "ru", &[], ALL).text, "Привет");
        assert_eq!(tts_text("(смеётся) Ну да", "", "ru", &[], ALL).text, "Ну да");
        assert_eq!(tts_text("(risas) Vale", "", "es", &[], ALL).text, "Vale");
        assert_eq!(tts_text("(soupire) Bon", "", "fr", &[], ALL).text, "Bon");
        assert_eq!(tts_text("(risos) Tá", "", "pt", &[], ALL).text, "Tá");
        assert_eq!(tts_text("（笑）你好", "", "zh", &[], ALL).text, "你好");
        assert_eq!(tts_text("【音乐】你好", "", "zh", &[], ALL).text, "你好");
    }

    #[test]
    fn speaker_labels_and_markup_are_not_read_aloud() {
        assert_eq!(clean("JOHN: Get down!").text, "Get down!");
        assert_eq!(clean("- Where? - Here.").text, "Where? - Here.");
        assert_eq!(tts_text("— Куда?\\N— Сюда.", "", "ru", &[], ALL).text, "Куда? Сюда.");
        assert_eq!(clean("<i>Hello</i> {\\an8}world").text, "Hello world");
        assert_eq!(clean("Look: here").text, "Look: here", "a colon in speech stays");
        assert_eq!(clean("OK: let's go").text, "OK: let's go", "two capitals are a word, not a name");
        assert_eq!(clean("MAN #2: Run!").text, "Run!");
    }

    #[test]
    fn emphasis_and_acronyms_are_said() {
        assert_eq!(clean("I *really* mean it").text, "I really mean it");
        assert_eq!(clean("*laughs* Fine").text, "Fine");
        assert_eq!(clean("They work at (NASA) now").text, "They work at (NASA) now");
        assert_eq!(clean("Call the (FBI)!").text, "Call the (FBI)!");
        assert_eq!(clean("(DOOR SLAMS) Who's there?").text, "Who's there?");
    }

    #[test]
    fn a_loop_of_the_translation_is_said_once_but_the_speakers_own_is_kept() {
        assert_eq!(clean("Thanks for watching! Thanks for watching! Thanks for watching!").text, "Thanks for watching!");
        assert_eq!(tts_text("нет, нет, нет, нет!", "no", "ru", &[], ALL).text, "нет!");
        assert_eq!(tts_text("нет, нет, нет!", "no, no, no!", "ru", &[], ALL).text, "нет, нет, нет!");
        assert_eq!(tts_text("谢谢谢谢谢谢", "", "zh", &[], ALL).text, "谢谢谢谢谢谢", "no word breaks: left as is");
    }

    #[test]
    fn spaces_quotes_and_pronunciation_are_normalized() {
        assert_eq!(clean("  «Hi»,   “you”  ").text, "\"Hi\", \"you\"");
        let e = GlossaryEntry { term: "Nvidia".into(), keep: true, pronunciation: "Энвидиа".into(), lang: "ru".into(), ..GlossaryEntry::default() };
        assert_eq!(tts_text("Карта Nvidia", "", "ru", std::slice::from_ref(&e), ALL).text, "Карта Энвидиа");
        assert_eq!(tts_text("Tarjeta Nvidia", "", "es", &[e], ALL).text, "Tarjeta Nvidia");
    }

    #[test]
    fn nothing_left_to_say_is_silence_with_a_reason() {
        assert_eq!(clean("[music]"), TtsText { text: String::new(), skip: Some(SKIP_SOUND_ONLY), ignored: vec![] });
        assert_eq!(clean("♪ ♪").skip, Some(SKIP_NO_WORDS));
        assert_eq!(clean("..."), TtsText { text: String::new(), skip: Some(SKIP_NO_WORDS), ignored: vec![] });
        assert_eq!(tts_text("(вздыхает)", "", "ru", &[], ALL).skip, Some(SKIP_SOUND_ONLY));
    }

    fn on(engine: Engine, t: &str) -> TtsText {
        tts_text(t, "", "ru", &[], Speak { engine, ..ALL })
    }

    #[test]
    fn each_engine_gets_its_own_tags_and_the_rest_are_named() {
        let line = "<|emotion:anger|> Хватит! [laughs] <whisper>тише</whisper> <laugh> з+амок <|emotion:rage|>";
        let higgs = on(Engine::Higgs, line);
        assert_eq!(higgs.text, "<|emotion:anger|> Хватит! тише замок");
        assert_eq!(higgs.ignored, vec!["<|emotion:rage|>", "<whisper>", "</whisper>", "<laugh>", "+"]);
        let drama = on(Engine::Drama, line);
        assert_eq!(drama.text, "Хватит! [laughs] <whisper>тише</whisper> замок");
        assert_eq!(drama.ignored, vec!["<|emotion:anger|>", "<|emotion:rage|>", "<laugh>", "+"]);
        let supertonic = on(Engine::Supertonic, line);
        assert_eq!(supertonic.text, "Хватит! тише <laugh> з+амок");
        assert_eq!(supertonic.ignored, vec!["<|emotion:anger|>", "<|emotion:rage|>", "<whisper>", "</whisper>"]);
        assert_eq!(on(Engine::Qwen, line).text, "Хватит! тише з+амок");
        assert_eq!(on(Engine::Cloud, line).text, "Хватит! тише замок");
        assert_eq!(on(Engine::Audio8, line).text, "Хватит! тише замок", "Audio8 reads no tags and places stress itself");
    }

    #[test]
    fn subtitle_sounds_and_markup_are_still_cleaned_for_an_engine_with_tags() {
        assert_eq!(on(Engine::Drama, "[music] <i>Привет</i> [door slams]").text, "Привет");
        assert!(on(Engine::Drama, "<i>Привет</i>").ignored.is_empty());
        assert_eq!(on(Engine::Drama, "[laughs]").skip, Some(SKIP_SOUND_ONLY), "a tag alone is not a line");
        assert_eq!(on(Engine::Higgs, "<|sfx:laughter|>").skip, Some(SKIP_SOUND_ONLY));
        assert_eq!(on(Engine::Higgs, "<|emotion:elation|><|sfx:laughter|>Ха-ха!").text, "<|emotion:elation|><|sfx:laughter|>Ха-ха!");
    }

    #[test]
    fn the_view_silences_empty_lines_and_the_project_shows_why() {
        let mut p = Project { tgt_lang: "ru".into(), ..Project::default() };
        let seg = |id: &str, tgt: &str| dub_core::Segment { id: id.into(), tgt_text: tgt.into(), ..Default::default() };
        p.segments = vec![seg("s0", "[музыка]"), seg("s1", "(смеётся) Привет"), seg("s2", "Привет")];
        let seen = std::sync::Mutex::new(Vec::new());
        let progress = |v: Value| seen.lock().unwrap().push(v["msg"].as_str().unwrap_or_default().to_string());
        let view = synthesis_view(&p, ALL, &progress);
        assert_eq!(view.segments[0].tgt_text, "");
        assert_eq!(view.segments[1].tgt_text, "Привет");
        assert_eq!(p.segments[1].tgt_text, "(смеётся) Привет", "the shown text is untouched");
        assert!(seen.lock().unwrap()[0].contains("s0"));
        annotate(&mut p, ALL);
        assert_eq!(p.segments[0].extra["tts_skip"], SKIP_SOUND_ONLY);
        assert_eq!(p.segments[1].extra["tts_text"], "Привет");
        assert!(!p.segments[2].extra.contains_key("tts_text"));
        p.segments[2].tgt_text = "<|emotion:anger|> Привет".into();
        annotate(&mut p, ALL);
        assert_eq!(p.segments[2].extra["tts_ignored"], serde_json::json!(["<|emotion:anger|>"]));
        assert_eq!(p.segments[2].extra["tts_text"], "Привет");
    }
}
