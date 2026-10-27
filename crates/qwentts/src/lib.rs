//! Qwen3-TTS 1.7B через qwentts.cpp (ServeurpersoCom; наш форк timoncool/qwentts.cpp, engine-0.2.x), загружаемый во
//! время работы: его C ABI `qwen.h` (ABI 7) — девять голосов модели (CustomVoice), клон по эталону (Base), адаптер LoRA
//! русских ударений на Base, потоковые куски и отмена. Устройство (`CUDA0`, `Vulkan0`, `CPU`, `MTL0`) — его модуль
//! бэкенда ggml, который библиотека грузит рядом с собой; та же раскладка, что у Speech Studio (`engines/qwen_tts.rs`).

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use libloading::Library;
use thiserror::Error;

/// Папка библиотеки и весов в каталоге моделей; те же пути, что у Speech Studio, чтобы файлы брались у соседа.
pub const DIR: &str = "qwen3-tts";
pub const CODEC: &str = "qwen-tokenizer-12hz-Q8_0.gguf";
/// Кванты талкеров, первый — по умолчанию: CustomVoice говорит голосами модели, Base клонирует эталон.
pub const QUANTS: &[(&str, &str, &str)] = &[
    ("Q4_K_M", "qwen-talker-1.7b-customvoice-Q4_K_M.gguf", "qwen-talker-1.7b-base-Q4_K_M.gguf"),
    ("Q8_0", "qwen-talker-1.7b-customvoice-Q8_0.gguf", "qwen-talker-1.7b-base-Q8_0.gguf"),
];
/// siriusfreak/qwen3-tts-12hz-1.7b-ru-stress-cf `lora/adapter.safetensors`: r 16, alpha 32 на талкере Base, читает
/// ударение знаком U+0301 после гласной. Имя файла то же, что у Speech Studio.
pub const STRESS_LORA: &str = "qwen-talker-1.7b-base-ru-stress-lora.safetensors";
const STRESS_LORA_SCALE: f32 = 32.0 / 16.0;
#[cfg(windows)]
pub const LIBRARY: &str = "qwen.dll";
#[cfg(target_os = "macos")]
pub const LIBRARY: &str = "libqwen.dylib";
#[cfg(all(not(windows), not(target_os = "macos")))]
pub const LIBRARY: &str = "libqwen.so";
/// Голоса CustomVoice: id, имя, пол, родной язык по карточке модели.
pub const SPEAKERS: &[(&str, &str, &str, &str)] = &[
    ("vivian", "Vivian", "female", "zh"),
    ("serena", "Serena", "female", "zh"),
    ("uncle_fu", "Uncle Fu", "male", "zh"),
    ("dylan", "Dylan", "male", "zh"),
    ("eric", "Eric", "male", "zh"),
    ("ryan", "Ryan", "male", "en"),
    ("aiden", "Aiden", "male", "en"),
    ("ono_anna", "Ono Anna", "female", "ja"),
    ("sohee", "Sohee", "female", "ko"),
];
pub const SAMPLE_RATE: u32 = 24_000;
const ABI_VERSION: i32 = 7;
const STATUS_CANCELLED: i32 = -5;

#[derive(Debug, Error)]
pub enum QwenError {
    #[error("qwen3-tts: {0}")]
    Load(String),
    #[error("qwen3-tts: {0}")]
    Engine(String),
    #[error("qwen3-tts: generation cancelled")]
    Cancelled,
}

#[repr(C)]
struct InitParams {
    abi_version: i32,
    talker_path: *const c_char,
    codec_path: *const c_char,
    use_fa: bool,
    clamp_fp16: bool,
    max_batch: i32,
    codec_chunk_sec: f32,
    device: *const c_char,
    lora_path: *const c_char,
    lora_scale: f32,
}

type ChunkFn = unsafe extern "C" fn(*const f32, i32, *mut c_void) -> bool;
type CancelFn = unsafe extern "C" fn(*mut c_void) -> bool;

