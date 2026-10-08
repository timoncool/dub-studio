//! Текст, уходящий в синтез (Higgs и облачный TTS): без тегов неречевых звуков, меток спикеров, разметки
//! HTML/ASS и петель, с нормальными пробелами и кавычками и с произношением глоссария. Отображаемый tgt_text
//! не меняется. Фраза, в которой после чистки не осталось слов, не синтезируется (тишина), причина — в
//! segment.extra.tts_skip.

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
pub fn tts_text(tgt: &str, src: &str, lang: &str, glossary: &[GlossaryEntry]) -> TtsText {
    let original_has_words = has_words(tgt);
    let t = ASS_OVERRIDE.replace_all(tgt, "");
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
    let t = normalize_quotes(&apply_pronunciation(&t, glossary, lang));
    let t = t.trim().trim_start_matches([',', ';', ':']).trim().to_string();
    if has_words(&t) {
        return TtsText { text: t, skip: None };
    }
    let skip = if original_has_words { SKIP_SOUND_ONLY } else { SKIP_NO_WORDS };
    TtsText { text: String::new(), skip: Some(skip) }
}

fn loops_in(s: &str) -> bool {
    collapse_loops(s) != s
}

/// Показ в окне и в project_get: `tts_skip` у фраз, которые не озвучиваются, и `tts_text` у фраз, чей текст
/// для синтеза отличается от показанного. Не хранится — считается при чтении.
pub fn annotate(proj: &mut Project) {
    let glossary = dub_core::glossary::for_target(&proj.glossary, &proj.tgt_lang);
    let lang = proj.tgt_lang.clone();
    for s in &mut proj.segments {
        s.extra.remove("tts_skip");
        s.extra.remove("tts_text");
        if s.tgt_text.trim().is_empty() {
            continue;
        }
        let t = tts_text(&s.tgt_text, &s.src_text, &lang, &glossary);
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
pub fn synthesis_view(proj: &Project, progress: &crate::analyze::Progress) -> Project {
    let mut view = proj.clone();
    let glossary = dub_core::glossary::for_target(&proj.glossary, &proj.tgt_lang);
    let mut skipped: Vec<String> = Vec::new();
    for s in &mut view.segments {
        let keep_original = s.extra.get("keep_original").and_then(Value::as_bool).unwrap_or(false);
        if keep_original || s.tgt_text.trim().is_empty() {
            continue;
        }
        let t = tts_text(&s.tgt_text, &s.src_text, &proj.tgt_lang, &glossary);
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
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(t: &str) -> TtsText {
        tts_text(t, "", "en", &[])
    }

    #[test]
    fn sound_tags_go_away_and_speech_stays() {
        assert_eq!(clean("[music] Hello there").text, "Hello there");
        assert_eq!(clean("(laughs) You got me!").text, "You got me!");
        assert_eq!(clean("(LAUGHING) Fine.").text, "Fine.");
        assert_eq!(clean("I was (quite honestly) tired").text, "I was (quite honestly) tired", "a remark in brackets is speech");
        assert_eq!(clean("*sigh* Again?").text, "Again?");
        assert_eq!(clean("♪ la la la ♪ Next").text, "Next");
        assert_eq!(tts_text("[МУЗЫКА] Привет", "", "ru", &[]).text, "Привет");
        assert_eq!(tts_text("(смеётся) Ну да", "", "ru", &[]).text, "Ну да");
        assert_eq!(tts_text("(risas) Vale", "", "es", &[]).text, "Vale");
        assert_eq!(tts_text("(soupire) Bon", "", "fr", &[]).text, "Bon");
        assert_eq!(tts_text("(risos) Tá", "", "pt", &[]).text, "Tá");
        assert_eq!(tts_text("（笑）你好", "", "zh", &[]).text, "你好");
        assert_eq!(tts_text("【音乐】你好", "", "zh", &[]).text, "你好");
    }

    #[test]
    fn speaker_labels_and_markup_are_not_read_aloud() {
        assert_eq!(clean("JOHN: Get down!").text, "Get down!");
        assert_eq!(clean("- Where? - Here.").text, "Where? - Here.");
        assert_eq!(tts_text("— Куда?\\N— Сюда.", "", "ru", &[]).text, "Куда? Сюда.");
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
        assert_eq!(tts_text("нет, нет, нет, нет!", "no", "ru", &[]).text, "нет!");
        assert_eq!(tts_text("нет, нет, нет!", "no, no, no!", "ru", &[]).text, "нет, нет, нет!");
        assert_eq!(tts_text("谢谢谢谢谢谢", "", "zh", &[]).text, "谢谢谢谢谢谢", "no word breaks: left as is");
    }

    #[test]
    fn spaces_quotes_and_pronunciation_are_normalized() {
        assert_eq!(clean("  «Hi»,   “you”  ").text, "\"Hi\", \"you\"");
        let e = GlossaryEntry { term: "Nvidia".into(), keep: true, pronunciation: "Энвидиа".into(), lang: "ru".into(), ..GlossaryEntry::default() };
        assert_eq!(tts_text("Карта Nvidia", "", "ru", std::slice::from_ref(&e)).text, "Карта Энвидиа");
        assert_eq!(tts_text("Tarjeta Nvidia", "", "es", &[e]).text, "Tarjeta Nvidia");
    }

    #[test]
    fn nothing_left_to_say_is_silence_with_a_reason() {
        assert_eq!(clean("[music]"), TtsText { text: String::new(), skip: Some(SKIP_SOUND_ONLY) });
        assert_eq!(clean("♪ ♪").skip, Some(SKIP_NO_WORDS));
        assert_eq!(clean("..."), TtsText { text: String::new(), skip: Some(SKIP_NO_WORDS) });
        assert_eq!(tts_text("(вздыхает)", "", "ru", &[]).skip, Some(SKIP_SOUND_ONLY));
    }

    #[test]
    fn the_view_silences_empty_lines_and_the_project_shows_why() {
        let mut p = Project { tgt_lang: "ru".into(), ..Project::default() };
        let seg = |id: &str, tgt: &str| dub_core::Segment { id: id.into(), tgt_text: tgt.into(), ..Default::default() };
        p.segments = vec![seg("s0", "[музыка]"), seg("s1", "(смеётся) Привет"), seg("s2", "Привет")];
        let seen = std::sync::Mutex::new(Vec::new());
        let progress = |v: Value| seen.lock().unwrap().push(v["msg"].as_str().unwrap_or_default().to_string());
        let view = synthesis_view(&p, &progress);
        assert_eq!(view.segments[0].tgt_text, "");
        assert_eq!(view.segments[1].tgt_text, "Привет");
        assert_eq!(p.segments[1].tgt_text, "(смеётся) Привет", "the shown text is untouched");
        assert!(seen.lock().unwrap()[0].contains("s0"));
        annotate(&mut p);
        assert_eq!(p.segments[0].extra["tts_skip"], SKIP_SOUND_ONLY);
        assert_eq!(p.segments[1].extra["tts_text"], "Привет");
        assert!(!p.segments[2].extra.contains_key("tts_text"));
    }
}
