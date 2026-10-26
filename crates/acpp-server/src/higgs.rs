//! Higgs Audio v3 of audiocpp_server against a runaway take: a phrase that does not find its end and speaks on. The
//! token cap of a phrase follows its length and script, a take longer than its phrase allows is a runaway, and each
//! attempt of a phrase has its own seed. One set of rules for every studio that speaks through the server.

use crate::AcppError;
use serde_json::{json, Map, Value};

/// Higgs v3 in audio.cpp counts `max_tokens` in codec frames of 40 ms (model_specs/higgs_audio_tts.json).
pub const FRAMES_PER_SEC: f64 = 25.0;
/// Frames the delay of the codebooks adds after the last sound.
pub const CODEBOOK_DELAY: u32 = 8;
/// The server takes a seed as a small unsigned integer.
pub const SEED_RANGE: u64 = 1 << 31;
/// Seconds of sound a letter past which a phrase is a runaway, and the seconds any phrase has on top (HiggsAudio-Studio).
const RUNAWAY_PER_CHAR: f64 = 0.13;
const RUNAWAY_SLACK: f64 = 3.0;
/// Letters a second of the language the runaway rule is set on (Russian): where a letter sounds longer (an ideograph
/// is a syllable), the rule allows a letter as many times more.
const RUNAWAY_CPS: f64 = 13.0;

/// Letters and digits of a phrase: what its length is counted in.
pub fn spoken_chars(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphanumeric()).count()
}

/// The language a phrase is paced by: the one it is set to, else what its letters say — ideographs Chinese, kana
/// Japanese, Hangul Korean (a sign of theirs is a syllable, two to three times a Russian letter), any other Russian.
pub fn pace_lang<'a>(text: &str, lang: &'a str) -> &'a str {
    if !lang.is_empty() && lang != "auto" {
        return lang;
    }
    let (mut han, mut kana, mut hangul, mut other) = (0usize, 0usize, 0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphanumeric()) {
        match c as u32 {
            0x3040..=0x30ff => kana += 1,
            0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff => han += 1,
            0xac00..=0xd7af => hangul += 1,
            _ => other += 1,
        }
    }
    if kana > 0 && kana + han > other {
        "ja"
    } else if han > other {
        "zh"
    } else if hangul > other {
        "ko"
    } else {
        "ru"
    }
}

/// Seconds of sound past which a phrase of `chars` letters and digits in `lang` is a runaway.
pub fn runaway_secs(chars: usize, lang: &str) -> f64 {
    let lang = if lang.is_empty() || lang == "auto" { "ru" } else { lang };
    chars as f64 * RUNAWAY_PER_CHAR * (RUNAWAY_CPS / dub_core::fit::table_cps(lang)).max(1.0) + RUNAWAY_SLACK
}

/// The `max_tokens` of a phrase: Higgs stops a second past the runaway length, so what it says by then is judged a
/// runaway and voiced again, and a phrase read at any pace short of that is not cut.
pub fn token_cap(chars: usize, lang: &str) -> u32 {
    ((((runaway_secs(chars, lang) + 1.0) * FRAMES_PER_SEC).ceil() as u32) + CODEBOOK_DELAY).clamp(64, 2048)
}

/// Whether a take of `secs` seconds of a phrase of `chars` letters and digits in `lang` is a runaway.
pub fn is_runaway(secs: f64, chars: usize, lang: &str) -> bool {
    chars >= 1 && secs > runaway_secs(chars, lang)
}

/// The seed of attempt `attempt` of a phrase whose first attempt has `base`.
pub fn attempt_seed(base: u64, attempt: u32) -> u64 {
    base.wrapping_add(u64::from(attempt) * 7919) % SEED_RANGE
}

