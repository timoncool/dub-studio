//! Распознавание речи через transcribe.cpp (ggml: CUDA, Vulkan, CPU) для всех программ: Dub Studio, Speech Studio, музыкальных
//! студий. Только загрузка движка во время работы (`libloading`), без ONNX Runtime: программа запускается без движка,
//! движок и модели скачиваются компонентами.

use libloading::Library;
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(windows)]
pub const LIBRARY: &str = "transcribe_engine.dll";
#[cfg(target_os = "macos")]
pub const LIBRARY: &str = "libtranscribe_engine.dylib";
#[cfg(not(any(windows, target_os = "macos")))]
pub const LIBRARY: &str = "libtranscribe_engine.so";

/// Файл закачки: адрес по закреплённой ревизии, размер и SHA-256. Одни и те же у всех программ, поэтому соседи берут
/// файлы друг у друга.
#[derive(Debug)]
pub struct Download {
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// Сборка движка: CUDA (NVIDIA), Vulkan с CPU (AMD, Intel), CPU (без годной видеокарты). Какую ставить и грузить,
/// программа решает по своему железу; у каждой своя папка в каталоге моделей, и другая сборка за неё не подставляется.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    Cuda,
    /// CUDA 12.9 for the cards and drivers CUDA 13 does not run; its cudart and cuBLAS come from the program's CUDA 12.9
    /// runtime (on Windows its folder on PATH, on Linux the library's RUNPATH `$ORIGIN/../cuda12-runtime`).
    Cuda12,
    Vulkan,
    Cpu,
}

/// Whether this computer's processor runs an engine built with the x86-64 floor of the studios: AVX2, FMA, F16C and BMI2
/// (every Intel since Haswell, every AMD since Zen). A processor without them gets this answer, naming `engine`, instead of
/// a crash on the engine's first call. Other architectures pass.
pub fn cpu_floor(engine: &str) -> Result<(), String> {
    let missing = cpu_floor_missing();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{engine} needs a processor with AVX2, FMA, F16C and BMI2 (Intel since 2013, AMD since 2017), and this one has not got {}",
        missing.join(", ")
    ))
}

/// What this computer's processor lacks of the x86-64 floor of [`cpu_floor`]: nothing on one that has it and on other
/// architectures.
pub fn cpu_floor_missing() -> Vec<&'static str> {
    #[cfg(target_arch = "x86_64")]
    {
        [
            ("AVX2", std::arch::is_x86_feature_detected!("avx2")),
            ("FMA", std::arch::is_x86_feature_detected!("fma")),
            ("F16C", std::arch::is_x86_feature_detected!("f16c")),
            ("BMI2", std::arch::is_x86_feature_detected!("bmi2")),
        ]
        .into_iter()
        .filter(|(_, has)| !has)
        .map(|(name, _)| name)
        .collect()
    }
    #[cfg(not(target_arch = "x86_64"))]
    Vec::new()
}

impl Build {
    pub const ALL: [Build; 4] = [Build::Cuda, Build::Cuda12, Build::Vulkan, Build::Cpu];

    /// Папка сборки в каталоге моделей (= id компонента).
    pub fn dir(self) -> &'static str {
        match self {
            Build::Cuda => "transcribe-engine",
            Build::Cuda12 => "transcribe-engine-cuda12",
            Build::Vulkan => "transcribe-engine-vulkan",
            Build::Cpu => "transcribe-engine-cpu",
        }
    }


    /// Whether this computer's processor runs the engine (see [`cpu_floor`]).
    pub fn runs_here(self) -> Result<(), String> {
        let _ = self;
        cpu_floor("transcribe.cpp")
    }

    #[cfg(windows)]
    pub const fn download(self) -> Download {
        match self {
            Build::Cuda => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/transcribe_engine.dll",
                size: 144_430_592,
                sha256: "b4d4cfef4a3a5802cd2426b539de86758a7f92c0acbeb4ea5ade917bff7ac380",
            },
            Build::Cuda12 => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/transcribe_engine-cuda12.dll",
                size: 172_778_496,
                sha256: "16657d6a49b88cf2ced1585dee1359f3713bc12b0af62d108d69474480fb8ad4",
            },
            Build::Vulkan => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/transcribe_engine-vulkan.dll",
                size: 48_807_936,
                sha256: "5b0d2d194ebb838d18f4145f418a141321748c89a41f6ba8692e200597ca5bf8",
            },
            Build::Cpu => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/transcribe_engine-cpu.dll",
                size: 3_560_960,
                sha256: "c39fa05fd6762fa8a29199a8d90765cab5c7afdfec1bd5dcdfd7bcf1cfc6af9a",
            },
        }
    }

    /// На Mac одна сборка на архитектуру, в папке процессорной: Metal и процессор на Apple silicon, процессор на
    /// x86_64. CUDA и Vulkan на Mac нет, их место занимает она же.
    #[cfg(target_os = "macos")]
    pub const fn download(self) -> Download {
        let _ = self;
        #[cfg(target_arch = "aarch64")]
        return Download {
            url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/libtranscribe_engine-macos-arm64.dylib",
            size: 9_012_648,
            sha256: "2a5204cd63156a58ac476610acdb0333bddf929dc9b941da422e8ba703a2e17c",
        };
        #[cfg(not(target_arch = "aarch64"))]
        return Download {
            url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/libtranscribe_engine-macos-x86_64.dylib",
            size: 9_933_088,
            sha256: "f6e73be83962f4bf9bb20ec54704f1c871119b61b114fdffb1439b0262c29bb8",
        };
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub const fn download(self) -> Download {
        match self {
            Build::Cuda => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/libtranscribe_engine.so",
                size: 160_764_040,
                sha256: "55f703005d320afe9ac4eee52b50599ef6eeb002975b2de6ea8d711fcae41283",
            },
            Build::Cuda12 => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/libtranscribe_engine-cuda12.so",
                size: 184_375_104,
                sha256: "6b17f67201f4cc9d40ed219c62783efe0c2c08a9dc17c27896e909300b82a4be",
            },
            Build::Vulkan => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/libtranscribe_engine-vulkan.so",
                size: 50_807_768,
                sha256: "a4d1951f881edea2b477abe44977587db0884f7a295647196b3743d20e752560",
            },
            Build::Cpu => Download {
                url: "https://github.com/timoncool/transcribe.cpp/releases/download/engine-0.1.4/libtranscribe_engine-cpu.so",
                size: 4_838_616,
                sha256: "566328f75598ca35b8aebbbb45e730cbccabe81abd185833186deb3d836db675",
            },
        }
    }
}

/// Модель распознавания на transcribe.cpp: её id, имя, папка (= id компонента) и файл в каталоге моделей, закачка и
/// языки из карточки (пусто — любой).
#[derive(Debug)]
pub struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub dir: &'static str,
    /// Компонент программы, что скачивает модель: один у всех программ, поэтому соседи узнают файлы друг друга.
    pub component: &'static str,
    pub file: &'static str,
    pub download: Download,
    pub languages: &'static [&'static str],
}

