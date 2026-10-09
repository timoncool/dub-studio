//! The window's words for what the library crates report. Their errors and progress events carry a stable
//! code and its arguments and speak English in `Display` (for logs); here each becomes a message of the
//! catalogues in the language the window shows.

use dub_core::glossary::GlossaryError;
use dub_llm::LlmError;

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

fn shown(path: &std::path::Path) -> String {
    path.display().to_string()
}

impl Localize for LlmError {
    fn localize(&self) -> String {
        match self {
            LlmError::Spawn(error) => t!("llm-spawn-failed", error = error.clone()),
            LlmError::BinaryMissing(path) => t!("llm-llama-server-missing", path = shown(path)),
            LlmError::ModelMissing(path) => t!("llm-gguf-missing", path = shown(path)),
            LlmError::LogFile { path, error } => t!("llm-log-file", path = shown(path), error = error.clone()),
            LlmError::ExitedEarly { status, stderr } => t!("llm-exited-early", status = status.clone(), stderr = stderr.clone()),
            LlmError::NotReady { secs, port, stderr } => t!("llm-not-ready", secs = *secs, port = *port, stderr = stderr.clone()),
            LlmError::Http(error) => t!("llm-http", error = error.clone()),
            LlmError::Api(error) => t!("llm-api", error = error.clone()),
            LlmError::Rejected { status, body, .. } => t!("llm-rejected", status = status.clone(), body = body.clone()),
            LlmError::EmptyAnswer { model, finish_reason } if finish_reason.is_empty() => t!("llm-empty-answer", model = model.clone()),
            LlmError::EmptyAnswer { model, finish_reason } => {
                t!("llm-empty-answer-reason", model = model.clone(), reason = finish_reason.clone())
            }
            LlmError::CutShort { model, max_tokens } => t!("llm-cut-short", model = model.clone(), max_tokens = *max_tokens),
            LlmError::PromptCut { read, chars } => t!("llm-prompt-cut", read = *read, chars = *chars),
        }
    }
}
