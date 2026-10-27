//! The language CrisperWhisper is told. A chosen language goes as its Whisper code; «auto» is the language VoxLingua107
//! (transcribe.cpp) hears in the first 30 s, as a Whisper code. A language Whisper does not know is an error naming it,
//! never English by default.

use crate::AcppError;

/// Whisper's languages (the `<|xx|>` tokens of CrisperWhisper 2.0's tokenizer, the same 100 as openai/whisper).
pub const WHISPER_LANGUAGES: &[&str] = &[
    "af", "am", "ar", "as", "az", "ba", "be", "bg", "bn", "bo", "br", "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es", "et",
    "eu", "fa", "fi", "fo", "fr", "gl", "gu", "ha", "haw", "he", "hi", "hr", "ht", "hu", "hy", "id", "is", "it", "ja", "jw", "ka",
    "kk", "km", "kn", "ko", "la", "lb", "ln", "lo", "lt", "lv", "mg", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my", "ne", "nl",
    "nn", "no", "oc", "pa", "pl", "ps", "pt", "ro", "ru", "sa", "sd", "si", "sk", "sl", "sn", "so", "sq", "sr", "su", "sv", "sw",
    "ta", "te", "tg", "th", "tk", "tl", "tr", "tt", "uk", "ur", "uz", "vi", "yi", "yo", "yue", "zh",
];

/// VoxLingua107 languages Whisper has no token for, with the names the language-id model gives them.
const NOT_IN_WHISPER: &[(&str, &str)] = &[
    ("ab", "Abkhazian"),
    ("ceb", "Cebuano"),
    ("eo", "Esperanto"),
    ("gn", "Guarani"),
    ("gv", "Manx"),
    ("ia", "Interlingua"),
    ("sco", "Scots"),
    ("war", "Waray"),
];

/// The audio the language id listens to, at 16 kHz.
pub const LANGUAGE_ID_SECS: usize = 30;

/// A language code (VoxLingua107's, or a chosen one such as `pt-BR`) as Whisper's.
pub fn whisper_code(code: &str) -> Result<&'static str, AcppError> {
    let lower = code.trim().to_ascii_lowercase();
    let primary = lower.split(['-', '_']).next().unwrap_or("");
    let code = match primary {
        "iw" | "he" => "he",
        "jv" | "jw" => "jw",
        "fil" | "tl" => "tl",
        "nb" | "no" => "no",
        other => other,
    };
    if let Some((_, name)) = NOT_IN_WHISPER.iter().find(|(c, _)| *c == code) {
        return Err(AcppError::Language(format!("the recording is heard as {name}, which CrisperWhisper does not know: choose the source language")));
    }
    WHISPER_LANGUAGES
        .iter()
        .copied()
        .find(|c| *c == code)
        .ok_or_else(|| AcppError::Language(format!("the language «{code}» has no Whisper code: choose the source language")))
}

/// The Whisper language of 16 kHz mono speech by VoxLingua107 on transcribe.cpp (`engine_dir`, the language-id GGUF
/// `model`), from its first [`LANGUAGE_ID_SECS`] seconds.
#[cfg(feature = "langid")]
pub fn heard_language(engine_dir: &std::path::Path, model: &std::path::Path, pcm16k: &[f32]) -> Result<&'static str, AcppError> {
    if pcm16k.len() < dub_tcpp::LANGID_MIN_SAMPLES {
        return Err(AcppError::Language("the recording is too short to hear its language: choose the source language".into()));
    }
    let head = &pcm16k[..pcm16k.len().min(LANGUAGE_ID_SECS * 16_000)];
    let guess = dub_tcpp::detect_language(engine_dir, model, head, &[]).map_err(AcppError::Language)?;
    let (code, _) = guess.ranked.first().ok_or_else(|| AcppError::Language("the language id gave no language".into()))?;
    whisper_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxlingua_codes_become_whisper_codes_or_a_named_error() {
        assert_eq!(whisper_code("ru").unwrap(), "ru");
        assert_eq!(whisper_code("pt-BR").unwrap(), "pt");
        assert_eq!(whisper_code("iw").unwrap(), "he");
        assert_eq!(whisper_code("jw").unwrap(), "jw");
        assert_eq!(whisper_code("haw").unwrap(), "haw");
        assert!(whisper_code("eo").unwrap_err().to_string().contains("Esperanto"));
        assert!(whisper_code("xx").is_err());
        let vox = "ab af am ar as az ba be bg bn bo br bs ca ceb cs cy da de el en eo es et eu fa fi fo fr gl gn gu gv ha haw hi hr ht hu hy ia id is it iw ja jw ka kk km kn ko la lb ln lo lt lv mg mi mk ml mn mr ms mt my ne nl nn no oc pa pl ps pt ro ru sa sco sd si sk sl sn so sq sr su sv sw ta te tg th tk tl tr tt uk ur uz vi war yi yo zh";
        let (ok, refused): (Vec<&str>, Vec<&str>) = vox.split(' ').partition(|c| whisper_code(c).is_ok());
        assert_eq!((ok.len(), refused.len()), (99, 8), "{refused:?}");
    }
}