#[repr(C)]
struct TtsParams {
    abi_version: i32,
    text: *const c_char,
    lang: *const c_char,
    instruct: *const c_char,
    speaker: *const c_char,
    ref_audio_24k: *const f32,
    ref_n_samples: i32,
    ref_text: *const c_char,
    seed: i64,
    max_new_tokens: i32,
    temperature: f32,
    top_k: i32,
    top_p: f32,
    repetition_penalty: f32,
    subtalker_temperature: f32,
    subtalker_top_k: i32,
    subtalker_top_p: f32,
    dump_dir: *const c_char,
    cancel: Option<CancelFn>,
    cancel_user_data: *mut c_void,
    on_chunk: Option<ChunkFn>,
    on_chunk_user_data: *mut c_void,
    ref_spk_emb: *const f32,
    ref_spk_dim: i32,
    ref_codes: *const i32,
    ref_t: i32,
}

#[repr(C)]
struct Audio {
    samples: *mut f32,
    n_samples: i32,
    sample_rate: i32,
    channels: i32,
}

/// Условие клона, которое библиотека один раз извлекает из эталона: эмбеддинг говорящего и коды кодека.
#[repr(C)]
struct VoiceRef {
    ref_spk_emb: *mut f32,
    ref_spk_dim: i32,
    ref_codes: *mut i32,
    ref_t: i32,
    num_codebooks: i32,
}

#[repr(C)]
struct RawContext {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
struct Api {
    version: unsafe extern "C" fn() -> *const c_char,
    last_error: unsafe extern "C" fn() -> *const c_char,
    init_default: unsafe extern "C" fn(*mut InitParams),
    init: unsafe extern "C" fn(*const InitParams) -> *mut RawContext,
    free: unsafe extern "C" fn(*mut RawContext),
    tts_default: unsafe extern "C" fn(*mut TtsParams),
    synthesize: unsafe extern "C" fn(*mut RawContext, *const TtsParams, *mut Audio) -> i32,
    audio_free: unsafe extern "C" fn(*mut Audio),
    model_type: unsafe extern "C" fn(*const RawContext) -> *const c_char,
    extract_voice_ref: unsafe extern "C" fn(*mut RawContext, *const f32, i32, *mut VoiceRef) -> i32,
    voice_ref_free: unsafe extern "C" fn(*mut VoiceRef),
}

/// Библиотека загружается один раз на процесс и не выгружается: статический инициализатор ggml второй загрузки
/// не переживает (GGML_ASSERT на своём обработчике terminate), выгрузка и новая загрузка роняют процесс. Модули
/// бэкендов она грузит рядом с собой, когда устройство нужно впервые: одна библиотека на все устройства, смена
/// устройства перезагружает только модель.
static LOADED: std::sync::Mutex<Option<(Library, Api)>> = std::sync::Mutex::new(None);

fn library(dir: &Path) -> Result<Api, QwenError> {
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, api)) = loaded.as_ref() {
        return Ok(*api);
    }
    let lib = unsafe { Library::new(dir.join(LIBRARY)) }.map_err(|e| QwenError::Load(format!("{}: {e}", dir.join(LIBRARY).display())))?;
    let api = unsafe {
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| QwenError::Load(format!("{}: {e}", $name)))?
            };
        }
        Api {
            version: sym!("qt_version"),
            last_error: sym!("qt_last_error"),
            init_default: sym!("qt_init_default_params"),
            init: sym!("qt_init"),
            free: sym!("qt_free"),
            tts_default: sym!("qt_tts_default_params"),
            synthesize: sym!("qt_synthesize"),
            audio_free: sym!("qt_audio_free"),
            model_type: sym!("qt_model_type"),
            extract_voice_ref: sym!("qt_extract_voice_ref"),
            voice_ref_free: sym!("qt_voice_ref_free"),
        }
    };
    *loaded = Some((lib, api));
    Ok(api)
}

pub fn dir(models_root: &Path) -> PathBuf {
    models_root.join(DIR)
}