pub const MODELS: &[Model] = &[
    Model {
        id: "parakeet-v3",
        name: "Parakeet TDT v3",
        dir: "asr-gguf-parakeet-v3",
        component: "parakeet",
        file: "parakeet-tdt-0.6b-v3-Q8_0.gguf",
        download: Download {
            url: "https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v3-gguf/resolve/d8d8c32cb2017c57e5307767343e97870f8a7f53/parakeet-tdt-0.6b-v3-Q8_0.gguf",
            size: 739_508_576,
            sha256: "5859f77944efcd8eafa23a6350731960b2b55b2203df51f319665c807d802cc7",
        },
        languages: &["bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt", "pl", "pt", "ro", "ru", "sk", "sl", "es", "sv", "uk"],
    },
    Model {
        id: "parakeet-ultra",
        name: "Parakeet Ultra",
        dir: "asr-gguf-parakeet-ultra",
        component: "parakeet-ultra",
        file: "parakeet-ultra-Q8_0.gguf",
        download: Download {
            url: "https://huggingface.co/handy-computer/parakeet-ultra-gguf/resolve/a1539be1b849ea7463b88343e5277b373d0c4ea9/parakeet-ultra-Q8_0.gguf",
            size: 740_363_168,
            sha256: "007a59761e9258779f189df396b50d12b83bb5b79e66df4b955c230d2f2a0a59",
        },
        languages: &["en", "de", "fr", "es", "it", "pt", "ru", "uk", "hr", "sl", "lv", "lt", "et", "fi", "sv", "da", "nl", "pl", "cs", "sk", "hu", "ro", "bg", "el", "mt"],
    },
    Model {
        id: "nemotron",
        name: "Nemotron 3.5",
        dir: "asr-gguf-nemotron",
        component: "asr-gguf-nemotron",
        file: "nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf",
        download: Download {
            url: "https://huggingface.co/handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf/resolve/53ce288f69326b415aeef030cfcb02a08a8096dd/nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf",
            size: 751_094_240,
            sha256: "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c",
        },
        languages: &["en", "es", "fr", "it", "pt", "nl", "de", "tr", "ru", "ar", "hi", "ja", "ko", "vi", "uk", "pl", "sv", "cs", "nb", "da", "bg", "fi", "hr", "sk", "zh", "hu", "ro", "et"],
    },
];

pub fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}

/// Модель, что называют настройки программы: `asr_engine` nemotron, иначе Parakeet в своём варианте (`asr`: `ultra` и
/// `ultra-int8` — Ultra, остальное — v3; слот хранит имена прежних ONNX-сборок, выбор, сделанный тогда, остаётся).
/// None — Whisper или облако.
pub fn chosen(engine: &str, variant: Option<&str>) -> Option<&'static Model> {
    match engine {
        "nemotron" => model("nemotron"),
        "" | "parakeet" => match variant {
            Some("ultra") | Some("ultra-int8") => model("parakeet-ultra"),
            _ => model("parakeet-v3"),
        },
        _ => None,
    }
}

pub fn engine_dir(models_root: &Path, build: Build) -> PathBuf {
    models_root.join(build.dir())
}

pub fn model_path(models_root: &Path, m: &Model) -> PathBuf {
    models_root.join(m.dir).join(m.file)
}

/// The engine and the model are the pinned files: one of another size is another release (an engine an update left
/// behind) and counts as not installed.
pub fn installed(models_root: &Path, build: Build, m: &Model) -> bool {
    let pinned = |path: PathBuf, size: u64| std::fs::metadata(path).is_ok_and(|f| f.is_file() && f.len() == size);
    pinned(engine_dir(models_root, build).join(LIBRARY), build.download().size) && pinned(model_path(models_root, m), m.download.size)
}

/// Вспомогательная модель transcribe.cpp: разметка речи (Silero VAD v6.2, только F32, считается на CPU) и определение
/// языка (VoxLingua107, 107 языков; Q8_0 по замеру авторов точнее F32: 86,30% против 85,23% на FLEURS).
#[derive(Debug)]
pub struct Helper {
    pub id: &'static str,
    pub dir: &'static str,
    pub file: &'static str,
    pub download: Download,
}

pub const VAD: Helper = Helper {
    id: "silero-vad",
    dir: "silero-vad",
    file: "silero-vad-v6.2-F32.gguf",
    download: Download {
        url: "https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/a9b2d61c20ae4ddf3644edb34969e8185a485448/silero-vad-v6.2-F32.gguf",
        size: 1_241_056,
        sha256: "0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6",
    },
};

pub const LANGID: Helper = Helper {
    id: "langid",
    dir: "langid-voxlingua107",
    file: "lang-id-voxlingua107-ecapa-Q8_0.gguf",
    download: Download {
        url: "https://huggingface.co/handy-computer/lang-id-voxlingua107-ecapa-gguf/resolve/e59f0f3847e77723b7d58447ef9456fa0c7e9436/lang-id-voxlingua107-ecapa-Q8_0.gguf",
        size: 26_693_632,
        sha256: "063e6cb0af3d53085fc7d68356dfde5ecee2ffa7bc79855f7e4ff930081bfe3a",
    },
};

pub fn helper_path(models_root: &Path, h: &Helper) -> PathBuf {
    models_root.join(h.dir).join(h.file)
}

/// Знает ли модель `lang`; пусто или auto — язык определяет она сама.
pub fn knows(m: &Model, lang: &str) -> bool {
    lang.is_empty() || lang == "auto" || m.languages.is_empty() || m.languages.contains(&lang)
}

#[repr(C)]
struct RunParams {
    struct_size: u64,
    task: c_int,
    timestamps: c_int,
    pnc: c_int,
    itn: c_int,
    diarize: c_int,
    language: *const c_char,
    target_language: *const c_char,
    keep_special_tags: bool,
    family: *const std::ffi::c_void,
    spec_k_drafts: i32,
    vocabulary: *const *const c_char,
    n_vocabulary: i32,
    prompt: *const c_char,
    prefix: *const c_char,
}

