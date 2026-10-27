//! The server's config file (app/server/README.md, «Config» and «Model entries»): what the studio writes before it
//! starts the process.

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cuda,
    Vulkan,
    Cpu,
    /// Apple's GPU on a Mac.
    Metal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Offline,
    Streaming,
}

/// One model of the server. `id` is what a request names; family, task and mode are always written out, as the server
/// guide asks.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub family: String,
    pub path: PathBuf,
    pub task: String,
    pub mode: Mode,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub load_options: Map<String, Value>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub session_options: Map<String, Value>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub default_request_options: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy_timeout_ms: Option<u64>,
    /// The rate of the PCM this model's speech deltas carry; the deltas themselves do not say it.
    #[serde(skip)]
    pub output_rate: Option<u32>,
}

fn options(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

impl ModelEntry {
    fn new(id: &str, family: &str, path: &Path, task: &str, mode: Mode) -> ModelEntry {
        ModelEntry {
            id: id.to_string(),
            family: family.to_string(),
            path: path.to_path_buf(),
            task: task.to_string(),
            mode,
            load_options: Map::new(),
            session_options: Map::new(),
            default_request_options: Map::new(),
            busy_timeout_ms: None,
            output_rate: None,
        }
    }

    /// Higgs Audio v3 voice cloning (docs/models/higgs_audio_tts.md: route `clon`), streamed; its codec is 24 kHz mono.
    pub fn higgs_tts(id: &str, gguf: &Path) -> ModelEntry {
        ModelEntry { output_rate: Some(24_000), ..ModelEntry::new(id, "higgs_audio_tts", gguf, "clon", Mode::Streaming) }
    }

    /// Parakeet-TDT recognition of a whole file; long files switch to long-form windows past 30 s.
    pub fn parakeet_tdt(id: &str, gguf: &Path) -> ModelEntry {
        ModelEntry {
            session_options: options(json!({ "parakeet_tdt.offline_mode": "auto" })),
            ..ModelEntry::new(id, "parakeet_tdt", gguf, "asr", Mode::Offline)
        }
    }

    /// CrisperWhisper recognition of a whole file with word timestamps, in the cleaned (`intended`) transcription:
    /// `verbatim` writes numbers as words. `model` is the model's folder or GGUF.
    pub fn crisperwhisper(id: &str, model: &Path) -> ModelEntry {
        ModelEntry {
            default_request_options: options(json!({ "transcription_mode": "intended", "return_timestamps": true })),
            ..ModelEntry::new(id, "crisperwhisper", model, "asr", Mode::Offline)
        }
    }

    /// Nemotron 3.5 ASR recognition of a whole file, told its language as a locale (`ru-RU`); a recording longer
    /// than one encoder pass is cut at its quietest points by the model. `gguf` is the model's package or a
    /// transcribe.cpp GGUF.
    pub fn nemotron_asr(id: &str, gguf: &Path) -> ModelEntry {
        ModelEntry::new(id, "nemotron_asr", gguf, "asr", Mode::Offline)
    }

    /// Nemotron 3 Diarization of a whole file, up to eight speakers in the order they first speak. `model` is the
    /// model's package or the studios' `nemotron3_diar_v3.onnx`.
    pub fn nemotron_3_diar(id: &str, model: &Path) -> ModelEntry {
        ModelEntry::new(id, "nemotron_3_diar", model, "diar", Mode::Offline)
    }

    /// Supertonic 3 speech in its preset voices (request options voice_id, num_inference_steps, speed, seed),
    /// streamed at 44.1 kHz mono. `model` is the model's package or the studios' int8 export folder (onnx/,
    /// voice_styles/).
    pub fn supertonic(id: &str, model: &Path) -> ModelEntry {
        ModelEntry { output_rate: Some(44_100), ..ModelEntry::new(id, "supertonic", model, "tts", Mode::Streaming) }
    }

    /// HTDemucs 6-stem separation (family `htdemucs_6stems`, task `sep`): drums, bass, vocals, other, guitar and piano
    /// as the result's named audio outputs. `gguf` is the package's `htdemucs-6stems-f16.gguf`.
    pub fn htdemucs(id: &str, gguf: &Path) -> ModelEntry {
        ModelEntry::new(id, "htdemucs_6stems", gguf, "sep", Mode::Offline)
    }

    /// MuScriptor audio-to-MIDI (task `midi`): the MIDI file comes back as a `midi` artifact, or note-event JSON with
    /// request option `output_format: json`. `model` is the model's safetensors folder or its GGUF.
    pub fn muscriptor(id: &str, model: &Path) -> ModelEntry {
        ModelEntry::new(id, "muscriptor", model, "midi", Mode::Offline)
    }

    /// SheetSage2 score transcription (task `midi`): the score comes back as an artifact. `gguf` is yue2.cpp's head
    /// GGUF with the MERT GGUF of the same quant beside it (`SheetSage2-Q8_0.gguf` → `MERT-v2-FullSong-Q8_0.gguf`), or
    /// audio.cpp's self-contained package.
    pub fn sheetsage2(id: &str, gguf: &Path) -> ModelEntry {
        ModelEntry::new(id, "sheetsage2", gguf, "midi", Mode::Offline)
    }
}

/// The whole config. `port` is chosen by [`crate::Server::start`].
#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    pub exe: PathBuf,
    pub backend: Backend,
    pub device: u32,
    pub threads: u32,
    pub models: Vec<ModelEntry>,
    /// Folder of the config file and the server's log.
    pub data_dir: PathBuf,
    /// The server's trace log (`--log --log-file`): every stage's timings, which slow the run; an error reaches the
    /// client in the answer either way.
    pub log: bool,
}

