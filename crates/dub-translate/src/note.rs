//! Записи журнала перевода: что стадия сообщает по ходу работы. Вызывающий получает их данными (код и
//! аргументы) и сам решает, как их показать; Display — та же запись по-английски для stderr и журнала.

use std::fmt;

use serde_json::Value;

use crate::{LineFailure, Reject, TranslateError};

/// Одна запись журнала перевода.
#[derive(Debug)]
pub enum Note<'a> {
    /// Модель отвечает JSON-объектом по схеме.
    FormatJson { model: &'a str },
    /// Схема на этом сервере ещё не проверена: первый ответ покажет, держит ли он её.
    FormatJsonProbe { model: &'a str },
    /// Модель OpenRouter не заявляет structured_outputs: нумерованные строки.
    FormatNumbered { model: &'a str },
    /// Сервер принял схему, но модель ответила нумерованными строками; `model` — None у своего сервера.
    SchemaIgnored { model: Option<&'a str> },
    /// Сервер отверг ответ по схеме; дальше нумерованные строки.
    SchemaRefused { model: Option<&'a str>, status: &'a str, body: String },
    /// Строка `line` после всех попыток оставлена с изъяном.
    LineFlawed { line: usize, reason: &'a Reject },
    /// Строка `line` осталась без перевода.
    LineFailed { line: usize, reason: &'a LineFailure },
    /// `bad` строк пакета из `total` не прошли проверку; причины по номерам строк.
    LinesRejected { bad: usize, total: usize, reasons: &'a [(usize, LineFailure)] },
    /// Пакет строк first..=last не удался, и перевод остановлен.
    BatchStopped { first: usize, last: usize, error: &'a TranslateError },
    /// Пакет строк first..=last не удался; строки спрашиваются меньшими пакетами.
    BatchFailed { first: usize, last: usize, error: &'a TranslateError },
    /// Раскладка кадра пропущена: vision-модели нет.
    LayoutNoVision,
    /// Раскладка кадра не нужна: субтитры не вжигаются.
    LayoutNotNeeded,
    /// Раскладка кадра прочитана.
    Layout { sub_style: &'a Value, titles: &'a [String], brands: &'a [String] },
    LayoutFailed { error: &'a TranslateError },
    SceneFailed { error: &'a TranslateError },
    /// Контекст сцены пропущен: vision-модели нет.
    SceneNoVision,
    AudioFailed { error: &'a TranslateError },
    /// Блок контекста длиннее бюджета и обрезан до него.
    ContextTrimmed { chars: usize, budget: usize },
    /// Авто-глоссарий имён не собран; перевод идёт без него.
    NamesSkipped { error: &'a TranslateError },
    /// Длинный скрипт разбит на пакеты.
    Chunks { lines: usize, chunks: usize, terms: usize, names: usize },
    /// Проход перевода закончен.
    Done { translated: usize, flawed: usize, untranslated: usize },
    /// Проход модели по тексту за глоссарием.
    GlossaryPass { pass: usize, passes: usize },
    /// Сервер отверг ответ глоссария по схеме; дальше JSON текстом.
    GlossarySchemaRefused { status: &'a str },
    /// Тип контента по голосам кадров: `votes` внятных ответов из `frames`.
    ContentType { decided: &'a str, votes: usize, frames: usize },
}

impl Note<'_> {
    /// Стабильный код записи (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            Note::FormatJson { .. } => "translate_format_json",
            Note::FormatJsonProbe { .. } => "translate_format_json_probe",
            Note::FormatNumbered { .. } => "translate_format_numbered",
            Note::SchemaIgnored { .. } => "translate_schema_ignored",
            Note::SchemaRefused { .. } => "translate_schema_refused",
            Note::LineFlawed { .. } => "translate_line_flawed",
            Note::LineFailed { .. } => "translate_line_failed",
            Note::LinesRejected { .. } => "translate_lines_rejected",
            Note::BatchStopped { .. } => "translate_batch_stopped",
            Note::BatchFailed { .. } => "translate_batch_failed",
            Note::LayoutNoVision => "translate_layout_no_vision",
            Note::LayoutNotNeeded => "translate_layout_not_needed",
            Note::Layout { .. } => "translate_layout",
            Note::LayoutFailed { .. } => "translate_layout_failed",
            Note::SceneFailed { .. } => "translate_scene_failed",
            Note::SceneNoVision => "translate_scene_no_vision",
            Note::AudioFailed { .. } => "translate_audio_failed",
            Note::ContextTrimmed { .. } => "translate_context_trimmed",
            Note::NamesSkipped { .. } => "translate_names_skipped",
            Note::Chunks { .. } => "translate_chunks",
            Note::Done { .. } => "translate_done",
            Note::GlossaryPass { .. } => "glossary_pass",
            Note::GlossarySchemaRefused { .. } => "glossary_schema_refused",
            Note::ContentType { .. } => "content_type",
        }
    }
}

impl fmt::Display for Note<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let server = |model: &Option<&str>| model.unwrap_or("the server").to_string();
        match self {
            Note::FormatJson { model } => write!(f, "translation: JSON schema answers ({model})"),
            Note::FormatJsonProbe { model } => write!(f, "translation: trying JSON schema answers ({model}); numbered lines if refused"),
            Note::FormatNumbered { model } => {
                write!(f, "translation: numbered lines; the model {model} does not declare structured_outputs in the OpenRouter catalogue")
            }
            Note::SchemaIgnored { model } => {
                write!(f, "translation: {} accepted the JSON schema but answered with numbered lines; numbered lines from now on", server(model))
            }
            Note::SchemaRefused { model, status, body } => {
                write!(f, "translation: {} refused the JSON schema answer ({status}: {body}); numbered lines from now on", server(model))
            }
            Note::LineFlawed { line, reason } => write!(f, "translation: line {line} keeps a translation with a flaw: {reason}"),
            Note::LineFailed { line, reason } => write!(f, "translation: line {line} is not translated: {reason}"),
            Note::LinesRejected { bad, total, reasons } => {
                let reasons: Vec<String> = reasons.iter().map(|(line, why)| format!("{line}: {why}")).collect();
                write!(f, "translation: {bad} of {total} lines failed the check ({})", reasons.join("; "))
            }
            Note::BatchStopped { first, last, error } => {
                write!(f, "translation: the batch of lines {first}..{last} failed ({error}); the translation stopped")
            }
            Note::BatchFailed { first, last, error } => write!(f, "translation: the batch of lines {first}..{last} failed ({error})"),
            Note::LayoutNoVision => write!(f, "ctx vision layout: skipped (no vision model is chosen or available)"),
            Note::LayoutNotNeeded => write!(f, "ctx vision layout: skipped (subtitles are not burned in, no layout needed)"),
            Note::Layout { sub_style, titles, brands } => write!(f, "ctx vision: sub_style={sub_style} titles={titles:?} brands={brands:?}"),
            Note::LayoutFailed { error } => write!(f, "ctx vision skipped: {error}"),
            Note::SceneFailed { error } => write!(f, "ctx scene skipped: {error}"),
            Note::SceneNoVision => write!(f, "ctx scene: skipped (no vision model is chosen or available)"),
            Note::AudioFailed { error } => write!(f, "ctx audio skipped: {error}"),
            Note::ContextTrimmed { chars, budget } => write!(f, "ctx translate: the context block of {chars} chars is cut to {budget} (n_ctx guard)"),
            Note::NamesSkipped { error } => write!(f, "translation: the automatic name glossary is skipped ({error})"),
            Note::Chunks { lines, chunks, terms, names } => {
                write!(f, "ctx translate: {lines} lines -> {chunks} chunks (glossary: {terms} terms, names: {names})")
            }
            Note::Done { translated, flawed, untranslated } => {
                write!(f, "ctx translate: done, {translated} lines translated ({flawed} with a flaw), {untranslated} left in the source")
            }
            Note::GlossaryPass { pass, passes } => write!(f, "glossary: model pass {pass}/{passes}"),
            Note::GlossarySchemaRefused { status } => {
                write!(f, "glossary: the server refused the JSON schema answer ({status}); asking for JSON as text")
            }
            Note::ContentType { decided, votes, frames } => write!(f, "content type (Gemma): {decided} ({votes} valid votes of {frames})"),
        }
    }
}
