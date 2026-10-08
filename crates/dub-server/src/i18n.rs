//! The server's words in the language the window shows. Every message a person reads - progress, an
//! error, a setup status - is a key of the Fluent catalogues in `locales/<lang>/server.ftl`, looked up
//! in the language the window last reported, so the six interface languages read the server too.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::RwLock;

use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{static_loader, LanguageIdentifier, Loader};

static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "en",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

/// The interface languages; the window reports one of these.
pub const LANGUAGES: [&str; 6] = ["en", "ru", "es", "fr", "pt", "zh"];

static CURRENT: RwLock<Option<LanguageIdentifier>> = RwLock::new(None);

/// The language the window shows now; English until it reports one.
pub fn language() -> LanguageIdentifier {
    CURRENT.read().ok().and_then(|current| current.clone()).unwrap_or_else(|| "en".parse().expect("a language tag"))
}

pub fn set_language(code: &str) -> Result<(), String> {
    if !LANGUAGES.contains(&code) {
        return Err(format!("unknown interface language {code}; one of {}", LANGUAGES.join(", ")));
    }
    let id: LanguageIdentifier = code.parse().map_err(|_| format!("not a language tag: {code}"))?;
    *CURRENT.write().map_err(|_| "the language lock is poisoned".to_string())? = Some(id);
    Ok(())
}

/// The message `key` in the current language with its arguments. A key no catalogue has comes back as
/// the key itself, which the catalogue test catches before it ships.
pub fn tr(key: &str, args: Vec<(&'static str, FluentValue<'static>)>) -> String {
    let map: HashMap<Cow<'static, str>, FluentValue<'static>> = args.into_iter().map(|(name, value)| (Cow::Borrowed(name), value)).collect();
    LOCALES.try_lookup_with_args(&language(), key, &map).unwrap_or_else(|| key.to_string())
}

/// `t!("key")` or `t!("key", name = value, ...)`: a message of the catalogues in the current language.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key, Vec::new())
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr($key, vec![$((stringify!($name), fluent_templates::fluent_bundle::FluentValue::from($value))),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the code asks for is in every language's catalogue.
    #[test]
    fn every_key_is_in_every_catalogue() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let pattern = regex::Regex::new(r#"t!\(\s*"([a-z0-9-]+)""#).unwrap();
        let mut keys = std::collections::BTreeSet::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.file_name().is_some_and(|name| name == "i18n.rs") {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    for found in pattern.captures_iter(&std::fs::read_to_string(&path).unwrap()) {
                        keys.insert(found[1].to_string());
                    }
                }
            }
        }
        assert!(!keys.is_empty());
        let defined = regex::Regex::new(r"(?m)^([a-z0-9-]+)\s*=").unwrap();
        let catalogues = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let mut missing = Vec::new();
        for code in LANGUAGES {
            let mut have = std::collections::BTreeSet::new();
            for entry in std::fs::read_dir(catalogues.join(code)).unwrap().flatten() {
                for found in defined.captures_iter(&std::fs::read_to_string(entry.path()).unwrap()) {
                    have.insert(found[1].to_string());
                }
            }
            missing.extend(keys.iter().filter(|key| !have.contains(*key)).map(|key| format!("{code}: {key}")));
        }
        assert!(missing.is_empty(), "keys missing from catalogues: {missing:?}");
    }

    #[test]
    fn the_language_follows_the_window_and_counts_in_russian() {
        set_language("ru").unwrap();
        assert_eq!(t!("asr-windowed", speakers = 2), "транскрипция по окнам на GPU (2 спикера)");
        assert_eq!(t!("asr-windowed", speakers = 5), "транскрипция по окнам на GPU (5 спикеров)");
        set_language("en").unwrap();
        assert_eq!(t!("asr-windowed", speakers = 1), "transcribing in windows on the GPU (1 speaker)");
        assert!(set_language("xx").is_err());
    }
}