impl ServerConfig {
    pub(crate) fn json(&self, port: u16) -> Value {
        json!({
            "host": "127.0.0.1",
            "port": port,
            "backend": self.backend,
            "device": self.device,
            "threads": self.threads.max(1),
            "lazy_load": true,
            "max_loaded_models": 0,
            "log_request_body": false,
            "models": self.models,
        })
    }

    pub(crate) fn model(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("audiocpp_server.json")
    }

    pub fn log_path(&self) -> PathBuf {
        self.data_dir.join("audiocpp_server.log")
    }

    /// The server's own output (its start, its errors), kept whether or not the trace log is on.
    pub fn output_path(&self) -> PathBuf {
        self.data_dir.join("audiocpp_server.out")
    }

    /// Where an error message takes the server's last words from.
    pub(crate) fn tail_path(&self) -> PathBuf {
        if self.log { self.log_path() } else { self.output_path() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_names_every_model_as_the_server_guide_asks() {
        let cfg = ServerConfig {
            exe: PathBuf::from("audiocpp_server.exe"),
            backend: Backend::Cuda,
            device: 0,
            threads: 8,
            models: vec![
                ModelEntry::higgs_tts("higgs", Path::new("m/higgs-q8_0/q8_0.gguf")),
                ModelEntry::parakeet_tdt("parakeet", Path::new("m/asr-gguf-parakeet-v3/parakeet-tdt-0.6b-v3-Q8_0.gguf")),
                ModelEntry::crisperwhisper("crisperwhisper", Path::new("m/crisperwhisper2-turbo")),
                ModelEntry::nemotron_asr("nemotron", Path::new("m/asr-gguf-nemotron/nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf")),
                ModelEntry::htdemucs("htdemucs", Path::new("m/HTDemucs-6stems-GGUF/htdemucs-6stems-f16.gguf")),
                ModelEntry::muscriptor("muscriptor", Path::new("m/muscriptor")),
                ModelEntry::sheetsage2("sheetsage2", Path::new("m/sheetsage/SheetSage2-Q8_0.gguf")),
            ],
            data_dir: PathBuf::from("d"),
            log: false,
        };
        let v = cfg.json(4321);
        assert_eq!(v["port"], 4321);
        assert_eq!(v["backend"], "cuda");
        assert_eq!(v["lazy_load"], true);
        let m = v["models"].as_array().unwrap();
        assert_eq!((m[0]["family"].as_str(), m[0]["task"].as_str(), m[0]["mode"].as_str()), (Some("higgs_audio_tts"), Some("clon"), Some("streaming")));
        assert!(m[0].get("output_rate").is_none(), "the rate is the client's, not a config key");
        assert_eq!(m[1]["session_options"]["parakeet_tdt.offline_mode"], "auto");
        assert_eq!(m[2]["default_request_options"]["transcription_mode"], "intended");
        assert_eq!(m[2]["default_request_options"]["return_timestamps"], true);
        assert!(m[1].get("default_request_options").is_none());
        assert_eq!((m[3]["family"].as_str(), m[3]["task"].as_str(), m[3]["mode"].as_str()), (Some("nemotron_asr"), Some("asr"), Some("offline")));
        assert_eq!((m[4]["family"].as_str(), m[4]["task"].as_str(), m[4]["mode"].as_str()), (Some("htdemucs_6stems"), Some("sep"), Some("offline")));
        assert_eq!((m[5]["family"].as_str(), m[5]["task"].as_str()), (Some("muscriptor"), Some("midi")));
        assert_eq!((m[6]["family"].as_str(), m[6]["task"].as_str()), (Some("sheetsage2"), Some("midi")));
    }
}
