//! The window's words for what the library crates report. Their errors and progress events carry a stable
//! code and its arguments and speak English in `Display` (for logs); here each becomes a message of the
//! catalogues in the language the window shows.

use dub_asr::{AsrError, SpeakerMatchError};
use dub_core::glossary::GlossaryError;
use dub_llm::LlmError;
use dub_sep::SepError;
use dub_translate::{AnswerProblem, LineFailure, Note, Reject, TranslateError};

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

impl Localize for AsrError {
    fn localize(&self) -> String {
        match self {
            AsrError::Parakeet(error) | AsrError::Io(error) => t!("asr-engine", error = error.clone()),
            AsrError::WavRead(path, error) => t!("asr-wav-read", path = path.clone(), error = error.clone()),
            AsrError::Resample(error) => t!("asr-resample", error = error.clone()),
            AsrError::SpeakerCount { max } => t!("asr-speaker-count", max = *max),
            AsrError::Speakers(e) => e.localize(),
        }
    }
}

impl Localize for SpeakerMatchError {
    fn localize(&self) -> String {
        match self {
            SpeakerMatchError::Embedding { speaker, error } => t!("speakers-embedding", speaker = *speaker, error = error.clone()),
            SpeakerMatchError::NoSample { speaker } => t!("speakers-no-sample", speaker = *speaker),
            SpeakerMatchError::TooManyVoices { given, found } => t!("speakers-too-many-voices", given = *given, found = *found),
            SpeakerMatchError::BadEmbedding => t!("speakers-bad-embedding"),
            SpeakerMatchError::DimensionChanged => t!("speakers-dimension-changed"),
            SpeakerMatchError::MoreVoicesThanSpeakers => t!("speakers-more-voices"),
            SpeakerMatchError::Unmatched => t!("speakers-unmatched"),
        }
    }
}

impl Localize for SepError {
    fn localize(&self) -> String {
        match self {
            SepError::EngineMissing(path) => t!("sep-engine-missing", path = shown(path)),
            SepError::ModelMissing(path) => t!("sep-model-missing", path = shown(path)),
            SepError::Spawn(error) => t!("sep-spawn", error = error.clone()),
            SepError::EngineFailed { code: Some(code), tail } => t!("sep-engine-failed", code = *code, tail = tail.clone()),
            SepError::EngineFailed { code: None, tail } => t!("sep-engine-killed", tail = tail.clone()),
            SepError::NoOutput(path) => t!("sep-no-output", path = shown(path)),
            SepError::Wav(error) => t!("sep-audio-io", error = error.clone()),
        }
    }
}

impl Localize for TranslateError {
    fn localize(&self) -> String {
        match self {
            TranslateError::Llm(e) => e.localize(),
            TranslateError::Frame(error) => t!("translate-frame", error = error.clone()),
            TranslateError::Audio(error) => t!("translate-audio", error = error.clone()),
            TranslateError::Empty { lines, last: Some(last) } => t!("translate-empty", lines = *lines, reason = last.localize()),
            TranslateError::Empty { lines, last: None } => t!("translate-empty-no-reason", lines = *lines),
            TranslateError::Contract { problem, answer: Some(answer) } => {
                t!("translate-contract-answer", problem = problem.localize(), answer = answer.clone())
            }
            TranslateError::Contract { problem, answer: None } => t!("translate-contract", problem = problem.localize()),
        }
    }
}

impl Localize for AnswerProblem {
    fn localize(&self) -> String {
        match self {
            AnswerProblem::NoJsonObject => t!("answer-no-json-object"),
            AnswerProblem::NotJson(error) => t!("answer-not-json", error = error.clone()),
            AnswerProblem::NoTerms => t!("answer-no-terms"),
        }
    }
}