pub fn talker(models_root: &Path, quant: &str, clone: bool) -> Option<PathBuf> {
    QUANTS.iter().find(|(q, _, _)| *q == quant).map(|(_, custom, base)| dir(models_root).join(if clone { base } else { custom }))
}

/// Есть ли библиотека, кодек и талкер кванта: Base для клона, CustomVoice для голосов модели.
pub fn installed(models_root: &Path, quant: &str, clone: bool) -> bool {
    let d = dir(models_root);
    d.join(LIBRARY).is_file() && d.join(CODEC).is_file() && talker(models_root, quant, clone).is_some_and(|t| t.is_file())
}

/// Есть ли адаптер русских ударений.
pub fn stress_installed(models_root: &Path) -> bool {
    dir(models_root).join(STRESS_LORA).is_file()
}

/// Ударение «+» перед гласной: с адаптером становится знаком U+0301 после неё, без адаптера снимается.
pub fn stress_marks(text: &str, acute: bool) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(&v) if c == '+' && is_vowel(v) => {
                chars.next();
                out.push(v);
                if acute {
                    out.push('\u{301}');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_vowel(c: char) -> bool {
    "аеёиоуыэюяАЕЁИОУЫЭЮЯ".contains(c)
}

/// Имя языка, которое читает ABI; `None` — модель определит сама.
fn lang_name(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_lowercase().as_str() {
        "ru" | "russian" => "russian",
        "en" | "english" => "english",
        "zh" | "chinese" => "chinese",
        "ja" | "japanese" => "japanese",
        "ko" | "korean" => "korean",
        "de" | "german" => "german",
        "fr" | "french" => "french",
        "es" | "spanish" => "spanish",
        "pt" | "portuguese" => "portuguese",
        "it" | "italian" => "italian",
        _ => return None,
    })
}

/// Что и чем сказать одну фразу: голосом модели (талкер CustomVoice) или по эталону (Base) — моно 24 кГц и его
/// расшифровка.
pub struct Say<'a> {
    pub text: &'a str,
    pub lang: &'a str,
    pub speaker: Option<&'a str>,
    pub reference: Option<(&'a [f32], Option<&'a str>)>,
    pub seed: u64,
    pub temperature: f32,
    pub max_new_tokens: i32,
}

fn cstring(s: &str, what: &str) -> Result<CString, QwenError> {
    CString::new(s).map_err(|_| QwenError::Engine(format!("{what} contains NUL")))
}

unsafe extern "C" fn chunk_trampoline(samples: *const f32, n: i32, user: *mut c_void) -> bool {
    if !samples.is_null() && n > 0 && !user.is_null() {
        let sink = &*(user as *const &(dyn Fn(&[f32]) + Sync));
        sink(std::slice::from_raw_parts(samples, n as usize));
    }
    true
}

unsafe extern "C" fn cancel_trampoline(user: *mut c_void) -> bool {
    !user.is_null() && (*(user as *const AtomicBool)).load(Ordering::Relaxed)
}

