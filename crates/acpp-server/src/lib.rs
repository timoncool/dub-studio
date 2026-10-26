//! Client of `audiocpp_server` (audio.cpp): starts the server process with a config of its models, waits until it
//! answers, speaks with Higgs Audio v3 as a stream of PCM deltas and recognises speech with Parakeet-TDT or
//! CrisperWhisper into words and segments in seconds, telling CrisperWhisper the language (`language`), and finds
//! who speaks when with Nemotron 3 Diarization.
//!
//! The API is blocking: the studios call engines from their worker threads. Cancelling a speech request closes its
//! connection; the server stops generating at its next delta, when writing to the closed socket fails.

mod asr;
pub mod asset;
mod config;
mod diar;
pub mod higgs;
pub mod language;
mod process;
mod registry;
mod speech;
mod sse;
mod tasks;

pub use tasks::{task_artifact, task_artifacts, task_audio, TaskArtifact, TaskAudio};
pub use asr::{TranscribeRequest, Transcript, TranscriptSpan, TranscriptWord};
pub use config::{Backend, Mode, ModelEntry, ServerConfig};
pub use diar::SpeakerTurn;
pub use process::Server;
pub use registry::{Servers, SERVERS};
pub use speech::{Flow, Speech, SpeechDone, SpeechRequest, VoiceRef};

/// What went wrong talking to the server; each says enough to show it to a user as it is.
#[derive(Debug, thiserror::Error)]
pub enum AcppError {
    #[error("audiocpp_server {exe} did not start: {error}")]
    Spawn { exe: String, error: String },
    #[error("audiocpp_server did not answer within {secs} s; its log ends with: {log}")]
    NotReady { secs: u64, log: String },
    #[error("audiocpp_server stopped (exit code {code:?}); its log ends with: {log}")]
    Exited { code: Option<i32>, log: String },
    #[error("audiocpp_server answered {status} ({kind}): {message}")]
    Server { status: u16, kind: String, message: String },
    #[error("audiocpp_server sent nothing for {secs} s")]
    Stalled { secs: u64 },
    #[error("audiocpp_server: {0}")]
    Protocol(String),
    #[error("{path}: {why}")]
    Input { path: String, why: String },
    #[error("{0}")]
    Language(String),
    #[error("{0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
}

impl AcppError {
    /// A stable code for the error (logs, telemetry, the studio's mapping to its own messages).
    pub fn code(&self) -> &'static str {
        match self {
            AcppError::Spawn { .. } => "acpp_spawn",
            AcppError::NotReady { .. } => "acpp_not_ready",
            AcppError::Exited { .. } => "acpp_exited",
            AcppError::Server { .. } => "acpp_server",
            AcppError::Stalled { .. } => "acpp_stalled",
            AcppError::Protocol(_) => "acpp_protocol",
            AcppError::Input { .. } => "acpp_input",
            AcppError::Language(_) => "acpp_language",
            AcppError::Io(_) => "acpp_io",
            AcppError::Cancelled => "cancelled",
        }
    }
}