#[repr(C)]
struct RawWord {
    struct_size: u64,
    t0_ms: i64,
    t1_ms: i64,
    seg_index: c_int,
    first_token: c_int,
    n_tokens: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Limits {
    struct_size: u64,
    effective_n_ctx: i32,
    effective_max_audio_ms: i64,
    max_kv_bytes: i64,
}

#[repr(C)]
struct Capabilities {
    struct_size: u64,
    native_sample_rate: i32,
    n_languages: c_int,
    languages: *const *const c_char,
    rest: [u8; 0],
}

#[repr(C)]
struct LoadParams {
    struct_size: u64,
    backend: c_int,
    device: *mut std::ffi::c_void,
    no_mmap: bool,
}

#[repr(C)]
struct VadResult {
    struct_size: u64,
    n_probs: i32,
    first_frame: i64,
    n_segments: i32,
}

#[repr(C)]
struct LangIdParams {
    struct_size: u64,
    allowed: *const *const c_char,
    n_allowed: i32,
}

#[repr(C)]
struct LangIdResult {
    struct_size: u64,
    n_candidates: i32,
    allowed_mass: f32,
}

#[repr(C)]
struct LangIdCandidate {
    struct_size: u64,
    index: i32,
    code: *const c_char,
    name: *const c_char,
    p: f32,
    logit: f32,
}

#[repr(C)]
struct StreamParams {
    struct_size: u64,
    family: *const std::ffi::c_void,
    commit_policy: c_int,
    stable_prefix_agreement_n: u32,
}

#[repr(C)]
struct StreamUpdate {
    struct_size: u64,
    result_changed: bool,
    is_final: bool,
    revision: i32,
    input_received_ms: i64,
    audio_committed_ms: i64,
    buffered_ms: i64,
    committed_changed: bool,
    tentative_changed: bool,
}

#[repr(C)]
struct StreamText {
    struct_size: u64,
    full_text: *const c_char,
    full_text_bytes: u64,
    committed_text: *const c_char,
    committed_text_bytes: u64,
    tentative_text: *const c_char,
    tentative_text_bytes: u64,
    raw_tentative_start_bytes: u64,
}

const ABI_MODEL_LOAD_PARAMS: c_int = 0;
const ABI_RUN_PARAMS: c_int = 2;
const ABI_STREAM_PARAMS: c_int = 3;
const ABI_CAPABILITIES: c_int = 4;
const ABI_WORD: c_int = 7;
const ABI_STREAM_UPDATE: c_int = 9;
const ABI_STREAM_TEXT: c_int = 10;
const ABI_SESSION_LIMITS: c_int = 11;
const ABI_LANGID_PARAMS: c_int = 21;
const ABI_LANGID_RESULT: c_int = 22;
const ABI_LANGID_CANDIDATE: c_int = 23;
const ABI_VAD_RESULT: c_int = 27;
const TIMESTAMPS_WORD: c_int = 3;
const BACKEND_CPU: c_int = 1;

enum RawModel {}
enum RawSession {}
enum RawVad {}
enum RawLangId {}

#[derive(Clone, Copy)]
struct Api {
    status_string: unsafe extern "C" fn(c_int) -> *const c_char,
    abi_size: unsafe extern "C" fn(c_int) -> usize,
    load: unsafe extern "C" fn(*const c_char, *const LoadParams, *mut *mut RawModel) -> c_int,
    load_params_init: unsafe extern "C" fn(*mut LoadParams),
    model_free: unsafe extern "C" fn(*mut RawModel),
    session_init: unsafe extern "C" fn(*mut RawModel, *const std::ffi::c_void, *mut *mut RawSession) -> c_int,
    session_free: unsafe extern "C" fn(*mut RawSession),
    run_params_init: unsafe extern "C" fn(*mut RunParams),
    run: unsafe extern "C" fn(*mut RawSession, *const f32, c_int, *const RunParams) -> c_int,
    n_words: unsafe extern "C" fn(*const RawSession) -> c_int,
    word_init: unsafe extern "C" fn(*mut RawWord),
    get_word: unsafe extern "C" fn(*const RawSession, c_int, *mut RawWord) -> c_int,
    limits_init: unsafe extern "C" fn(*mut Limits),
    get_limits: unsafe extern "C" fn(*const RawSession, *mut Limits) -> c_int,
    caps_init: unsafe extern "C" fn(*mut Capabilities),
    get_caps: unsafe extern "C" fn(*const RawModel, *mut Capabilities) -> c_int,
    run_batch: unsafe extern "C" fn(*mut RawSession, *const *const f32, *const c_int, c_int, *const RunParams) -> c_int,
    batch_status: unsafe extern "C" fn(*const RawSession, c_int) -> c_int,
    batch_n_words: unsafe extern "C" fn(*const RawSession, c_int) -> c_int,
    batch_get_word: unsafe extern "C" fn(*const RawSession, c_int, c_int, *mut RawWord) -> c_int,
    stream_params_init: unsafe extern "C" fn(*mut StreamParams),
    stream_update_init: unsafe extern "C" fn(*mut StreamUpdate),
    stream_text_init: unsafe extern "C" fn(*mut StreamText),
    stream_begin: unsafe extern "C" fn(*mut RawSession, *const RunParams, *const StreamParams) -> c_int,
    stream_feed: unsafe extern "C" fn(*mut RawSession, *const f32, c_int, *mut StreamUpdate) -> c_int,
    stream_finalize: unsafe extern "C" fn(*mut RawSession, *mut StreamUpdate) -> c_int,
    stream_get_text: unsafe extern "C" fn(*const RawSession, *mut StreamText) -> c_int,
    stream_reset: unsafe extern "C" fn(*mut RawSession),
    vad_session_init: unsafe extern "C" fn(*mut RawModel, *const std::ffi::c_void, *mut *mut RawVad) -> c_int,
    vad_session_free: unsafe extern "C" fn(*mut RawVad),
    vad_stream_feed: unsafe extern "C" fn(*mut RawVad, *const f32, c_int) -> c_int,
    vad_stream_flush: unsafe extern "C" fn(*mut RawVad) -> c_int,
    vad_stream_reset: unsafe extern "C" fn(*mut RawVad),
    vad_result_init: unsafe extern "C" fn(*mut VadResult),
    vad_get_result: unsafe extern "C" fn(*const RawVad, *mut VadResult) -> c_int,
    vad_probs: unsafe extern "C" fn(*const RawVad) -> *const f32,
    langid_session_init: unsafe extern "C" fn(*mut RawModel, *const std::ffi::c_void, *mut *mut RawLangId) -> c_int,
    langid_session_free: unsafe extern "C" fn(*mut RawLangId),
    langid_params_init: unsafe extern "C" fn(*mut LangIdParams),
    langid_run: unsafe extern "C" fn(*mut RawLangId, *const f32, c_int, *const LangIdParams) -> c_int,
    langid_result_init: unsafe extern "C" fn(*mut LangIdResult),
    langid_get_result: unsafe extern "C" fn(*const RawLangId, *mut LangIdResult) -> c_int,
    langid_candidate_init: unsafe extern "C" fn(*mut LangIdCandidate),
    langid_get_candidate: unsafe extern "C" fn(*const RawLangId, c_int, *mut LangIdCandidate) -> c_int,
}

/// Куда читается структура возможностей движка: с запасом на её рост, первые поля читает `Capabilities`.
type CapsBuffer = [u64; 64];

/// Каждая сборка грузится один раз на процесс и не выгружается (статический инициализатор ggml второй загрузки той же
/// библиотеки не переживает); смена сборки в настройках берёт другую, уже загруженную или новую.
static LOADED: Mutex<Vec<(PathBuf, Library, Api)>> = Mutex::new(Vec::new());

/// Opens the engine library so that its own folder is searched for the libraries it needs (cuBLAS lies beside the CUDA
/// build): plain `LoadLibraryExW(path, 0, 0)` searches the program's folder and PATH, not the library's.
fn open(path: &Path) -> Result<Library, String> {
    #[cfg(windows)]
    {
        use libloading::os::windows::{Library as WindowsLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
        let full = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
        unsafe { WindowsLibrary::load_with_flags(&full, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map(Library::from)
            .map_err(|e| format!("{}: {e}", full.display()))
    }
    #[cfg(not(windows))]
    unsafe { Library::new(path) }.map_err(|e| format!("{}: {e}", path.display()))
}

fn library(dir: &Path) -> Result<Api, String> {
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, api)) = loaded.iter().find(|(d, _, _)| d == dir) {
        return Ok(*api);
    }
    let path = dir.join(LIBRARY);
    if !path.is_file() {
        return Err(format!("transcribe.cpp: the engine is not installed: {}", path.display()));
    }
    Build::Cpu.runs_here()?;
    let lib = open(&path)?;
    let api = unsafe {
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("transcribe.cpp {}: {e}", $name))?
            };
        }
        Api {
            status_string: sym!("transcribe_status_string"),
            abi_size: sym!("transcribe_abi_struct_size"),
            load: sym!("transcribe_model_load_file"),
            load_params_init: sym!("transcribe_model_load_params_init"),
            model_free: sym!("transcribe_model_free"),
            session_init: sym!("transcribe_session_init"),
            session_free: sym!("transcribe_session_free"),
            run_params_init: sym!("transcribe_run_params_init"),
            run: sym!("transcribe_run"),
            n_words: sym!("transcribe_n_words"),
            word_init: sym!("transcribe_word_init"),
            get_word: sym!("transcribe_get_word"),
            limits_init: sym!("transcribe_session_limits_init"),
            get_limits: sym!("transcribe_session_get_limits"),
            caps_init: sym!("transcribe_capabilities_init"),
            get_caps: sym!("transcribe_model_get_capabilities"),
            run_batch: sym!("transcribe_run_batch"),
            batch_status: sym!("transcribe_batch_status"),
            batch_n_words: sym!("transcribe_batch_n_words"),
            batch_get_word: sym!("transcribe_batch_get_word"),
            stream_params_init: sym!("transcribe_stream_params_init"),
            stream_update_init: sym!("transcribe_stream_update_init"),
            stream_text_init: sym!("transcribe_stream_text_init"),
            stream_begin: sym!("transcribe_stream_begin"),
            stream_feed: sym!("transcribe_stream_feed"),
            stream_finalize: sym!("transcribe_stream_finalize"),
            stream_get_text: sym!("transcribe_stream_get_text"),
            stream_reset: sym!("transcribe_stream_reset"),
            vad_session_init: sym!("transcribe_vad_session_init"),
            vad_session_free: sym!("transcribe_vad_session_free"),
            vad_stream_feed: sym!("transcribe_vad_stream_feed"),
            vad_stream_flush: sym!("transcribe_vad_stream_flush"),
            vad_stream_reset: sym!("transcribe_vad_stream_reset"),
            vad_result_init: sym!("transcribe_vad_result_init"),
            vad_get_result: sym!("transcribe_vad_get_result"),
            vad_probs: sym!("transcribe_vad_probs"),
            langid_session_init: sym!("transcribe_langid_session_init"),
            langid_session_free: sym!("transcribe_langid_session_free"),
            langid_params_init: sym!("transcribe_langid_params_init"),
            langid_run: sym!("transcribe_langid_run"),
            langid_result_init: sym!("transcribe_langid_result_init"),
            langid_get_result: sym!("transcribe_langid_get_result"),
            langid_candidate_init: sym!("transcribe_langid_candidate_init"),
            langid_get_candidate: sym!("transcribe_langid_get_candidate"),
        }
    };
    // структуры разложены здесь вручную: библиотеку другой раскладки отвергаем до первого вызова, что их заполняет
    for (which, ours, name) in [
        (ABI_RUN_PARAMS, std::mem::size_of::<RunParams>(), "run params"),
        (ABI_WORD, std::mem::size_of::<RawWord>(), "word"),
        (ABI_SESSION_LIMITS, std::mem::size_of::<Limits>(), "session limits"),
        (ABI_STREAM_PARAMS, std::mem::size_of::<StreamParams>(), "stream params"),
        (ABI_STREAM_UPDATE, std::mem::size_of::<StreamUpdate>(), "stream update"),
        (ABI_STREAM_TEXT, std::mem::size_of::<StreamText>(), "stream text"),
        (ABI_VAD_RESULT, std::mem::size_of::<VadResult>(), "vad result"),
        (ABI_LANGID_PARAMS, std::mem::size_of::<LangIdParams>(), "langid params"),
        (ABI_LANGID_RESULT, std::mem::size_of::<LangIdResult>(), "langid result"),
        (ABI_LANGID_CANDIDATE, std::mem::size_of::<LangIdCandidate>(), "langid candidate"),
    ] {
        let theirs = unsafe { (api.abi_size)(which) };
        if theirs != ours {
            return Err(format!("transcribe.cpp {}: its {name} struct is {theirs} bytes, ours {ours}", path.display()));
        }
    }
    // the load params grow at their end (`no_mmap` came after engine-0.1.0): the library stamps its own size, and only the
    // backend and the device before it are set here
    let load = unsafe { (api.abi_size)(ABI_MODEL_LOAD_PARAMS) };
    if load < std::mem::offset_of!(LoadParams, no_mmap) || load > std::mem::size_of::<LoadParams>() {
        return Err(format!("transcribe.cpp {}: its model load params struct is {load} bytes, ours {}", path.display(), std::mem::size_of::<LoadParams>()));
    }
    let caps = unsafe { (api.abi_size)(ABI_CAPABILITIES) };
    if caps < std::mem::size_of::<Capabilities>() || caps > std::mem::size_of::<CapsBuffer>() {
        return Err(format!("transcribe.cpp {}: its capabilities struct is {caps} bytes, we read {}..{}", path.display(), std::mem::size_of::<Capabilities>(), std::mem::size_of::<CapsBuffer>()));
    }
    loaded.push((dir.to_path_buf(), lib, api));
    Ok(api)
}

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

