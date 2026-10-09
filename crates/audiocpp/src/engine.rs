//! FFI поверх audiocpp_engine.dll (Higgs Audio v3, C-ABI). Символы, типы и трамплины перенесены
//! из эталонного Higgs-Ultimate desktop/src-tauri/src/engine.rs; крейт самостоятельный (без Tauri/shine).
//!
//! Правила владения памятью C-ABI: samples и error в AudioResultRaw аллоцирует C-сторона, Rust ОБЯЗАН
//! вызвать audiocpp_free_result после копирования сэмплов. Прогресс идёт через extern "C" трамплин,
//! Box<ProgressCallback> передаётся как user-указатель и освобождается сразу после вызова generate*.

use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void, CStr, CString};
use std::os::raw::{c_float, c_int};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

/// Декодировать C-строку в String (null -> пусто). Общий декодер полей C-ABI движка.
/// # Safety: `ptr` должен быть либо null, либо валидным указателем на NUL-терминированную C-строку.
unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// Сериализовать options -> NUL-терминированную C-строку для передачи в движок. Общий шаг всех
/// generate_*: JSON -> CString (пустой JSON при ошибке сериализации, как unwrap_or_default).
fn opts_cstring(options: &serde_json::Value) -> Result<CString, EngineError> {
    CString::new(serde_json::to_string(options).unwrap_or_default())
        .map_err(|e| EngineError::InvalidParam(e.to_string()))
}

pub type ProgressCallback = Arc<dyn Fn(i32, i32, &str) + Send + Sync + 'static>;
pub type AudioChunkCallback = Arc<dyn Fn(i32, i32, i64, &[f32], bool) + Send + Sync + 'static>;

/// backend id для audiocpp_load_model: cpu=1, cuda=2, vulkan=3, metal=4.
pub fn backend_id(backend: &str) -> c_int {
    match backend {
        "cpu" => 1,
        "cuda" => 2,
        "vulkan" => 3,
        "metal" => 4,
        _ => 0,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub family: String,
    pub display_name: String,
    pub weight_type: String,
    pub model_root: String,
}

/// Результат генерации: PCM f32 + частота дискретизации + число каналов.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioResult {
    pub sample_rate: i32,
    pub channels: i32,
    pub samples: Vec<f32>,
}

impl AudioResult {
    /// PCM16 WAV в память (RIFF/WAVE), готовый для записи на диск.
    pub fn encode_pcm16_wav(&self) -> Vec<u8> {
        let channels = self.channels.max(1) as u16;
        let bits_per_sample: u16 = 16;
        let data_bytes = (self.samples.len() * 2) as u32;
        let riff_size = 36 + data_bytes;
        let byte_rate = (self.sample_rate as u32) * channels as u32 * bits_per_sample as u32 / 8;
        let block_align = channels * bits_per_sample / 8;

        let mut out = Vec::with_capacity(44 + data_bytes as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&riff_size.to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&(self.sample_rate as u32).to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits_per_sample.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_bytes.to_le_bytes());
        for &sample in &self.samples {
            let pcm = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            out.extend_from_slice(&pcm.to_le_bytes());
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadModelRequest {
    pub model_root: String,
    pub backend: String,
    pub device: i32,
    pub threads: i32,
    pub weight_type: Option<String>,
    pub session_options: Option<serde_json::Value>,
}

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("could not load the engine: {0}")]
    LibraryLoad(String),
    #[error("generation cancelled")]
    Cancelled,
    #[error("generation failed: {0}")]
    Generation(String),
    #[error("invalid parameter: {0}")]
    InvalidParam(String),
    #[error("streaming is not supported by this DLL")]
    StreamingUnsupported,
}

impl EngineError {
    /// Стабильный код ошибки (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            EngineError::LibraryLoad(_) => "tts_library_load",
            EngineError::Cancelled => "tts_cancelled",
            EngineError::Generation(_) => "tts_generation",
            EngineError::InvalidParam(_) => "tts_invalid_param",
            EngineError::StreamingUnsupported => "tts_streaming_unsupported",
        }
    }
}

// ─── типы FFI ──────────────────────────────────────────────────────────────

