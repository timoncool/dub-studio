//! Крейт audiocpp — самостоятельная FFI-обвязка над audiocpp_engine.dll (Higgs Audio v3 TTS).
//!
//! Низкоуровневый слой (`engine::Engine`) 1:1 повторяет C-ABI движка. Высокоуровневый `AudiocppEngine`
//! даёт API уровня dub-движка: load(dll) -> load_model(root, backend, ...) -> tts / voice_clone,
//! возвращающие (Vec<f32>, sample_rate). Кодирование WAV — через AudioResult::encode_pcm16_wav.

mod engine;

pub use engine::{
    add_dll_directory, backend_id, AudioChunkCallback, AudioResult, Engine, EngineError, ModelInfo,
    ProgressCallback,
};

use std::path::Path;
use std::sync::Arc;

/// Высокоуровневая обёртка над Engine для сценариев дубляжа.
pub struct AudiocppEngine {
    engine: Engine,
}

impl AudiocppEngine {
    /// Загрузить DLL движка. Каталог DLL автоматически добавляется в путь поиска зависимостей.
    pub fn load(dll_path: impl AsRef<Path>) -> Result<Self, EngineError> {
        Ok(Self {
            engine: Engine::load(dll_path.as_ref())?,
        })
    }

    /// Загрузить веса Higgs. backend: "cpu"|"cuda"|"vulkan"|"metal"; device — индекс GPU;
    /// threads — число потоков; weight_type — вариант квантизации (например "q8_0", пусто = дефолт модели).
    pub fn load_model(
        &self,
        model_root: impl AsRef<Path>,
        backend: &str,
        device: i32,
        threads: i32,
        weight_type: Option<&str>,
    ) -> Result<ModelInfo, EngineError> {
        let req = engine::LoadModelRequest {
            model_root: model_root.as_ref().to_string_lossy().into_owned(),
            backend: backend.to_string(),
            device,
            threads,
            weight_type: weight_type.map(|s| s.to_string()),
            session_options: None,
        };
        self.engine.load_model(&req)
    }

    pub fn version(&self) -> String {
        self.engine.version()
    }

    pub fn is_model_loaded(&self) -> bool {
        self.engine.is_model_loaded()
    }

    pub fn cancel(&self) {
        self.engine.cancel()
    }

    /// Прямой доступ к низкоуровневому движку (стрим-варианты, finish_sentence и т.п.).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    fn noop_progress() -> ProgressCallback {
        Arc::new(|_, _, _| {})
    }

    /// TTS одним семплом. opts_json — JSON-строка опций генерации движка (температура, seed и т.п.),
    /// пустая строка = дефолт. Возвращает (PCM f32, sample_rate).
    pub fn tts(&self, text: &str, opts_json: &str) -> Result<(Vec<f32>, i32), EngineError> {
        self.tts_with_progress(text, opts_json, Self::noop_progress())
    }

    pub fn tts_with_progress(
        &self,
        text: &str,
        opts_json: &str,
        progress: ProgressCallback,
    ) -> Result<(Vec<f32>, i32), EngineError> {
        let opts = parse_opts(opts_json)?;
        let res = self.engine.generate_tts(text, &opts, progress)?;
        Ok((res.samples, res.sample_rate))
    }

    /// Клон голоса: text озвучивается тембром из ref_wav. ref_text — расшифровка референса (может быть пустой).
    pub fn voice_clone(
        &self,
        text: &str,
        ref_wav: &str,
        ref_text: Option<&str>,
        opts_json: &str,
    ) -> Result<(Vec<f32>, i32), EngineError> {
        self.voice_clone_with_progress(text, ref_wav, ref_text, opts_json, Self::noop_progress())
    }

    pub fn voice_clone_with_progress(
        &self,
        text: &str,
        ref_wav: &str,
        ref_text: Option<&str>,
        opts_json: &str,
        progress: ProgressCallback,
    ) -> Result<(Vec<f32>, i32), EngineError> {
        let opts = parse_opts(opts_json)?;
        let res = self
            .engine
            .generate_voice_clone(text, ref_wav, ref_text, &opts, progress)?;
        Ok((res.samples, res.sample_rate))
    }

    /// Закодировать PCM f32 в PCM16 WAV (в память).
    pub fn encode_wav(samples: &[f32], sample_rate: i32, channels: i32) -> Vec<u8> {
        AudioResult {
            sample_rate,
            channels,
            samples: samples.to_vec(),
        }
        .encode_pcm16_wav()
    }
}

/// Разобрать JSON-опции; пустая строка -> пустой объект {}.
fn parse_opts(opts_json: &str) -> Result<serde_json::Value, EngineError> {
    let s = opts_json.trim();
    if s.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(s)
        .map_err(|e| EngineError::InvalidParam(format!("invalid opts_json: {e}")))
}