fn engine_err(e: String) -> String {
    e
}

/// Слово распознавания: текст и время от начала звука, с.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

/// The name of `lang` among the languages a model lists: as listed, or the listed region of a plain code («ru» →
/// «ru-RU»). None for «auto», no language, or a model that lists none: the model is then told nothing.
fn language_of(languages: &[String], lang: &str) -> Result<Option<String>, String> {
    let lang = lang.trim();
    if lang.is_empty() || lang.eq_ignore_ascii_case("auto") || languages.is_empty() {
        return Ok(None);
    }
    let prefix = format!("{}-", lang.to_ascii_lowercase());
    let found = languages.iter().find(|l| l.eq_ignore_ascii_case(lang)).or_else(|| languages.iter().find(|l| l.to_ascii_lowercase().starts_with(&prefix)));
    match found {
        Some(l) => Ok(Some(l.clone())),
        None => Err(engine_err(format!("transcribe.cpp: the model does not recognise «{lang}» (it knows {})", languages.join(", ")))),
    }
}

/// Модель на видеокарте и её сессия; освобождаются при drop.
pub struct TcppAsr {
    api: Api,
    model: *mut RawModel,
    session: *mut RawSession,
    languages: Vec<String>,
    /// Самый длинный звук за один прогон, с; 0 — без предела (длинное семейство режет само).
    max_secs: f64,
    /// На модели идёт поток (`TcppStream`): прогоны до его конца отвечают «занято», как в официальных привязках.
    streaming: bool,
}

// Один прогон на сессию за раз держит вызывающий (через &mut или Mutex).
unsafe impl Send for TcppAsr {}

impl Drop for TcppAsr {
    fn drop(&mut self) {
        unsafe {
            (self.api.session_free)(self.session);
            (self.api.model_free)(self.model);
        }
    }
}

