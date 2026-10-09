//! The window's words for what the library crates report. Their errors and progress events carry a stable
//! code and its arguments and speak English in `Display` (for logs); here each becomes a message of the
//! catalogues in the language the window shows.

use dub_core::glossary::GlossaryError;

/// A report of a library crate as the window reads it, in the window's language.
pub trait Localize {
    fn localize(&self) -> String;
}

impl Localize for GlossaryError {
    fn localize(&self) -> String {
        match self {
            GlossaryError::OverLimit { total, max } => t!("glossary-over-limit", total = *total, max = *max),
            GlossaryError::EmptyTerm { entry } => t!("glossary-empty-term", entry = *entry),
            GlossaryError::FieldTooLong { entry, term, max } => {
                t!("glossary-field-too-long", entry = *entry, term = term.clone(), max = *max)
            }
            GlossaryError::Duplicate { term, first, second } => {
                t!("glossary-duplicate", term = term.clone(), first = *first, second = *second)
            }
            GlossaryError::TsvKeep { line, value } => t!("glossary-tsv-keep", line = *line, value = value.clone()),
            GlossaryError::TsvEmptyTerm { line } => t!("glossary-tsv-empty-term", line = *line),
            GlossaryError::OneOf => t!("glossary-one-of"),
            GlossaryError::Nothing => t!("glossary-nothing"),
        }
    }
}