type CreateFn = unsafe extern "C" fn() -> *mut c_void;
type DestroyFn = unsafe extern "C" fn(*mut c_void);
type LoadModelFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    c_int,
    c_int,
    c_int,
    *const c_char,
    *const c_char,
) -> c_int;
type UnloadModelFn = unsafe extern "C" fn(*mut c_void);
type IsLoadedFn = unsafe extern "C" fn(*const c_void) -> bool;
type IsGeneratingFn = unsafe extern "C" fn(*const c_void) -> bool;
type CancelFn = unsafe extern "C" fn(*mut c_void);
type GetModelInfoFn = unsafe extern "C" fn(*const c_void, *mut ModelInfoRaw) -> c_int;
type GenerateTtsFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    ProgressCallbackC,
    *mut c_void,
    *mut AudioResultRaw,
) -> c_int;
type GenerateVoiceCloneFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    ProgressCallbackC,
    *mut c_void,
    *mut AudioResultRaw,
) -> c_int;
type GenerateFinishFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    *const c_char,
    ProgressCallbackC,
    *mut c_void,
    *mut AudioResultRaw,
) -> c_int;
type GenerateTtsStreamFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    ProgressCallbackC,
    AudioChunkCallbackC,
    *mut c_void,
    *mut AudioResultRaw,
) -> c_int;
type GenerateVoiceCloneStreamFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    ProgressCallbackC,
    AudioChunkCallbackC,
    *mut c_void,
    *mut AudioResultRaw,
) -> c_int;
type GenerateFinishStreamFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    *const c_char,
    ProgressCallbackC,
    AudioChunkCallbackC,
    *mut c_void,
    *mut AudioResultRaw,
) -> c_int;
type FreeResultFn = unsafe extern "C" fn(*mut AudioResultRaw);
type LastErrorFn = unsafe extern "C" fn(*const c_void) -> *const c_char;
type VersionFn = unsafe extern "C" fn() -> *const c_char;

#[repr(C)]
struct ModelInfoRaw {
    family: *const c_char,
    display_name: *const c_char,
    weight_type: *const c_char,
    model_root: *const c_char,
}

/// C-структура результата: samples и error аллоцирует C-сторона; Rust обязан вызвать audiocpp_free_result.
#[repr(C)]
struct AudioResultRaw {
    sample_rate: c_int,
    channels: c_int,
    sample_count: usize,
    samples: *mut c_float,
    error: *mut c_char,
}

impl AudioResultRaw {
    fn empty() -> Self {
        AudioResultRaw {
            sample_rate: 0,
            channels: 0,
            sample_count: 0,
            samples: std::ptr::null_mut(),
            error: std::ptr::null_mut(),
        }
    }
}

type ProgressCallbackC = unsafe extern "C" fn(c_int, c_int, *const c_char, *mut c_void);
type AudioChunkCallbackC =
    unsafe extern "C" fn(c_int, c_int, i64, *const c_float, usize, bool, *mut c_void);

struct StreamCallbacks {
    progress: ProgressCallback,
    audio: AudioChunkCallback,
}

pub struct Engine {
    _lib: Library,
    handle: *mut c_void,
    destroy: DestroyFn,
    load_model: LoadModelFn,
    unload_model: UnloadModelFn,
    is_loaded: IsLoadedFn,
    is_generating: IsGeneratingFn,
    cancel: CancelFn,
    get_model_info: GetModelInfoFn,
    generate_tts: GenerateTtsFn,
    generate_voice_clone: GenerateVoiceCloneFn,
    generate_finish: GenerateFinishFn,
    generate_tts_stream: Option<GenerateTtsStreamFn>,
    generate_voice_clone_stream: Option<GenerateVoiceCloneStreamFn>,
    generate_finish_stream: Option<GenerateFinishStreamFn>,
    free_result: FreeResultFn,
    last_error: LastErrorFn,
    version: VersionFn,
}