impl TcppAsr {
    /// Загрузить модель `model` движком из `engine_dir`.
    pub fn load(engine_dir: &Path, model: &Path) -> Result<TcppAsr, String> {
        let api = library(engine_dir)?;
        if !model.is_file() {
            return Err(format!("transcribe.cpp: the model is not downloaded: {}", model.display()));
        }
        let c_path = CString::new(model.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let status = |s: c_int| text(unsafe { (api.status_string)(s) });
        let mut raw: *mut RawModel = std::ptr::null_mut();
        let s = unsafe { (api.load)(c_path.as_ptr(), std::ptr::null(), &mut raw) };
        if s != 0 || raw.is_null() {
            return Err(format!("transcribe.cpp {}: {}", model.display(), status(s)));
        }
        let mut session: *mut RawSession = std::ptr::null_mut();
        let s = unsafe { (api.session_init)(raw, std::ptr::null(), &mut session) };
        if s != 0 || session.is_null() {
            unsafe { (api.model_free)(raw) };
            return Err(format!("transcribe.cpp {}: {}", model.display(), status(s)));
        }
        let mut caps = std::mem::MaybeUninit::<CapsBuffer>::zeroed();
        let caps_ptr = caps.as_mut_ptr() as *mut Capabilities;
        let mut languages = Vec::new();
        unsafe {
            (api.caps_init)(caps_ptr);
            if (api.get_caps)(raw, caps_ptr) == 0 {
                let c = &*caps_ptr;
                let listed = if c.languages.is_null() { 0 } else { c.n_languages.max(0) as usize };
                for i in 0..listed {
                    languages.push(text(*c.languages.add(i)));
                }
            }
        }
        let mut limits = Limits { struct_size: 0, effective_n_ctx: 0, effective_max_audio_ms: 0, max_kv_bytes: 0 };
        let max_secs = unsafe {
            (api.limits_init)(&mut limits);
            if (api.get_limits)(session, &mut limits) == 0 { limits.effective_max_audio_ms.max(0) as f64 / 1000.0 } else { 0.0 }
        };
        Ok(TcppAsr { api, model: raw, session, languages, max_secs, streaming: false })
    }

    fn busy(&self) -> Result<(), String> {
        if self.streaming {
            return Err(engine_err("transcribe.cpp: the model is busy with live recognition".into()));
        }
        Ok(())
    }

    /// Имя `lang` у самой модели: «ru» — это «ru-RU» у модели, что пишет языки с регионом. «auto» и пустой язык
    /// модели не называются: модели с языками-регионами (Nemotron) строку «auto» не принимают.
    fn language(&self, lang: &str) -> Result<Option<CString>, String> {
        language_of(&self.languages, lang).map(|l| l.map(|l| CString::new(l).expect("model language names hold no NUL")))
    }

    /// Слова 16 кГц моно со временем от его начала, с.
    pub fn words(&mut self, pcm: &[f32], lang: &str) -> Result<Vec<Word>, String> {
        self.busy()?;
        let language = self.language(lang)?;
        let window = if self.max_secs > 0.0 { (self.max_secs * 16_000.0) as usize } else { pcm.len() }.max(16_000);
        let mut out = Vec::new();
        for (n, piece) in pcm.chunks(window).enumerate() {
            let offset = (n * window) as f64 / 16_000.0;
            let params = self.run_params(&language);
            let s = unsafe { (self.api.run)(self.session, piece.as_ptr(), piece.len() as c_int, &params) };
            if s != 0 {
                return Err(engine_err(format!("transcribe.cpp: {}", text(unsafe { (self.api.status_string)(s) }))));
            }
            let n_words = unsafe { (self.api.n_words)(self.session) };
            for i in 0..n_words.max(0) {
                let mut w = std::mem::MaybeUninit::<RawWord>::zeroed();
                unsafe {
                    (self.api.word_init)(w.as_mut_ptr());
                    if (self.api.get_word)(self.session, i, w.as_mut_ptr()) != 0 {
                        continue;
                    }
                    let w = w.assume_init();
                    let word = text(w.text);
                    if word.trim().is_empty() {
                        continue;
                    }
                    out.push(Word { word: word.trim().to_string(), start: offset + w.t0_ms as f64 / 1000.0, end: offset + w.t1_ms as f64 / 1000.0 });
                }
            }
        }
        Ok(out)
    }
}

impl TcppAsr {
    fn run_params(&self, language: &Option<CString>) -> RunParams {
        let mut p = std::mem::MaybeUninit::<RunParams>::zeroed();
        unsafe {
            (self.api.run_params_init)(p.as_mut_ptr());
            let mut params = p.assume_init();
            params.timestamps = TIMESTAMPS_WORD;
            params.language = language.as_ref().map_or(std::ptr::null(), |l| l.as_ptr());
            params
        }
    }

    /// Слова нескольких коротких отрывков 16 кГц моно одним пакетным прогоном; ошибка отрывка — в его элементе.
    /// Отрывок длиннее того, что модель берёт за раз, идёт сам по себе через `words`.
    pub fn words_batch(&mut self, pieces: &[&[f32]], lang: &str) -> Result<Vec<Result<Vec<Word>, String>>, String> {
        self.busy()?;
        let language = self.language(lang)?;
        let limit = if self.max_secs > 0.0 { (self.max_secs * 16_000.0) as usize } else { usize::MAX };
        let mut out: Vec<Option<Result<Vec<Word>, String>>> = (0..pieces.len()).map(|_| None).collect();
        let short: Vec<usize> = (0..pieces.len()).filter(|&i| !pieces[i].is_empty() && pieces[i].len() <= limit).collect();
        for i in (0..pieces.len()).filter(|i| !short.contains(i)) {
            out[i] = Some(if pieces[i].is_empty() { Ok(Vec::new()) } else { self.words(pieces[i], lang) });
        }
        for group in short.chunks(16) {
            let ptrs: Vec<*const f32> = group.iter().map(|&i| pieces[i].as_ptr()).collect();
            let lens: Vec<c_int> = group.iter().map(|&i| pieces[i].len() as c_int).collect();
            let params = self.run_params(&language);
            let s = unsafe { (self.api.run_batch)(self.session, ptrs.as_ptr(), lens.as_ptr(), group.len() as c_int, &params) };
            if s != 0 {
                return Err(engine_err(format!("transcribe.cpp: {}", text(unsafe { (self.api.status_string)(s) }))));
            }
            for (k, &i) in group.iter().enumerate() {
                let status = unsafe { (self.api.batch_status)(self.session, k as c_int) };
                if status != 0 {
                    out[i] = Some(Err(engine_err(format!("transcribe.cpp: {}", text(unsafe { (self.api.status_string)(status) })))));
                    continue;
                }
                let n = unsafe { (self.api.batch_n_words)(self.session, k as c_int) };
                let mut words = Vec::new();
                for j in 0..n.max(0) {
                    let mut w = std::mem::MaybeUninit::<RawWord>::zeroed();
                    unsafe {
                        (self.api.word_init)(w.as_mut_ptr());
                        if (self.api.batch_get_word)(self.session, k as c_int, j, w.as_mut_ptr()) != 0 {
                            continue;
                        }
                        let w = w.assume_init();
                        let word = text(w.text);
                        if !word.trim().is_empty() {
                            words.push(Word { word: word.trim().to_string(), start: w.t0_ms as f64 / 1000.0, end: w.t1_ms as f64 / 1000.0 });
                        }
                    }
                }
                out[i] = Some(Ok(words));
            }
        }
        Ok(out.into_iter().map(|r| r.expect("every piece answered")).collect())
    }
}

/// Распознавание по ходу речи: своя сессия на загруженной модели; вычисления идут под тем же замком, что и
/// прогоны модели (transcribe.cpp не считает две сессии одной модели одновременно).
pub struct TcppStream {
    asr: Arc<Mutex<TcppAsr>>,
    api: Api,
    session: *mut RawSession,
    language: Option<CString>,
    open: bool,
}

unsafe impl Send for TcppStream {}

impl Drop for TcppStream {
    fn drop(&mut self) {
        let mut asr = self.asr.lock().unwrap_or_else(|e| e.into_inner());
        if self.open {
            asr.streaming = false;
        }
        unsafe { (self.api.session_free)(self.session) };
    }
}

impl TcppStream {
    /// Поток на модели `model` (из кэша процесса) для языка `lang` (пусто или auto — модель решает сама).
    pub fn new(engine_dir: &Path, model: &Path, lang: &str) -> Result<TcppStream, String> {
        let asr = shared(engine_dir, model)?;
        let (api, session, language) = {
            let guard = asr.lock().unwrap_or_else(|e| e.into_inner());
            let language = guard.language(lang)?;
            let mut session: *mut RawSession = std::ptr::null_mut();
            let s = unsafe { (guard.api.session_init)(guard.model, std::ptr::null(), &mut session) };
            if s != 0 || session.is_null() {
                return Err(engine_err(format!("transcribe.cpp stream: {}", text(unsafe { (guard.api.status_string)(s) }))));
            }
            (guard.api, session, language)
        };
        Ok(TcppStream { asr, api, session, language, open: false })
    }