unsafe fn last_error(api: &Api) -> String {
    let p = (api.last_error)();
    if p.is_null() { "unknown error".into() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

/// Талкер и кодек на видеокарте; освобождаются при drop. Один синтез за раз — забота вызывающего.
pub struct QwenEngine {
    api: Api,
    raw: *mut RawContext,
    stop: AtomicBool,
    /// Условие последнего эталона по хэшу его звука: фразы одного говорящего и повторы берут его готовым.
    voice: std::sync::Mutex<Option<([u8; 32], VoiceRef)>>,
}

// qt_synthesize потокобезопасен, а хэндл освобождается только в drop.
unsafe impl Send for QwenEngine {}
unsafe impl Sync for QwenEngine {}

impl QwenEngine {
    /// Загружает библиотеку (CUDA-библиотеки она находит по PATH) и талкер кванта на устройстве ggml `device` (`CUDA0`,
    /// `Vulkan0`, `CPU`, `MTL0`): Base при `clone`, иначе CustomVoice; `stress` добавляет к Base адаптер ударений.
    pub fn load(models_root: &Path, device: &str, quant: &str, clone: bool, stress: bool) -> Result<QwenEngine, QwenError> {
        if stress && !clone {
            return Err(QwenError::Load("the stress adapter is trained on the Base talker".into()));
        }
        let d = dir(models_root);
        // её библиотеки ggml лежат рядом под именами, которые есть и у других движков: эта папка в PATH первой
        let path = std::env::var("PATH").unwrap_or_default();
        if !std::env::split_paths(&path).any(|p| p == d) {
            let joined = std::env::join_paths(std::iter::once(d.clone()).chain(std::env::split_paths(&path))).map_err(|e| QwenError::Load(e.to_string()))?;
            std::env::set_var("PATH", joined);
        }
        let api = library(&d)?;
        let talker = talker(models_root, quant, clone).ok_or_else(|| QwenError::Load(format!("no quantisation {quant}")))?;
        let (t, c) = (cstring(&talker.to_string_lossy(), "talker path")?, cstring(&d.join(CODEC).to_string_lossy(), "codec path")?);
        let lora = if stress { Some(cstring(&d.join(STRESS_LORA).to_string_lossy(), "adapter path")?) } else { None };
        let device = cstring(device, "device")?;
        let mut p: InitParams = unsafe { std::mem::zeroed() };
        unsafe { (api.init_default)(&mut p) };
        p.abi_version = ABI_VERSION;
        p.talker_path = t.as_ptr();
        p.codec_path = c.as_ptr();
        p.device = device.as_ptr();
        if let Some(lora) = &lora {
            p.lora_path = lora.as_ptr();
            p.lora_scale = STRESS_LORA_SCALE;
        }
        let raw = unsafe { (api.init)(&p) };
        if raw.is_null() {
            return Err(QwenError::Load(unsafe { last_error(&api) }));
        }
        let engine = QwenEngine { api, raw, stop: AtomicBool::new(false), voice: std::sync::Mutex::new(None) };
        let kind = unsafe { CStr::from_ptr((engine.api.model_type)(engine.raw)) }.to_string_lossy().into_owned();
        let expected = if clone { "base" } else { "custom_voice" };
        if kind != expected {
            return Err(QwenError::Load(format!("{} is a {kind} checkpoint, not {expected}", talker.display())));
        }
        Ok(engine)
    }

    /// The library's version (`qt_version`).
    pub fn version(&self) -> String {
        unsafe { CStr::from_ptr((self.api.version)()) }.to_string_lossy().into_owned()
    }

    /// Останавливает идущий синтез за кадр (~83 мс).
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Одна фраза целиком (библиотека отдаёт её кусками в колбэк, здесь они собираются).
    pub fn speak(&self, say: &Say) -> Result<Vec<f32>, QwenError> {
        if let Some(s) = say.speaker.filter(|s| !SPEAKERS.iter().any(|k| k.0 == *s)) {
            return Err(QwenError::Engine(format!("no speaker {s}")));
        }
        let text = cstring(say.text, "text")?;
        let speaker = say.speaker.map(|s| cstring(s, "speaker")).transpose()?;
        let ref_text = say.reference.and_then(|(_, t)| t).filter(|t| !t.trim().is_empty()).map(|t| cstring(t, "reference text")).transpose()?;
        let lang = lang_name(say.lang).map(|l| cstring(l, "lang")).transpose()?;
        let mut p: TtsParams = unsafe { std::mem::zeroed() };
        unsafe { (self.api.tts_default)(&mut p) };
        p.abi_version = ABI_VERSION;
        p.text = text.as_ptr();
        p.speaker = speaker.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
        let mut voice = self.voice.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((audio, _)) = say.reference {
            let key: [u8; 32] = *blake3::hash(unsafe { std::slice::from_raw_parts(audio.as_ptr() as *const u8, std::mem::size_of_val(audio)) }).as_bytes();
            if voice.as_ref().is_none_or(|(k, _)| *k != key) {
                if let Some((_, mut old)) = voice.take() {
                    unsafe { (self.api.voice_ref_free)(&mut old) };
                }
                let mut r: VoiceRef = unsafe { std::mem::zeroed() };
                if unsafe { (self.api.extract_voice_ref)(self.raw, audio.as_ptr(), audio.len() as i32, &mut r) } != 0 {
                    return Err(QwenError::Engine(unsafe { last_error(&self.api) }));
                }
                *voice = Some((key, r));
            }
            let r = &voice.as_ref().expect("set above").1;
            p.ref_spk_emb = r.ref_spk_emb;
            p.ref_spk_dim = r.ref_spk_dim;
            if let Some(t) = &ref_text {
                p.ref_codes = r.ref_codes;
                p.ref_t = r.ref_t;
                p.ref_text = t.as_ptr();
            }
        }
        p.lang = lang.as_ref().map_or(std::ptr::null(), |l| l.as_ptr());
        p.seed = (say.seed & i64::MAX as u64) as i64;
        p.temperature = say.temperature;
        if say.max_new_tokens > 0 {
            p.max_new_tokens = say.max_new_tokens;
        }
        self.stop.store(false, Ordering::Relaxed);
        p.cancel = Some(cancel_trampoline);
        p.cancel_user_data = &self.stop as *const AtomicBool as *mut c_void;
        let gathered = std::sync::Mutex::new(Vec::<f32>::new());
        let sink = |a: &[f32]| gathered.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(a);
        let sink_ref: &(dyn Fn(&[f32]) + Sync) = &sink;
        p.on_chunk = Some(chunk_trampoline);
        p.on_chunk_user_data = &sink_ref as *const &(dyn Fn(&[f32]) + Sync) as *mut c_void;
        let mut out = Audio { samples: std::ptr::null_mut(), n_samples: 0, sample_rate: 0, channels: 0 };
        let status = unsafe { (self.api.synthesize)(self.raw, &p, &mut out) };
        unsafe { (self.api.audio_free)(&mut out) };
        self.stop.store(false, Ordering::Relaxed);
        match status {
            0 => Ok(gathered.into_inner().unwrap_or_else(|e| e.into_inner())),
            STATUS_CANCELLED => Err(QwenError::Cancelled),
            _ => Err(QwenError::Engine(unsafe { last_error(&self.api) })),
        }
    }
}

impl Drop for QwenEngine {
    fn drop(&mut self) {
        if let Some((_, mut r)) = self.voice.get_mut().unwrap_or_else(|e| e.into_inner()).take() {
            unsafe { (self.api.voice_ref_free)(&mut r) };
        }
        unsafe { (self.api.free)(self.raw) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_map_to_the_names_the_library_reads() {
        assert_eq!(lang_name("ru"), Some("russian"));
        assert_eq!(lang_name("Russian"), Some("russian"));
        assert_eq!(lang_name("auto"), None);
    }

    #[test]
    fn each_quant_names_both_talkers_and_the_folder_is_shared_with_speech_studio() {
        let root = Path::new("models");
        assert!(talker(root, "Q4_K_M", true).unwrap().ends_with("qwen3-tts/qwen-talker-1.7b-base-Q4_K_M.gguf"));
        assert!(talker(root, "Q8_0", false).unwrap().ends_with("qwen3-tts/qwen-talker-1.7b-customvoice-Q8_0.gguf"));
        assert!(talker(root, "Q2_K", false).is_none());
    }

    #[test]
    fn a_plus_before_a_vowel_becomes_an_acute_after_it_or_goes() {
        assert_eq!(stress_marks("Он з+амок запер на зам+ок, 2+2", true), "Он за\u{301}мок запер на замо\u{301}к, 2+2");
        assert_eq!(stress_marks("Он з+амок запер на зам+ок, 2+2", false), "Он замок запер на замок, 2+2");
        assert_eq!(stress_marks("+Атлас", true), "А\u{301}тлас");
    }
}
