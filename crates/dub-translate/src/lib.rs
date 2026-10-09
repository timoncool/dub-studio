//! dub-translate — стадия перевода + vision над dub-llm (Gemma через llama-server).
//!
//! Порт dubengine/translate.py (плоский MT + rewrite) и ctx_translate.py (единый Gemma-проход: vision
//! layout/scene + audio-контекст + перевод всего транскрипта с контекстом). Промпты и параметры сэмплинга
//! перенесены ДОСЛОВНО — они выверены на тест-сете продукта. Дефолт-модель Gemma-4 12B QAT + mmproj.

mod batch;
mod contract;
mod ctx;
mod extract;
mod gloss;
mod note;
mod seg;
mod text_fix;
mod translate;
mod vision;

pub use contract::{looks_untranslated, tgt_expects_non_latin, Contract, Reject};
pub use note::Note;
pub use ctx::{run as ctx_run, CtxConfig, CtxResult};
pub use extract::extract_glossary;
pub use seg::Seg;
pub use text_fix::{cyrillic_homoglyphs, fix_translation};
pub use translate::{rewrite as flat_rewrite, run as flat_run, run_with as flat_run_with, FlatOpts};
pub use vision::{analyze_layout, classify_content_type, is_counter, scene_context, Layout, FONTS};

use thiserror::Error;

/// Код языка -> английское имя для промпта Gemma. Полный набор Whisper large-v3 (99 языков) —
/// совпадает со списком, который распознаёт Whisper-ASR (истинный потолок source). Для незнакомого
/// кода вызывающая сторона подставляет сам код. Единый источник для translate.rs и ctx.rs.
pub const WHISPER_LANGS: &[(&str, &str)] = &[
    ("en", "English"), ("zh", "Chinese"), ("de", "German"), ("es", "Spanish"), ("ru", "Russian"),
    ("ko", "Korean"), ("fr", "French"), ("ja", "Japanese"), ("pt", "Portuguese"), ("tr", "Turkish"),
    ("pl", "Polish"), ("ca", "Catalan"), ("nl", "Dutch"), ("ar", "Arabic"), ("sv", "Swedish"),
    ("it", "Italian"), ("id", "Indonesian"), ("hi", "Hindi"), ("fi", "Finnish"), ("vi", "Vietnamese"),
    ("he", "Hebrew"), ("uk", "Ukrainian"), ("el", "Greek"), ("ms", "Malay"), ("cs", "Czech"),
    ("ro", "Romanian"), ("da", "Danish"), ("hu", "Hungarian"), ("ta", "Tamil"), ("no", "Norwegian"),
    ("th", "Thai"), ("ur", "Urdu"), ("hr", "Croatian"), ("bg", "Bulgarian"), ("lt", "Lithuanian"),
    ("la", "Latin"), ("mi", "Maori"), ("ml", "Malayalam"), ("cy", "Welsh"), ("sk", "Slovak"),
    ("te", "Telugu"), ("fa", "Persian"), ("lv", "Latvian"), ("bn", "Bengali"), ("sr", "Serbian"),
    ("az", "Azerbaijani"), ("sl", "Slovenian"), ("kn", "Kannada"), ("et", "Estonian"), ("mk", "Macedonian"),
    ("br", "Breton"), ("eu", "Basque"), ("is", "Icelandic"), ("hy", "Armenian"), ("ne", "Nepali"),
    ("mn", "Mongolian"), ("bs", "Bosnian"), ("kk", "Kazakh"), ("sq", "Albanian"), ("sw", "Swahili"),
    ("gl", "Galician"), ("mr", "Marathi"), ("pa", "Punjabi"), ("si", "Sinhala"), ("km", "Khmer"),
    ("sn", "Shona"), ("yo", "Yoruba"), ("so", "Somali"), ("af", "Afrikaans"), ("oc", "Occitan"),
    ("ka", "Georgian"), ("be", "Belarusian"), ("tg", "Tajik"), ("sd", "Sindhi"), ("gu", "Gujarati"),
    ("am", "Amharic"), ("yi", "Yiddish"), ("lo", "Lao"), ("uz", "Uzbek"), ("fo", "Faroese"),
    ("ht", "Haitian Creole"), ("ps", "Pashto"), ("tk", "Turkmen"), ("nn", "Nynorsk"), ("mt", "Maltese"),
    ("sa", "Sanskrit"), ("lb", "Luxembourgish"), ("my", "Burmese"), ("bo", "Tibetan"), ("tl", "Tagalog"),
    ("mg", "Malagasy"), ("as", "Assamese"), ("tt", "Tatar"), ("haw", "Hawaiian"), ("ln", "Lingala"),
    ("ha", "Hausa"), ("ba", "Bashkir"), ("jw", "Javanese"), ("su", "Sundanese"), ("yue", "Cantonese"),
];

#[derive(Debug, Clone, Error)]
pub enum TranslateError {
    #[error("llm: {0}")]
    Llm(#[from] dub_llm::LlmError),
    #[error("frame extract: {0}")]
    Frame(String),
    #[error("audio ctx: {0}")]
    Audio(String),
    /// Ни одна строка не переведена; `last` — причина последнего отказа, если она записана.
    #[error("MT returned empty for all {lines} segments: {}", last.as_ref().map_or("no reason recorded".to_string(), ToString::to_string))]
    Empty { lines: usize, last: Option<LineFailure> },
    /// Ответ модели не по контракту; `answer` — его начало.
    #[error("model answer: {problem}{}", answer.as_ref().map_or(String::new(), |a| format!("; answer: {a}")))]
    Contract { problem: AnswerProblem, answer: Option<String> },
}

impl TranslateError {
    /// Стабильный код ошибки (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            TranslateError::Llm(e) => e.code(),
            TranslateError::Frame(_) => "translate_frame",
            TranslateError::Audio(_) => "translate_audio",
            TranslateError::Empty { .. } => "translate_empty",
            TranslateError::Contract { .. } => "translate_contract",
        }
    }
}

/// Что не так с ответом модели.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AnswerProblem {
    #[error("the answer has no JSON object")]
    NoJsonObject,
    /// Объект не разобран; поле — ошибка разбора.
    #[error("the answer is not valid JSON: {0}")]
    NotJson(String),
    #[error("the answer has no terms list")]
    NoTerms,
}

impl AnswerProblem {
    pub fn code(&self) -> &'static str {
        match self {
            AnswerProblem::NoJsonObject => "answer_no_json_object",
            AnswerProblem::NotJson(_) => "answer_not_json",
            AnswerProblem::NoTerms => "answer_no_terms",
        }
    }
}

/// Почему строка осталась без перевода: ответ не прошёл проверку или запрос пакета не удался.
#[derive(Debug, Clone, Error)]
pub enum LineFailure {
    #[error("{0}")]
    Rejected(Reject),
    #[error("{0}")]
    Error(Box<TranslateError>),
}