    fn check(&self, s: c_int) -> Result<(), String> {
        if s == 0 {
            Ok(())
        } else {
            Err(engine_err(format!("transcribe.cpp stream: {}", text(unsafe { (self.api.status_string)(s) }))))
        }
    }

    fn so_far(&self) -> Result<String, String> {
        let mut out = std::mem::MaybeUninit::<StreamText>::zeroed();
        unsafe {
            (self.api.stream_text_init)(out.as_mut_ptr());
            self.check((self.api.stream_get_text)(self.session, out.as_mut_ptr()))?;
            Ok(text(out.assume_init().full_text).trim().to_string())
        }
    }

    /// Сбросить поток после ошибки и отпустить модель.
    fn abandon(&mut self, asr: &mut TcppAsr) {
        unsafe { (self.api.stream_reset)(self.session) };
        self.open = false;
        asr.streaming = false;
    }

    /// Дописать звук фразы (16 кГц моно); текст фразы на сейчас. Ошибка сбрасывает поток.
    pub fn feed(&mut self, pcm: &[f32]) -> Result<String, String> {
        let asr = self.asr.clone();
        let mut guard = asr.lock().unwrap_or_else(|e| e.into_inner());
        let said = self.feed_locked(&mut guard, pcm);
        if said.is_err() {
            self.abandon(&mut guard);
        }
        said
    }

    fn feed_locked(&mut self, asr: &mut TcppAsr, pcm: &[f32]) -> Result<String, String> {
        if !self.open {
            asr.busy()?;
            let mut rp = std::mem::MaybeUninit::<RunParams>::zeroed();
            let mut sp = std::mem::MaybeUninit::<StreamParams>::zeroed();
            unsafe {
                (self.api.run_params_init)(rp.as_mut_ptr());
                let mut run = rp.assume_init();
                run.language = self.language.as_ref().map_or(std::ptr::null(), |l| l.as_ptr());
                (self.api.stream_params_init)(sp.as_mut_ptr());
                self.check((self.api.stream_begin)(self.session, &run, sp.as_ptr()))?;
            }
            self.open = true;
            asr.streaming = true;
        }
        if !pcm.is_empty() {
            let mut u = std::mem::MaybeUninit::<StreamUpdate>::zeroed();
            unsafe {
                (self.api.stream_update_init)(u.as_mut_ptr());
                self.check((self.api.stream_feed)(self.session, pcm.as_ptr(), pcm.len() as c_int, u.as_mut_ptr()))?;
            }
        }
        self.so_far()
    }