// Указатели трогаем только через &self под внешней сериализацией джоб-очереди — движок сам по себе Send+Sync.
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl Engine {
    /// Загрузить DLL и разрешить символы. handle создаётся через audiocpp_create.
    pub fn load(library_path: &Path) -> Result<Self, EngineError> {
        // На Windows подмешиваем каталог DLL в PATH — иначе зависимые DLL (cuda/ggml) не находятся.
        if let Some(dir) = library_path.parent() {
            add_dll_directory(dir);
        }
        let lib = unsafe {
            Library::new(library_path).map_err(|e| {
                EngineError::LibraryLoad(format!("{}: {}", library_path.display(), e))
            })?
        };

        unsafe {
            let create: Symbol<CreateFn> = lib
                .get(b"audiocpp_create")
                .map_err(|e| EngineError::LibraryLoad(format!("symbol audiocpp_create: {e}")))?;
            let destroy: Symbol<DestroyFn> = lib
                .get(b"audiocpp_destroy")
                .map_err(|e| EngineError::LibraryLoad(format!("symbol audiocpp_destroy: {e}")))?;
            let load_model: Symbol<LoadModelFn> = lib
                .get(b"audiocpp_load_model")
                .map_err(|e| EngineError::LibraryLoad(format!("symbol audiocpp_load_model: {e}")))?;
            let unload_model: Symbol<UnloadModelFn> = lib
                .get(b"audiocpp_unload_model")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_unload_model: {e}"))
                })?;
            let is_loaded: Symbol<IsLoadedFn> = lib
                .get(b"audiocpp_is_model_loaded")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_is_model_loaded: {e}"))
                })?;
            let is_generating: Symbol<IsGeneratingFn> = lib
                .get(b"audiocpp_is_generating")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_is_generating: {e}"))
                })?;
            let cancel: Symbol<CancelFn> = lib
                .get(b"audiocpp_cancel")
                .map_err(|e| EngineError::LibraryLoad(format!("symbol audiocpp_cancel: {e}")))?;
            let get_model_info: Symbol<GetModelInfoFn> = lib
                .get(b"audiocpp_get_model_info")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_get_model_info: {e}"))
                })?;
            let generate_tts: Symbol<GenerateTtsFn> = lib
                .get(b"audiocpp_generate_tts")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_generate_tts: {e}"))
                })?;
            let generate_voice_clone: Symbol<GenerateVoiceCloneFn> = lib
                .get(b"audiocpp_generate_voice_clone")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_generate_voice_clone: {e}"))
                })?;
            let generate_finish: Symbol<GenerateFinishFn> = lib
                .get(b"audiocpp_generate_finish_sentence")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!(
                        "symbol audiocpp_generate_finish_sentence: {e}"
                    ))
                })?;
            // stream-варианты опциональны: старые DLL их не экспортируют.
            let generate_tts_stream_ptr = lib
                .get::<GenerateTtsStreamFn>(b"audiocpp_generate_tts_stream")
                .ok()
                .map(|symbol| *symbol);
            let generate_voice_clone_stream_ptr = lib
                .get::<GenerateVoiceCloneStreamFn>(b"audiocpp_generate_voice_clone_stream")
                .ok()
                .map(|symbol| *symbol);
            let generate_finish_stream_ptr = lib
                .get::<GenerateFinishStreamFn>(b"audiocpp_generate_finish_sentence_stream")
                .ok()
                .map(|symbol| *symbol);
            let free_result: Symbol<FreeResultFn> = lib
                .get(b"audiocpp_free_result")
                .map_err(|e| {
                    EngineError::LibraryLoad(format!("symbol audiocpp_free_result: {e}"))
                })?;
            let last_error: Symbol<LastErrorFn> = lib
                .get(b"audiocpp_last_error")
                .map_err(|e| EngineError::LibraryLoad(format!("symbol audiocpp_last_error: {e}")))?;
            let version: Symbol<VersionFn> = lib
                .get(b"audiocpp_version")
                .map_err(|e| EngineError::LibraryLoad(format!("symbol audiocpp_version: {e}")))?;

            // Разыменовываем символы в сырые указатели ДО move lib в структуру (Symbol заимствует lib).
            let create_ptr = *create;
            let destroy_ptr = *destroy;
            let load_model_ptr = *load_model;
            let unload_model_ptr = *unload_model;
            let is_loaded_ptr = *is_loaded;
            let is_generating_ptr = *is_generating;
            let cancel_ptr = *cancel;
            let get_model_info_ptr = *get_model_info;
            let generate_tts_ptr = *generate_tts;
            let generate_voice_clone_ptr = *generate_voice_clone;
            let generate_finish_ptr = *generate_finish;
            let free_result_ptr = *free_result;
            let last_error_ptr = *last_error;
            let version_ptr = *version;

            let handle = create_ptr();
            if handle.is_null() {
                return Err(EngineError::LibraryLoad(
                    "audiocpp_create returned null".into(),
                ));
            }

            Ok(Engine {
                _lib: lib,
                handle,
                destroy: destroy_ptr,
                load_model: load_model_ptr,
                unload_model: unload_model_ptr,
                is_loaded: is_loaded_ptr,
                is_generating: is_generating_ptr,
                cancel: cancel_ptr,
                get_model_info: get_model_info_ptr,
                generate_tts: generate_tts_ptr,
                generate_voice_clone: generate_voice_clone_ptr,
                generate_finish: generate_finish_ptr,
                generate_tts_stream: generate_tts_stream_ptr,
                generate_voice_clone_stream: generate_voice_clone_stream_ptr,
                generate_finish_stream: generate_finish_stream_ptr,
                free_result: free_result_ptr,
                last_error: last_error_ptr,
                version: version_ptr,
            })
        }
    }

    pub fn version(&self) -> String {
        unsafe {
            let v = (self.version)();
            if v.is_null() {
                "unknown".into()
            } else {
                CStr::from_ptr(v).to_string_lossy().into_owned()
            }
        }
    }

    pub fn is_model_loaded(&self) -> bool {
        unsafe { (self.is_loaded)(self.handle) }
    }

    pub fn is_generating(&self) -> bool {
        unsafe { (self.is_generating)(self.handle) }
    }

    pub fn supports_streaming(&self) -> bool {
        self.generate_tts_stream.is_some()
            && self.generate_voice_clone_stream.is_some()
            && self.generate_finish_stream.is_some()
    }

    pub fn cancel(&self) {
        unsafe { (self.cancel)(self.handle) }
    }

    /// Загрузить веса модели. backend: cpu/cuda/vulkan/metal; weight_type — вариант квантизации (может быть пустым).
    pub fn load_model(&self, req: &LoadModelRequest) -> Result<ModelInfo, EngineError> {
        let model_root = CString::new(req.model_root.as_str())
            .map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let backend = backend_id(&req.backend);
        let weight_type = req.weight_type.as_deref().unwrap_or("");
        let weight_c =
            CString::new(weight_type).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let session_json = req
            .session_options
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .unwrap_or_default();
        let session_c =
            CString::new(session_json).map_err(|e| EngineError::InvalidParam(e.to_string()))?;

        let status = unsafe {
            (self.load_model)(
                self.handle,
                model_root.as_ptr(),
                backend,
                req.device,
                req.threads,
                weight_c.as_ptr(),
                session_c.as_ptr(),
            )
        };
        if status != 0 {
            let err = self.get_last_error();
            return Err(EngineError::Generation(format!(
                "load_model failed (code {status}): {err}"
            )));
        }
        self.get_model_info()
    }

    pub fn unload_model(&self) {
        unsafe { (self.unload_model)(self.handle) }
    }

    fn get_model_info(&self) -> Result<ModelInfo, EngineError> {
        unsafe {
            let mut raw = ModelInfoRaw {
                family: std::ptr::null(),
                display_name: std::ptr::null(),
                weight_type: std::ptr::null(),
                model_root: std::ptr::null(),
            };
            let status = (self.get_model_info)(self.handle, &mut raw);
            if status != 0 {
                return Err(EngineError::Generation("get_model_info failed".into()));
            }
            Ok(ModelInfo {
                family: cstr_to_string(raw.family),
                display_name: cstr_to_string(raw.display_name),
                weight_type: cstr_to_string(raw.weight_type),
                model_root: cstr_to_string(raw.model_root),
            })
        }
    }

    pub fn get_last_error(&self) -> String {
        unsafe {
            let ptr = (self.last_error)(self.handle);
            cstr_to_string(ptr)
        }
    }

    // ─── извлечение результата + освобождение C-памяти ─────────────────────

    fn extract_result(
        &self,
        status: c_int,
        mut raw: AudioResultRaw,
    ) -> Result<AudioResult, EngineError> {
        let error_msg = if raw.error.is_null() {
            None
        } else {
            Some(unsafe { cstr_to_string(raw.error) })
        };

        let result = if status == 0 && !raw.samples.is_null() && raw.sample_count > 0 {
            let samples =
                unsafe { std::slice::from_raw_parts(raw.samples, raw.sample_count).to_vec() };
            Ok(AudioResult {
                sample_rate: raw.sample_rate,
                channels: raw.channels,
                samples,
            })
        } else {
            Err(match status {
                5 => EngineError::Cancelled,
                _ => EngineError::Generation(
                    error_msg.unwrap_or_else(|| format!("generation failed (code {status})")),
                ),
            })
        };

        // Всегда освобождаем C-аллокации (samples/error), даже на ошибке.
        unsafe { (self.free_result)(&mut raw) };
        result
    }

    // ─── трамплины прогресса/чанков ────────────────────────────────────────

    extern "C" fn progress_trampoline(
        current: c_int,
        total: c_int,
        phase: *const c_char,
        user: *mut c_void,
    ) {
        if user.is_null() {
            return;
        }
        let cb = unsafe { &*(user as *const ProgressCallback) };
        let phase_str = unsafe { cstr_to_string(phase) };
        cb(current, total, &phase_str);
    }

    extern "C" fn stream_progress_trampoline(
        current: c_int,
        total: c_int,
        phase: *const c_char,
        user: *mut c_void,
    ) {
        if user.is_null() {
            return;
        }
        let cb = unsafe { &*(user as *const StreamCallbacks) };
        let phase_str = unsafe { cstr_to_string(phase) };
        (cb.progress)(current, total, &phase_str);
    }

    extern "C" fn audio_chunk_trampoline(
        sample_rate: c_int,
        channels: c_int,
        start_sample: i64,
        samples: *const c_float,
        sample_count: usize,
        is_final: bool,
        user: *mut c_void,
    ) {
        if user.is_null() || samples.is_null() || sample_count == 0 {
            return;
        }
        let cb = unsafe { &*(user as *const StreamCallbacks) };
        let slice = unsafe { std::slice::from_raw_parts(samples, sample_count) };
        (cb.audio)(sample_rate, channels, start_sample, slice, is_final);
    }

    fn make_progress_box(progress: ProgressCallback) -> *mut c_void {
        Box::into_raw(Box::new(progress)) as *mut c_void
    }

    unsafe fn reclaim_progress_box(ptr: *mut c_void) {
        if !ptr.is_null() {
            drop(Box::from_raw(ptr as *mut ProgressCallback));
        }
    }

    fn make_stream_box(progress: ProgressCallback, audio: AudioChunkCallback) -> *mut c_void {
        Box::into_raw(Box::new(StreamCallbacks { progress, audio })) as *mut c_void
    }

    unsafe fn reclaim_stream_box(ptr: *mut c_void) {
        if !ptr.is_null() {
            drop(Box::from_raw(ptr as *mut StreamCallbacks));
        }
    }

    // ─── generate_tts ─────────────────────────────────────────────────────

    pub fn generate_tts(
        &self,
        text: &str,
        options: &serde_json::Value,
        progress: ProgressCallback,
    ) -> Result<AudioResult, EngineError> {
        let text_c = CString::new(text).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let opts_c = opts_cstring(options)?;
        let cb_ptr = Self::make_progress_box(progress);
        let mut raw = AudioResultRaw::empty();
        let status = unsafe {
            (self.generate_tts)(
                self.handle,
                text_c.as_ptr(),
                opts_c.as_ptr(),
                Self::progress_trampoline,
                cb_ptr,
                &mut raw,
            )
        };
        unsafe { Self::reclaim_progress_box(cb_ptr) };
        self.extract_result(status, raw)
    }

    pub fn generate_tts_stream(
        &self,
        text: &str,
        options: &serde_json::Value,
        progress: ProgressCallback,
        audio: AudioChunkCallback,
    ) -> Result<AudioResult, EngineError> {
        let Some(generate_stream) = self.generate_tts_stream else {
            return Err(EngineError::StreamingUnsupported);
        };
        let text_c = CString::new(text).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let opts_c = opts_cstring(options)?;
        let cb_ptr = Self::make_stream_box(progress, audio);
        let mut raw = AudioResultRaw::empty();
        let status = unsafe {
            generate_stream(
                self.handle,
                text_c.as_ptr(),
                opts_c.as_ptr(),
                Self::stream_progress_trampoline,
                Self::audio_chunk_trampoline,
                cb_ptr,
                &mut raw,
            )
        };
        unsafe { Self::reclaim_stream_box(cb_ptr) };
        self.extract_result(status, raw)
    }

    // ─── generate_voice_clone ─────────────────────────────────────────────

    pub fn generate_voice_clone(
        &self,
        text: &str,
        ref_audio_path: &str,
        ref_text: Option<&str>,
        options: &serde_json::Value,
        progress: ProgressCallback,
    ) -> Result<AudioResult, EngineError> {
        let text_c = CString::new(text).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let ref_path_c =
            CString::new(ref_audio_path).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let ref_text_c = CString::new(ref_text.unwrap_or(""))
            .map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let opts_c = opts_cstring(options)?;
        let cb_ptr = Self::make_progress_box(progress);
        let mut raw = AudioResultRaw::empty();
        let status = unsafe {
            (self.generate_voice_clone)(
                self.handle,
                text_c.as_ptr(),
                ref_path_c.as_ptr(),
                ref_text_c.as_ptr(),
                opts_c.as_ptr(),
                Self::progress_trampoline,
                cb_ptr,
                &mut raw,
            )
        };
        unsafe { Self::reclaim_progress_box(cb_ptr) };
        self.extract_result(status, raw)
    }

    pub fn generate_voice_clone_stream(
        &self,
        text: &str,
        ref_audio_path: &str,
        ref_text: Option<&str>,
        options: &serde_json::Value,
        progress: ProgressCallback,
        audio: AudioChunkCallback,
    ) -> Result<AudioResult, EngineError> {
        let Some(generate_stream) = self.generate_voice_clone_stream else {
            return Err(EngineError::StreamingUnsupported);
        };
        let text_c = CString::new(text).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let ref_path_c =
            CString::new(ref_audio_path).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let ref_text_c = CString::new(ref_text.unwrap_or(""))
            .map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let opts_c = opts_cstring(options)?;
        let cb_ptr = Self::make_stream_box(progress, audio);
        let mut raw = AudioResultRaw::empty();
        let status = unsafe {
            generate_stream(
                self.handle,
                text_c.as_ptr(),
                ref_path_c.as_ptr(),
                ref_text_c.as_ptr(),
                opts_c.as_ptr(),
                Self::stream_progress_trampoline,
                Self::audio_chunk_trampoline,
                cb_ptr,
                &mut raw,
            )
        };
        unsafe { Self::reclaim_stream_box(cb_ptr) };
        self.extract_result(status, raw)
    }

    // ─── generate_finish_sentence ─────────────────────────────────────────

    pub fn generate_finish_sentence(
        &self,
        audio_path: &str,
        continuation_text: Option<&str>,
        options: &serde_json::Value,
        progress: ProgressCallback,
    ) -> Result<AudioResult, EngineError> {
        let audio_c =
            CString::new(audio_path).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let text_c = CString::new(continuation_text.unwrap_or(""))
            .map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let opts_c = opts_cstring(options)?;
        let cb_ptr = Self::make_progress_box(progress);
        let mut raw = AudioResultRaw::empty();
        let status = unsafe {
            (self.generate_finish)(
                self.handle,
                audio_c.as_ptr(),
                text_c.as_ptr(),
                opts_c.as_ptr(),
                Self::progress_trampoline,
                cb_ptr,
                &mut raw,
            )
        };
        unsafe { Self::reclaim_progress_box(cb_ptr) };
        self.extract_result(status, raw)
    }

    pub fn generate_finish_sentence_stream(
        &self,
        audio_path: &str,
        continuation_text: Option<&str>,
        options: &serde_json::Value,
        progress: ProgressCallback,
        audio: AudioChunkCallback,
    ) -> Result<AudioResult, EngineError> {
        let Some(generate_stream) = self.generate_finish_stream else {
            return Err(EngineError::StreamingUnsupported);
        };
        let audio_c =
            CString::new(audio_path).map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let text_c = CString::new(continuation_text.unwrap_or(""))
            .map_err(|e| EngineError::InvalidParam(e.to_string()))?;
        let opts_c = opts_cstring(options)?;
        let cb_ptr = Self::make_stream_box(progress, audio);
        let mut raw = AudioResultRaw::empty();
        let status = unsafe {
            generate_stream(
                self.handle,
                audio_c.as_ptr(),
                text_c.as_ptr(),
                opts_c.as_ptr(),
                Self::stream_progress_trampoline,
                Self::audio_chunk_trampoline,
                cb_ptr,
                &mut raw,
            )
        };
        unsafe { Self::reclaim_stream_box(cb_ptr) };
        self.extract_result(status, raw)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

// ─── Windows: путь поиска DLL ───────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn add_dll_directory(path: &Path) {
    // Префикс к PATH надёжнее SetDefaultDllDirectories: не ограничивает поиск только AddDllDirectory-каталогами.
    let dir = path.to_string_lossy().into_owned();
    let current = std::env::var("PATH").unwrap_or_default();
    let new_path = if current.is_empty() {
        dir
    } else {
        format!("{};{}", dir, current)
    };
    std::env::set_var("PATH", new_path);
}

#[cfg(not(target_os = "windows"))]
pub fn add_dll_directory(_path: &Path) {}