impl Localize for Reject {
    fn localize(&self) -> String {
        match self {
            Reject::Missing => t!("line-missing"),
            Reject::Cut => t!("line-cut"),
            Reject::Untranslated => t!("line-untranslated"),
            Reject::Echo => t!("line-echo"),
            Reject::TooShort { got, min } => t!("line-too-short", got = *got, min = *min),
            Reject::TooLong { got, max } => t!("line-too-long", got = *got, max = *max),
            Reject::Loop(gram) => t!("line-loop", gram = gram.clone()),
            Reject::Term(term) => t!("line-term-missing", term = term.clone()),
        }
    }
}

impl Localize for LineFailure {
    fn localize(&self) -> String {
        match self {
            LineFailure::Rejected(reason) => reason.localize(),
            LineFailure::Error(error) => error.localize(),
        }
    }
}

impl Localize for Note<'_> {
    fn localize(&self) -> String {
        match self {
            Note::FormatJson { model } => t!("translate-format-json", model = model.to_string()),
            Note::FormatJsonProbe { model } => t!("translate-format-json-probe", model = model.to_string()),
            Note::FormatNumbered { model } => t!("translate-format-numbered", model = model.to_string()),
            Note::SchemaIgnored { model: Some(model) } => t!("translate-schema-ignored", model = model.to_string()),
            Note::SchemaIgnored { model: None } => t!("translate-schema-ignored-server"),
            Note::SchemaRefused { model: Some(model), status, body } => {
                t!("translate-schema-refused", model = model.to_string(), status = status.to_string(), body = body.clone())
            }
            Note::SchemaRefused { model: None, status, body } => {
                t!("translate-schema-refused-server", status = status.to_string(), body = body.clone())
            }
            Note::LineFlawed { line, reason } => t!("translate-line-flawed", line = *line, reason = reason.localize()),
            Note::LineFailed { line, reason } => t!("translate-line-failed", line = *line, reason = reason.localize()),
            Note::LinesRejected { bad, total, reasons } => {
                let reasons: Vec<String> =
                    reasons.iter().map(|(line, why)| t!("translate-line-reason", line = *line, reason = why.localize())).collect();
                t!("translate-lines-rejected", bad = *bad, total = *total, reasons = reasons.join("; "))
            }
            Note::BatchStopped { first, last, error } => {
                t!("translate-batch-stopped", first = *first, last = *last, error = error.localize())
            }
            Note::BatchFailed { first, last, error } => t!("translate-batch-failed", first = *first, last = *last, error = error.localize()),
            Note::LayoutNoVision => t!("translate-layout-no-vision"),
            Note::LayoutNotNeeded => t!("translate-layout-not-needed"),
            Note::Layout { sub_style, titles, brands } => {
                t!("translate-layout", sub_style = sub_style.to_string(), titles = titles.join(", "), brands = brands.join(", "))
            }
            Note::LayoutFailed { error } => t!("translate-layout-failed", error = error.localize()),
            Note::SceneFailed { error } => t!("translate-scene-failed", error = error.localize()),
            Note::SceneNoVision => t!("translate-scene-no-vision"),
            Note::AudioFailed { error } => t!("translate-audio-failed", error = error.localize()),
            Note::ContextTrimmed { chars, budget } => t!("translate-context-trimmed", chars = *chars, budget = *budget),
            Note::NamesSkipped { error } => t!("translate-names-skipped", error = error.localize()),
            Note::Chunks { lines, chunks, terms, names } => {
                t!("translate-chunks", lines = *lines, chunks = *chunks, terms = *terms, names = *names)
            }
            Note::Done { translated, flawed, untranslated } => {
                t!("translate-pass-done", translated = *translated, flawed = *flawed, untranslated = *untranslated)
            }
            Note::GlossaryPass { pass, passes } => t!("glossary-pass", pass = *pass, passes = *passes),
            Note::GlossarySchemaRefused { status } => t!("glossary-schema-refused", status = status.to_string()),
            Note::ContentType { decided, votes, frames } => {
                t!("content-type-decided", decided = decided.to_string(), votes = *votes, frames = *frames)
            }
        }
    }
}