    /// Закончить фразу: её окончательный текст; следующий `feed` начинает новую. Модель свободна после этого.
    pub fn finish(&mut self) -> Result<String, String> {
        if !self.open {
            return Ok(String::new());
        }
        let asr = self.asr.clone();
        let mut guard = asr.lock().unwrap_or_else(|e| e.into_inner());
        let mut u = std::mem::MaybeUninit::<StreamUpdate>::zeroed();
        let said = unsafe {
            (self.api.stream_update_init)(u.as_mut_ptr());
            self.check((self.api.stream_finalize)(self.session, u.as_mut_ptr())).and_then(|_| self.so_far())
        };
        self.abandon(&mut guard);
        said
    }
}

/// Мелкая модель (разметка речи, определение языка) — на CPU: поднять её на видеокарту дольше, чем она считает.
fn load_helper(api: &Api, path: &Path) -> Result<*mut RawModel, String> {
    if !path.is_file() {
        return Err(format!("transcribe.cpp: the model is not downloaded: {}", path.display()));
    }
    let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let mut raw: *mut RawModel = std::ptr::null_mut();
    let mut lp = std::mem::MaybeUninit::<LoadParams>::zeroed();
    let s = unsafe {
        (api.load_params_init)(lp.as_mut_ptr());
        let mut params = lp.assume_init();
        params.backend = BACKEND_CPU;
        (api.load)(c_path.as_ptr(), &params, &mut raw)
    };
    if s != 0 || raw.is_null() {
        return Err(format!("transcribe.cpp {}: {}", path.display(), text(unsafe { (api.status_string)(s) })));
    }
    Ok(raw)
}

struct Vad {
    api: Api,
    model: *mut RawModel,
    session: *mut RawVad,
}

unsafe impl Send for Vad {}

impl Drop for Vad {
    fn drop(&mut self) {
        unsafe {
            (self.api.vad_session_free)(self.session);
            (self.api.model_free)(self.model);
        }
    }
}

static VAD_LOADED: Mutex<Option<((PathBuf, PathBuf), Vad)>> = Mutex::new(None);

/// Вероятность речи на каждый кадр в 32 мс (512 отсчётов 16 кГц моно) по Silero VAD v6.2 из `model`.
pub fn vad_probabilities(engine_dir: &Path, model: &Path, pcm: &[f32]) -> Result<Vec<f32>, String> {
    let api = library(engine_dir)?;
    let key = (engine_dir.to_path_buf(), model.to_path_buf());
    let mut slot = VAD_LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_none_or(|(k, _)| *k != key) {
        *slot = None;
        let raw = load_helper(&api, model)?;
        let mut session: *mut RawVad = std::ptr::null_mut();
        let s = unsafe { (api.vad_session_init)(raw, std::ptr::null(), &mut session) };
        if s != 0 || session.is_null() {
            unsafe { (api.model_free)(raw) };
            return Err(format!("transcribe.cpp {}: {}", model.display(), text(unsafe { (api.status_string)(s) })));
        }
        *slot = Some((key, Vad { api, model: raw, session }));
    }
    let vad = &slot.as_ref().expect("loaded above").1;
    let mut probs = Vec::with_capacity(pcm.len() / 512 + 1);
    let mut take = |s: c_int| -> Result<(), String> {
        if s != 0 {
            return Err(format!("transcribe.cpp vad: {}", text(unsafe { (api.status_string)(s) })));
        }
        let mut r = std::mem::MaybeUninit::<VadResult>::zeroed();
        unsafe {
            (api.vad_result_init)(r.as_mut_ptr());
            let got = (api.vad_get_result)(vad.session, r.as_mut_ptr());
            if got != 0 {
                return Err(format!("transcribe.cpp vad result: {}", text((api.status_string)(got))));
            }
            let r = r.assume_init();
            if r.n_probs > 0 && r.first_frame != probs.len() as i64 {
                return Err(format!("transcribe.cpp vad: frame {} came after {} frames", r.first_frame, probs.len()));
            }
            let n = r.n_probs.max(0) as usize;
            let p = (api.vad_probs)(vad.session);
            if n > 0 && !p.is_null() {
                probs.extend_from_slice(std::slice::from_raw_parts(p, n));
            }
        }
        Ok(())
    };
    unsafe { (api.vad_stream_reset)(vad.session) };
    for piece in pcm.chunks(16_000 * 60) {
        take(unsafe { (api.vad_stream_feed)(vad.session, piece.as_ptr(), piece.len() as c_int) })?;
    }
    take(unsafe { (api.vad_stream_flush)(vad.session) })?;
    Ok(probs)
}

struct LangId {
    api: Api,
    model: *mut RawModel,
    session: *mut RawLangId,
}

unsafe impl Send for LangId {}

impl Drop for LangId {
    fn drop(&mut self) {
        unsafe {
            (self.api.langid_session_free)(self.session);
            (self.api.model_free)(self.model);
        }
    }
}

static LANGID_LOADED: Mutex<Option<((PathBuf, PathBuf), LangId)>> = Mutex::new(None);

/// Короче этого (0,5 с) определение языка не слушает: `INPUT_TOO_SHORT`.
pub const LANGID_MIN_SAMPLES: usize = 8_000;

/// Ответ определения языка: языки по убыванию вероятности (среди разрешённых) и доля всей вероятности, что пришлась
/// на разрешённые (1 без ограничения; мала — речь, вероятно, на другом языке).
#[derive(Debug, Clone)]
pub struct LangGuess {
    pub ranked: Vec<(String, f32)>,
    pub allowed_mass: f32,
}

/// Язык речи 16 кГц моно по VoxLingua107 из `model` (первые 30 с), выбор среди `allowed` (пусто — все 107).
pub fn detect_language(engine_dir: &Path, model: &Path, pcm: &[f32], allowed: &[&str]) -> Result<LangGuess, String> {
    let api = library(engine_dir)?;
    let key = (engine_dir.to_path_buf(), model.to_path_buf());
    let mut slot = LANGID_LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_none_or(|(k, _)| *k != key) {
        *slot = None;
        let raw = load_helper(&api, model)?;
        let mut session: *mut RawLangId = std::ptr::null_mut();
        let s = unsafe { (api.langid_session_init)(raw, std::ptr::null(), &mut session) };
        if s != 0 || session.is_null() {
            unsafe { (api.model_free)(raw) };
            return Err(format!("transcribe.cpp {}: {}", model.display(), text(unsafe { (api.status_string)(s) })));
        }
        *slot = Some((key, LangId { api, model: raw, session }));
    }
    let id = &slot.as_ref().expect("loaded above").1;
    let codes: Vec<CString> = allowed.iter().map(|c| CString::new(*c).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    let ptrs: Vec<*const c_char> = codes.iter().map(|c| c.as_ptr()).collect();
    let mut lp = std::mem::MaybeUninit::<LangIdParams>::zeroed();
    let params = unsafe {
        (api.langid_params_init)(lp.as_mut_ptr());
        let mut params = lp.assume_init();
        if !ptrs.is_empty() {
            params.allowed = ptrs.as_ptr();
            params.n_allowed = ptrs.len() as i32;
        }
        params
    };
    let s = unsafe { (api.langid_run)(id.session, pcm.as_ptr(), pcm.len() as c_int, &params) };
    if s != 0 {
        return Err(format!("transcribe.cpp langid: {}", text(unsafe { (api.status_string)(s) })));
    }
    let mut r = std::mem::MaybeUninit::<LangIdResult>::zeroed();
    let (n, allowed_mass) = unsafe {
        (api.langid_result_init)(r.as_mut_ptr());
        if (api.langid_get_result)(id.session, r.as_mut_ptr()) != 0 {
            return Err("transcribe.cpp langid: no result".into());
        }
        let r = r.assume_init();
        (r.n_candidates.max(0), r.allowed_mass)
    };
    let mut out = Vec::new();
    for i in 0..n.min(5) {
        let mut c = std::mem::MaybeUninit::<LangIdCandidate>::zeroed();
        unsafe {
            (api.langid_candidate_init)(c.as_mut_ptr());
            if (api.langid_get_candidate)(id.session, i, c.as_mut_ptr()) == 0 {
                let c = c.assume_init();
                if !c.code.is_null() {
                    out.push((text(c.code), c.p));
                }
            }
        }
    }
    Ok(LangGuess { ranked: out, allowed_mass })
}

/// Загруженная модель процесса: одна на видеокарте, общая для всех задач, пока не попросят другую или `unload`.
static SHARED: Mutex<Option<((PathBuf, PathBuf), Arc<Mutex<TcppAsr>>)>> = Mutex::new(None);

/// Модель `model` сборки `engine_dir` из кэша процесса; другая модель или сборка в кэше выгружается.
pub fn shared(engine_dir: &Path, model: &Path) -> Result<Arc<Mutex<TcppAsr>>, String> {
    let key = (engine_dir.to_path_buf(), model.to_path_buf());
    let mut slot = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, asr)) = slot.as_ref() {
        if *k == key {
            return Ok(asr.clone());
        }
    }
    *slot = None;
    let asr = Arc::new(Mutex::new(TcppAsr::load(engine_dir, model)?));
    *slot = Some((key, asr.clone()));
    Ok(asr)
}