/// Puts the phrase's token cap into the request options of `text` paced as `lang` (unless they name one) and asks the
/// server for the sound it has at the cap rather than an error, so a runaway is judged by its length.
pub fn guard(options: &mut Map<String, Value>, text: &str, lang: &str) {
    if !options.contains_key("max_tokens") {
        options.insert("max_tokens".into(), json!(token_cap(spoken_chars(text), pace_lang(text, lang))));
    }
    options.insert("allow_incomplete_generation".into(), json!(true));
}

/// Whether the server ended a phrase because it reached its token cap without an end: a runaway take, not a failure
/// of the engine.
pub fn is_token_cap(error: &AcppError) -> bool {
    matches!(error, AcppError::Server { message, .. } if message.contains("max_tokens"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_cap_follows_the_phrase_and_its_script() {
        assert_eq!(token_cap(10, "ru"), ((10.0 * 0.13 + 3.0 + 1.0) * 25.0f64).ceil() as u32 + 8, "frames of 40 ms to a second past the runaway");
        assert_eq!(token_cap(2_000, "ru"), 2048);
        assert_eq!(token_cap(0, "ru"), 108);
        assert!(token_cap(40, "zh") > token_cap(40, "ru"), "an ideograph is a syllable");
        assert_eq!(token_cap(10, "auto"), token_cap(10, "ru"));
    }

    #[test]
    fn a_take_past_its_phrase_is_a_runaway() {
        assert!(!is_runaway(4.0, 10, "ru"), "0.13 s a letter + 3 s is still speech");
        assert!(is_runaway(4.5, 10, "ru"));
        assert!(!is_runaway(60.0, 0, "ru"), "a phrase without letters has no length to judge by");
    }

    /// A Chinese phrase of language «auto» is paced as Chinese: 40 ideographs read in 9 s were a «runaway» by the
    /// Russian rate on every attempt.
    #[test]
    fn a_phrase_of_auto_language_is_paced_by_its_script() {
        let zh = "今天晚上八点我们在老桥旁边见面然后一起去看电影吃饭聊天散步回家休息明天再见朋友们";
        assert_eq!(pace_lang(zh, "auto"), "zh");
        assert_eq!(pace_lang(zh, ""), "zh");
        assert_eq!(pace_lang("今日はいい天気ですね", "auto"), "ja");
        assert_eq!(pace_lang("오늘 날씨가 좋네요", "auto"), "ko");
        assert_eq!(pace_lang("Сегодня в восемь вечера", "auto"), "ru");
        assert_eq!(pace_lang(zh, "en"), "en", "a language set by hand stands");
        assert!(!is_runaway(9.0, spoken_chars(zh), pace_lang(zh, "auto")));
        assert!(is_runaway(9.0, spoken_chars(zh), "ru"));
    }

    #[test]
    fn each_attempt_has_its_own_seed_within_the_servers_range() {
        assert_eq!(attempt_seed(5, 0), 5);
        assert_eq!(attempt_seed(5, 2), 5 + 2 * 7919);
        assert!(attempt_seed(u64::MAX, 3) < SEED_RANGE);
        assert_ne!(attempt_seed(5, 1), attempt_seed(5, 2));
    }

    #[test]
    fn the_guard_caps_a_phrase_and_keeps_a_cap_it_is_given() {
        let mut options = Map::new();
        guard(&mut options, "Да.", "ru");
        assert_eq!(options["max_tokens"], json!(token_cap(2, "ru")));
        assert_eq!(options["allow_incomplete_generation"], json!(true));
        let mut given = Map::new();
        given.insert("max_tokens".into(), json!(300));
        guard(&mut given, "Да.", "ru");
        assert_eq!(given["max_tokens"], json!(300));
    }

    #[test]
    fn the_servers_token_cap_error_is_a_runaway() {
        let cap = AcppError::Server { status: 200, kind: "stream_error".into(), message: "max_tokens reached before EOC".into() };
        let other = AcppError::Server { status: 500, kind: "internal".into(), message: "CUDA error".into() };
        assert!(is_token_cap(&cap));
        assert!(!is_token_cap(&other));
    }
}