/// Выгрузить модель из кэша процесса (видеопамять освобождается, когда её отпустит последняя задача).
pub fn unload() {
    *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_its_own_folder_and_a_pinned_download() {
        let mut dirs: Vec<&str> = MODELS.iter().map(|m| m.dir).collect();
        dirs.sort();
        dirs.dedup();
        assert_eq!(dirs.len(), MODELS.len());
        let mut components: Vec<&str> = MODELS.iter().map(|m| m.component).collect();
        components.sort();
        components.dedup();
        assert_eq!(components.len(), MODELS.len());
        for m in MODELS {
            assert!(m.download.url.contains("/resolve/") && m.download.url.ends_with(m.file), "{}", m.id);
            assert_eq!(m.download.sha256.len(), 64, "{}", m.id);
            assert!(m.download.size > 0, "{}", m.id);
        }
    }

    /// A CUDA build (DUB_TCPP_ENGINE_DIR: transcribe_engine.dll with cuBLAS beside it) loads with PATH holding only
    /// the system folder.
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn the_engine_finds_the_libraries_beside_it() {
        let Ok(dir) = std::env::var("DUB_TCPP_ENGINE_DIR") else { return };
        let system = std::env::var("SystemRoot").map(|r| format!("{r}\\System32")).unwrap();
        std::env::set_var("PATH", system);
        library(Path::new(&dir)).expect("the engine and the libraries beside it");
    }

    #[test]
    fn an_engine_of_another_release_is_not_installed() {
        static TINY: Model = Model {
            id: "tiny",
            name: "tiny",
            dir: "tiny-model",
            component: "tiny",
            file: "tiny.gguf",
            download: Download { url: "", size: 3, sha256: "" },
            languages: &[],
        };
        let root = std::env::temp_dir().join(format!("dub-tcpp-pin-{}", std::process::id()));
        let lib = engine_dir(&root, Build::Cpu).join(LIBRARY);
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        std::fs::create_dir_all(root.join(TINY.dir)).unwrap();
        std::fs::write(model_path(&root, &TINY), b"abc").unwrap();
        let engine = std::fs::File::create(&lib).unwrap();
        engine.set_len(Build::Cpu.download().size + 4096).unwrap();
        assert!(!installed(&root, Build::Cpu, &TINY), "an older engine of another size");
        engine.set_len(Build::Cpu.download().size).unwrap();
        drop(engine);
        assert!(installed(&root, Build::Cpu, &TINY));
        std::fs::write(model_path(&root, &TINY), b"abcd").unwrap();
        assert!(!installed(&root, Build::Cpu, &TINY), "a model of another size");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn auto_and_no_language_tell_the_model_nothing() {
        let regional: Vec<String> = ["en-US", "ru-RU", "de-DE"].iter().map(|s| s.to_string()).collect();
        let plain: Vec<String> = ["en", "ru"].iter().map(|s| s.to_string()).collect();
        for languages in [&regional, &plain] {
            assert_eq!(language_of(languages, "auto").unwrap(), None);
            assert_eq!(language_of(languages, " AUTO ").unwrap(), None);
            assert_eq!(language_of(languages, "").unwrap(), None);
        }
        assert_eq!(language_of(&regional, "ru").unwrap().as_deref(), Some("ru-RU"));
        assert_eq!(language_of(&regional, "ru-RU").unwrap().as_deref(), Some("ru-RU"));
        assert_eq!(language_of(&plain, "ru").unwrap().as_deref(), Some("ru"));
        assert!(language_of(&regional, "ja").unwrap_err().contains("does not recognise"));
        assert_eq!(language_of(&[], "ja").unwrap(), None);
    }

    /// A model hears a language named by its plain code: DUB_TCPP_ENGINE_DIR (an engine), DUB_TCPP_LANG_MODELS
    /// (models separated by `;`), DUB_TCPP_LANG_WAV (16 kHz mono speech) and DUB_TCPP_LANG (its language, `ru`).
    #[test]
    #[ignore]
    fn a_plain_language_code_reaches_every_model() {
        let (Ok(dir), Ok(models), Ok(wav), Ok(lang)) =
            (std::env::var("DUB_TCPP_ENGINE_DIR"), std::env::var("DUB_TCPP_LANG_MODELS"), std::env::var("DUB_TCPP_LANG_WAV"), std::env::var("DUB_TCPP_LANG"))
        else {
            return;
        };
        let mut r = hound::WavReader::open(&wav).unwrap();
        let pcm: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        for model in models.split(';') {
            let mut asr = TcppAsr::load(Path::new(&dir), Path::new(model)).unwrap();
            let named = asr.language(&lang).unwrap().map(|c| c.into_string().unwrap());
            let words = asr.words(&pcm, &lang).unwrap_or_else(|e| panic!("{model} with {lang:?} (sent {named:?}): {e}"));
            println!("[OK] {model}: {lang:?} sent as {named:?}, {} words: {}", words.len(), words.iter().map(|w| w.word.as_str()).collect::<Vec<_>>().join(" "));
            assert!(!words.is_empty(), "{model}");
        }
    }

    #[test]
    fn every_build_has_its_own_folder_and_a_pinned_download() {
        let mut dirs: Vec<&str> = Build::ALL.iter().map(|b| b.dir()).collect();
        dirs.sort();
        dirs.dedup();
        assert_eq!(dirs.len(), Build::ALL.len());
        // a Mac has one build, in every slot
        #[cfg(not(target_os = "macos"))]
        {
            let mut urls: Vec<&str> = Build::ALL.iter().map(|b| b.download().url).collect();
            urls.sort();
            urls.dedup();
            assert_eq!(urls.len(), Build::ALL.len(), "one release holds every build, so their file names differ");
        }
        for b in Build::ALL {
            let d = b.download();
            assert!(d.url.contains("/releases/download/engine-"), "{b:?}");
            assert_eq!(d.sha256.len(), 64, "{b:?}");
            assert!(d.size > 0, "{b:?}");
        }
    }

    #[test]
    fn the_settings_name_their_model() {
        assert_eq!(chosen("", None).unwrap().id, "parakeet-v3");
        assert_eq!(chosen("parakeet", Some("int8")).unwrap().id, "parakeet-v3");
        assert_eq!(chosen("parakeet", Some("fp32")).unwrap().id, "parakeet-v3");
        assert_eq!(chosen("parakeet", Some("ultra")).unwrap().id, "parakeet-ultra");
        assert_eq!(chosen("parakeet", Some("ultra-int8")).unwrap().id, "parakeet-ultra");
        assert_eq!(chosen("nemotron", None).unwrap().id, "nemotron");
        assert!(chosen("whisper", None).is_none());
    }

    /// The engine of `TCPP_ENGINE_DIR` hears `TCPP_PCM` (16 kHz mono f32 little-endian) with `TCPP_MODEL` and finds every
    /// word of `TCPP_EXPECT` in it.
    #[test]
    #[ignore = "needs TCPP_ENGINE_DIR, TCPP_MODEL, TCPP_PCM and TCPP_EXPECT"]
    fn the_engine_hears_a_recording() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let bytes = std::fs::read(var("TCPP_PCM")).expect("the recording");
        let pcm: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let asr = shared(Path::new(&var("TCPP_ENGINE_DIR")), Path::new(&var("TCPP_MODEL"))).expect("load");
        let words = asr.lock().unwrap().words(&pcm, "en").expect("recognition");
        let heard: String = words.iter().map(|w| w.word.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
        eprintln!("heard: {heard}");
        for expected in var("TCPP_EXPECT").split_whitespace() {
            assert!(heard.contains(&expected.to_lowercase()), "{expected} is not in: {heard}");
        }
    }

    #[test]
    fn a_model_knows_its_languages_and_auto() {
        let p = model("parakeet-v3").unwrap();
        assert!(knows(p, "ru") && knows(p, "auto") && knows(p, ""));
        assert!(!knows(p, "ja"));
        assert!(knows(model("nemotron").unwrap(), "ja"));
    }
}
