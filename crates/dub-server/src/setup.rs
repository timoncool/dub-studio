//! setup — «первый запуск»: манифест всех внешних компонентов (модели, движки-сайдкары, системные
//! библиотеки) + диагностика их наличия на диске + закачка одной кнопкой: свести ручную установку к
//! единственному шагу (драйвер NVIDIA), убрав из README требование ставить CUDA Toolkit, VC++ и
//! качать веса вручную.
//!
//! Каждый скачиваемый файл закреплён: HF — коммитом в URL, GitHub/PyPI/NVIDIA — версией в пути, и у каждого
//! есть точный размер и SHA-256. Файл публикуется только после сверки хэша; архив раскладывается и оставляет
//! рядом запись `<архив>.json` (sha256 архива + что и какого размера он положил) — по ней «установлено»
//! значит «установлено ИМЕННО закреплённой версией», а удаление снимает ровно положенные файлы.
//!
//! Закачка идёт вне GPU-очереди (фоновый менеджер `downloads`, а on-demand догрузка — внутри джобы), кусками
//! по Range с докачкой после обрыва/перезапуска; 429/5xx — ожидание, а не ошибка; общий бюджет соединений на
//! все закачки процесса.
//!
//! Классы источников:
//!   • модели — прямые файлы HF (higgs-q8_0/*, gemma-4 + mmproj, parakeet-tdt int8, nemotron 3
//!     diarization, roformer voc_fv6-Q8_0);
//!   • сайдкары/движки — zip-релизы GitHub (BSRoformer.cpp v0.1.0, llama.cpp b11900 win-cuda-13.4,
//!     onnxruntime 1.28.2, ffmpeg BtbN, deno 2.9.8) + audiocpp_engine.dll (своя сборка с sm_75, GitHub timoncool/Higgs-Ultimate) + yt-dlp.exe 2026.10.21;
//!   • CUDA-runtime — PyPI-wheel'ы NVIDIA (cudart 13.4.92 / cublas 13.8.0.4 / cuDNN 9.27.0.42) + redist cuFFT
//!     12.4.0.43, распаковка *.dll плоско;
//!   • VC++ runtime + OCR-модели — БАНДЛ (кладутся в релиз рядом с exe, как VC++ в Higgs); не качаются,
//!     но статус показываем;
//!   • драйвер NVIDIA — диагностика версии драйвера и compute capability (hw::gpu_report), «скачивание» =
//!     открыть сайт (кнопка во фронте).
//!
//! Сайдкары, движок Higgs и CUDA-библиотеки у каждой платформы свои (модуль `platform`): на Linux это tar-сборки
//! тех же релизов, колёса manylinux и redist NVIDIA linux-x86_64, а VC++ runtime не нужен.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

// ── Тип файла-члена компонента ──────────────────────────────────────────────

/// Одна закачиваемая единица компонента: закреплённый URL + относительный путь назначения (от repo_root) +
/// точный размер и SHA-256 того, что отдаёт этот URL. Для zip/wheel dest_rel задаёт каталог распаковки и имя
/// записи об установке (`<dest_rel>.json`), сам архив лежит только на время закачки (см. `Extract`).
#[derive(Clone, Debug)]
pub struct FileSpec {
    pub url: &'static str,
    /// Куда лечь ФАЙЛУ (для прямых файлов) ИЛИ имя архива в каталоге распаковки (для zip/wheel).
    pub dest_rel: &'static str,
    /// Точный размер в байтах.
    pub size: u64,
    /// SHA-256 (hex, нижний регистр): HF — lfs.oid закреплённой ревизии, GitHub — digest ассета, PyPI/NVIDIA —
    /// их манифесты.
    pub sha256: &'static str,
    pub extract: Extract,
}

/// Что делать со скачанным файлом. Архив — zip/wheel, tar.gz или tar.xz (формат по имени в dest_rel), после
/// раскладки он удаляется.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extract {
    /// Прямой файл — оставить как есть (dest_rel = финальный путь).
    None,
    /// Все файлы плоско (только имена) в каталог dest_rel-родителя: движки-сайдкары (программа + библиотеки).
    Flat,
    /// Только ffmpeg и ffprobe, плоско в каталог.
    Pick,
    /// Весь архив с сохранением поддерева: onnxruntime, чтобы `onnxruntime-*/lib/…` лёг ровно там, где его
    /// ищет dub-asr::ensure_ort_dylib.
    Tree,
    /// Только динамические библиотеки платформы (*.dll / *.so*), плоско в каталог: CUDA runtime, cuDNN, cuFFT.
    Libs,
}

/// Обязательность компонента для запуска пайплайна.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Requirement {
    /// Без него analyze/render не работает — блокирует «первый запуск».
    Required,
    /// Улучшает результат, но пайплайн деградирует gracefully (напр. OCR-блюр).
    Recommended,
    /// Альтернативный вариант/квант (качается по выбору в настройках, не преселектится, не гейтит).
    Optional,
}

/// Как компонент попадает на диск.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    /// Качается сетью (есть file_specs с URL).
    Download,
    /// Идёт в комплекте релиза рядом с exe (VC++, OCR-модели). Не качаем; если нет — просим переустановить.
    Bundled,
    /// Часть системы, ставится вне приложения (драйвер NVIDIA). «Скачать» = открыть сайт.
    External,
}

/// Описание одного компонента манифеста.
#[derive(Clone, Debug)]
pub struct Component {
    pub id: &'static str,
    /// Человекочитаемое имя на языке окна.
    pub name: String,
    /// Назначение (что сломается без него) на языке окна.
    pub purpose: String,
    pub requirement: Requirement,
    pub delivery: Delivery,
    /// Совокупный размер компонента, байт (сумма файлов; для rolling-релизов — оценка).
    pub size: u64,
    /// Файлы к закачке (для Delivery::Download). Пусто у Bundled/External.
    pub files: &'static [FileSpec],
    /// Пути-«маркеры» (относительно repo_root): ключевые файлы, по которым видно компонент на диске. Для
    /// скачиваемого компонента установленность решают его files (см. `component_status`), маркеры —
    /// дополнительное условие и то, что снимает удаление старой установки без записи об архиве.
    pub markers: &'static [Marker],
    /// URL внешней страницы (для Delivery::External — сайт драйвера).
    pub external_url: Option<&'static str>,
}

/// Маркер наличия: путь + точный размер (0 = только существование).
#[derive(Clone, Copy, Debug)]
pub struct Marker {
    pub rel: &'static str,
    /// Точный размер (0 = не проверять).
    pub expect: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
//  URL-константы (источник истины; сверены HEAD-запросами, размеры = байт-в-байт с диском)
// ═══════════════════════════════════════════════════════════════════════════

// HF: Nemotron 3 Diarization (Streaming Sortformer v3, до 8 спикеров) — ONNX под parakeet-rs 0.3.8
// (altunenes/parakeet-rs, папка nemotron-3-diarization) + лицензия OpenMDW-1.1 рядом с моделью. Ревизия
// закреплена sha коммита, а не main: размеры ниже верны именно для неё.
const HF_NEMOTRON_DIAR: &str = "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx";
const HF_NEMOTRON_DIAR_LICENSE: &str = "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/LICENSE";
// HF: Mel-Band Roformer voc_fv6-Q8_0 (chenmozhijin/BSRoformer-GGUF).
const HF_ROFORMER: &str = "https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q8_0.gguf";
/// Файлы, маркеры и размер компонента, у которого своя сборка на каждую платформу.
pub struct Parts {
    pub size: u64,
    pub files: &'static [FileSpec],
    pub markers: &'static [Marker],
}

/// The archive of a build of the separation engine (dub_sep::asset) on a platform that has the build.
const fn sep_file(build: dub_sep::asset::Build) -> FileSpec {
    match build.engine() {
        Some(a) => FileSpec { url: a.url, dest_rel: a.archive, size: a.size, sha256: a.sha256, extract: Extract::Flat },
        None => panic!("this platform has no such build of the separation engine"),
    }
}

// Сборка llama.cpp одна на обе платформы.
const GH_LLAMA_BUILD: &str = "b11900";

/// The llama.cpp build of this computer's card, as YuE2 and Speech Studio pick it: CUDA 13, CUDA 12, Vulkan on another
/// card, the processor without one (the build «auto» of the engines, `acpp::build`).
pub fn llama_parts(build: crate::acpp::Build) -> &'static Parts {
    match build {
        crate::acpp::Build::Cuda13 => &platform::LLAMA,
        crate::acpp::Build::Cuda12 => &platform::LLAMA_CUDA12,
        crate::acpp::Build::Vulkan => &platform::LLAMA_VULKAN,
        crate::acpp::Build::Cpu | crate::acpp::Build::Metal => &platform::LLAMA_CPU,
    }
}

/// The oldest glibc the upstream llama.cpp CUDA builds for Linux start on (b11900 needs GLIBC_2.38 and GLIBCXX_3.4.32).
const LLAMA_CUDA_GLIBC: (u32, u32) = (2, 38);

/// The llama.cpp build Gemma runs on: the card's build, except a CUDA one on a Linux whose glibc is older than the
/// upstream CUDA builds need; the card then runs the Vulkan build.
pub fn llama_build() -> crate::acpp::Build {
    llama_build_for(crate::acpp::build(), glibc_version())
}

fn llama_build_for(card: crate::acpp::Build, glibc: Option<(u32, u32)>) -> crate::acpp::Build {
    use crate::acpp::Build;
    match (card, glibc) {
        (Build::Cuda13 | Build::Cuda12, Some(version)) if version < LLAMA_CUDA_GLIBC => Build::Vulkan,
        _ => card,
    }
}

/// Why Gemma does not run the card's CUDA build, for Settings; None when it does.
pub fn llama_note() -> Option<String> {
    let glibc = glibc_version()?;
    (llama_build() != crate::acpp::build()).then(|| {
        t!(
            "setup-llama-vulkan-glibc",
            glibc = format!("{}.{}", glibc.0, glibc.1),
            need = format!("{}.{}", LLAMA_CUDA_GLIBC.0, LLAMA_CUDA_GLIBC.1)
        )
    })
}

/// The C library version of this Linux (`gnu_get_libc_version`); None on other systems.
pub fn glibc_version() -> Option<(u32, u32)> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let text = unsafe { std::ffi::CStr::from_ptr(libc::gnu_get_libc_version()) };
        parse_glibc(text.to_str().ok()?)
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        None
    }
}

#[cfg(any(all(target_os = "linux", target_env = "gnu"), test))]
fn parse_glibc(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
    Some((major, minor))
}

/// The folder of a llama.cpp build: CUDA 13 in tools/llama, where former versions put it, the others beside it.
pub fn llama_dir(repo_root: &Path, build: crate::acpp::Build) -> PathBuf {
    let folder = match build {
        crate::acpp::Build::Cuda13 => "llama",
        crate::acpp::Build::Cuda12 => "llama-cuda12",
        crate::acpp::Build::Vulkan => "llama-vulkan",
        crate::acpp::Build::Cpu | crate::acpp::Build::Metal => "llama-cpu",
    };
    repo_root.join("tools").join(folder)
}

/// Windows x64: zip-релизы, PyPI-колёса win_amd64, redist NVIDIA windows-x86_64.
#[cfg(windows)]
mod platform {
    use super::{Extract, FileSpec, Marker, Parts};

    // GitHub: llama.cpp win-cuda-13.4 (ggml-org/llama.cpp, билд b11900).
    // ggml-cuda.dll импортирует cublas64_13.dll (ставит cuda-runtime), cudart слинкован статически.
    const GH_LLAMA: &str =
        "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-win-cuda-13.4-x64.zip";
    // GitHub: onnxruntime 1.28.2 win-x64 (microsoft/onnxruntime) — строго 1.28.x (ort rc.13 api-28; иначе дедлок).
    const GH_ORT: &str =
        "https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-1.28.2.zip";
    // GitHub: onnxruntime 1.28.2 GPU-сборка под CUDA 13 (gpu_cuda13) — CUDA-EP для Parakeet/Sortformer на
    // GPU. onnxruntime_providers_cuda.dll грузит cudart64_13/cublas64_13/cublasLt64_13 (cuda-runtime),
    // cudnn64_9 (WHEEL_CUDNN) и cufft64_12 (REDIST_CUFFT).
    // Содержит onnxruntime.dll(GPU) + onnxruntime_providers_cuda.dll + onnxruntime_providers_shared.dll.
    const GH_ORT_GPU: &str =
        "https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-win-x64-gpu_cuda13-1.28.2.zip";
    // GitHub: ffmpeg static win64 GPL (BtbN/FFmpeg-Builds), master-сборка последнего дня месяца: дневные
    // autobuild BtbN удаляет через пару недель, а сборки последнего дня месяца хранит, поэтому закреплена такая.
    const GH_FFMPEG: &str =
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-30-13-08/ffmpeg-N-127021-ge0c94b2d1c-win64-gpl.zip";
    // PyPI-wheel'ы NVIDIA CUDA 13 runtime (CUDA 13.4 Update 2: cudart 13.4.92, cuBLAS 13.8.0.4).
    // Дают cudart64_13.dll, cublas64_13.dll, cublasLt64_13.dll.
    const WHEEL_CUDART: &str = "https://files.pythonhosted.org/packages/86/00/d5436004268f049214193659ebc36550b5ef3925c3d13b4cc980e13be6f5/nvidia_cuda_runtime-13.4.92-py3-none-win_amd64.whl";
    const WHEEL_CUBLAS: &str = "https://files.pythonhosted.org/packages/a3/df/f1246959833e2c437db8be3e5b477f66b87f8817821ed40de6c7561c9a36/nvidia_cublas-13.8.0.4-py3-none-win_amd64.whl";
    // PyPI: cuDNN 9 под CUDA 13 (nvidia-cudnn-cu13 9.27.0.42) — нужен для CUDA-EP onnxruntime (Parakeet/
    // Sortformer на GPU). Даёт cudnn64_9.dll + split-либы. ≈416 МБ. cudart/cublas _13 уже есть (cuda-runtime выше).
    const WHEEL_CUDNN: &str = "https://files.pythonhosted.org/packages/87/6a/e55ff0ac26a5c6e2b21f41c9d04ad096b4ed6da593fba7e25845c61b0532/nvidia_cudnn_cu13-9.27.0.42-py3-none-win_amd64.whl";
    // NVIDIA CUDA-13 redist (официальный, CUDA 13.4 Update 2): cuFFT 12.4.0.43 → даёт cufft64_12.dll.
    // onnxruntime_providers_cuda.dll (сборка cuda13) грузит именно cufft64_12.dll — без него CUDA-EP не
    // грузится («CUDA not enabled»). cuFFT сохраняет soname 12 даже в CUDA 13. Извлекается как *.dll плоско.
    const REDIST_CUFFT: &str = "https://developer.download.nvidia.com/compute/cuda/redist/libcufft/windows-x86_64/libcufft-windows-x86_64-12.4.0.43-archive.zip";
    // GitHub: yt-dlp 2026.10.21 (yt-dlp/yt-dlp, SHA-256 из SHA2-256SUMS релиза) — загрузка видео по ссылке. Это опора:
    // более свежие версии ставит рядом ytdlp::update, при их провале работает эта.
    const GH_YTDLP: &str = "https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.21/yt-dlp.exe";
    // GitHub: deno 2.9.8 (denoland/deno) — JS-рантайм, без которого yt-dlp не решает задачи YouTube (yt-dlp-ejs уже внутри
    // yt-dlp.exe; вики yt-dlp EJS: deno не ниже 2.3.0).
    const GH_DENO: &str = "https://github.com/denoland/deno/releases/download/v2.9.8/deno-x86_64-pc-windows-msvc.zip";
    // transcribe.cpp (fork timoncool/transcribe.cpp, branch studio, scripts/build-engine-windows.cmd): libtranscribe and
    // ggml whole in one library. CUDA 13, sm 75/80/86/89/90/100/120 without PTX, cudart and cuBLAS from cuda-runtime;
    // the CPU build for a computer without a card CUDA 13 runs on. The pins are dub_tcpp's.
    const fn tcpp_file(build: dub_asr::tcpp::Build, dest_rel: &'static str) -> FileSpec {
        let d = build.download();
        FileSpec { url: d.url, dest_rel, size: d.size, sha256: d.sha256, extract: Extract::None }
    }
    pub const TRANSCRIBE_ENGINE: Parts = Parts {
        size: dub_asr::tcpp::Build::Cuda.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Cuda, "models/transcribe-engine/transcribe_engine.dll")],
        markers: &[Marker { rel: "models/transcribe-engine/transcribe_engine.dll", expect: dub_asr::tcpp::Build::Cuda.download().size }],
    };
    pub const TRANSCRIBE_ENGINE_CPU: Parts = Parts {
        size: dub_asr::tcpp::Build::Cpu.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Cpu, "models/transcribe-engine-cpu/transcribe_engine.dll")],
        markers: &[Marker { rel: "models/transcribe-engine-cpu/transcribe_engine.dll", expect: dub_asr::tcpp::Build::Cpu.download().size }],
    };
    // CUDA 12.9, sm 52-120a without PTX; cudart and cuBLAS from cuda12-runtime, whose folder is on PATH on such a card.
    pub const TRANSCRIBE_ENGINE_CUDA12: Parts = Parts {
        size: dub_asr::tcpp::Build::Cuda12.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Cuda12, "models/transcribe-engine-cuda12/transcribe_engine.dll")],
        markers: &[Marker { rel: "models/transcribe-engine-cuda12/transcribe_engine.dll", expect: dub_asr::tcpp::Build::Cuda12.download().size }],
    };
    pub const TRANSCRIBE_ENGINE_VULKAN: Parts = Parts {
        size: dub_asr::tcpp::Build::Vulkan.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Vulkan, "models/transcribe-engine-vulkan/transcribe_engine.dll")],
        markers: &[Marker { rel: "models/transcribe-engine-vulkan/transcribe_engine.dll", expect: dub_asr::tcpp::Build::Vulkan.download().size }],
    };
    // audiocpp_server (acpp_server::asset: fork timoncool/audio.cpp, branch studio; model specs inside, AVX2 floor, no
    // AVX-512) with the VC++ 14.51 libraries beside it. CUDA 13: SASS 7.5-12.0a without PTX, cudart inside, cuBLAS and
    // cuFFT from cuda-runtime. CUDA 12.9: SASS 5.2-12.0a without PTX, its libraries from cuda12-runtime.
    const fn server_file(asset: acpp_server::asset::Asset, archive: &'static str) -> FileSpec {
        FileSpec { url: asset.url, dest_rel: archive, size: asset.size, sha256: asset.sha256, extract: Extract::Flat }
    }
    const SERVER_CUDA13: Parts = Parts {
        size: acpp_server::asset::SERVER_WIN_CUDA13.size,
        files: &[server_file(acpp_server::asset::SERVER_WIN_CUDA13, "tools/audiocpp/_server.zip")],
        markers: &[Marker { rel: "tools/audiocpp/audiocpp_server.exe", expect: 0 }],
    };
    const SERVER_CUDA12: Parts = Parts {
        size: acpp_server::asset::SERVER_WIN_CUDA12.size,
        files: &[server_file(acpp_server::asset::SERVER_WIN_CUDA12, "tools/audiocpp-cuda12/_server.zip")],
        markers: &[Marker { rel: "tools/audiocpp-cuda12/audiocpp_server.exe", expect: 0 }],
    };
    const SERVER_VULKAN: Parts = Parts {
        size: acpp_server::asset::SERVER_WIN_VULKAN.size,
        files: &[server_file(acpp_server::asset::SERVER_WIN_VULKAN, "tools/audiocpp-vulkan/_server.zip")],
        markers: &[Marker { rel: "tools/audiocpp-vulkan/audiocpp_server.exe", expect: 0 }],
    };
    const SERVER_CPU: Parts = Parts {
        size: acpp_server::asset::SERVER_WIN_CPU.size,
        files: &[server_file(acpp_server::asset::SERVER_WIN_CPU, "tools/audiocpp-cpu/_server.zip")],
        markers: &[Marker { rel: "tools/audiocpp-cpu/audiocpp_server.exe", expect: 0 }],
    };

    /// The download of a build of the server.
    pub fn server_parts(build: acpp_server::asset::Build) -> &'static Parts {
        match build {
            acpp_server::asset::Build::Cuda13 => &SERVER_CUDA13,
            acpp_server::asset::Build::Cuda12 => &SERVER_CUDA12,
            acpp_server::asset::Build::Vulkan => &SERVER_VULKAN,
            acpp_server::asset::Build::Cpu => &SERVER_CPU,
            acpp_server::asset::Build::Metal => &Parts { size: 0, files: &[], markers: &[] },
        }
    }

    // NVIDIA CUDA 12.9 runtime for the CUDA 12.9 builds (PyPI wheels: cudart 12.9.79, cuBLAS 12.9.2.10, cuFFT
    // 11.4.1.4): cudart64_12, cublas64_12, cublasLt64_12, cufft64_11, in their own folder, on PATH only for a card that
    // takes the CUDA 12 builds.
    pub const CUDA12_RUNTIME: Parts = Parts {
        size: 756_821_809,
        files: &[
            FileSpec { url: "https://files.pythonhosted.org/packages/59/df/e7c3a360be4f7b93cee39271b792669baeb3846c58a4df6dfcf187a7ffab/nvidia_cuda_runtime_cu12-12.9.79-py3-none-win_amd64.whl", dest_rel: "models/cuda12-runtime/_cudart.whl", size: 3_591_604, sha256: "8e018af8fa02363876860388bd10ccb89eb9ab8fb0aa749aaf58430a9f7c4891", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/20/e2/fc9a0e985249d873150276d5afb02e39a66817fedbf1a385724393e505ed/nvidia_cublas_cu12-12.9.2.10-py3-none-win_amd64.whl", dest_rel: "models/cuda12-runtime/_cublas.whl", size: 553_162_896, sha256: "623f43027d40d44ceadf0043f002bd25cf353e8f13ce90b9a87057019f560661", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/20/ee/29955203338515b940bd4f60ffdbc073428f25ef9bfbce44c9a066aedc5c/nvidia_cufft_cu12-11.4.1.4-py3-none-win_amd64.whl", dest_rel: "models/cuda12-runtime/_cufft.whl", size: 200_067_309, sha256: "8e5bfaac795e93f80611f807d42844e8e27e340e0cde270dcb6c65386d795b80", extract: Extract::Libs },
        ],
        markers: &[
            Marker { rel: "models/cuda12-runtime/cudart64_12.dll", expect: 0 },
            Marker { rel: "models/cuda12-runtime/cublas64_12.dll", expect: 0 },
            Marker { rel: "models/cuda12-runtime/cublasLt64_12.dll", expect: 0 },
            Marker { rel: "models/cuda12-runtime/cufft64_11.dll", expect: 0 },
        ],
    };

    // Движок Drama 3 (C ABI форка timoncool/drama-rs, capi), CUDA 13.3, sm 75/80/86/89/90/100/120 без PTX:
    // импортирует cublas64_13.dll (ставит cuda-runtime), cudart слинкован статически.
    pub const DRAMA_ENGINE: FileSpec = FileSpec { url: "https://github.com/timoncool/drama-rs/releases/download/engine-0.1.3/drama_engine.dll", dest_rel: "models/drama-engine/drama_engine.dll", size: 32_075_264, sha256: "677133f63605d72e3eceb148eb1fd3eee72f0aae02b65750da59bf60d041793f", extract: Extract::None };

    // Qwen3-TTS (timoncool/qwentts.cpp, branch studio, ABI 7, scripts/build-engine-windows.cmd): qwen.dll with ggml and
    // its CPU backend, and the backend module of each other device beside it (GGML_BACKEND_DL), which the library loads
    // when a model first runs on that device. CUDA 13: sm 75/80/86/89/90/100/120; CUDA 12.9: sm 52-90 and 120; no PTX;
    // cuBLAS comes from cuda-runtime or cuda12-runtime.
    pub const QWEN_ENGINE: Parts = Parts {
        size: super::QWEN_CODEC.size + 848_126,
        files: &[super::QWEN_CODEC, FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/qwen-windows-x64.zip", dest_rel: "models/qwen3-tts/_engine.zip", size: 848_126, sha256: "adbc02c0a1e3e4bcb800287ecdf0f706515b9a1bcbdf28c8d14b1e23b56a749d", extract: Extract::Flat }],
        markers: &[Marker { rel: super::QWEN_CODEC.dest_rel, expect: super::QWEN_CODEC.size }, Marker { rel: "models/qwen3-tts/qwen.dll", expect: 433_152 }, Marker { rel: "models/qwen3-tts/ggml-cpu.dll", expect: 983_040 }],
    };
    pub const QWEN_VULKAN: Parts = Parts {
        size: 41_529_856,
        files: &[FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/ggml-vulkan-windows-x64.dll", dest_rel: "models/qwen3-tts/ggml-vulkan.dll", size: 41_529_856, sha256: "a0d2ee540061584bdf57448de57a87257719307a29501e95c30d33fd0327535d", extract: Extract::None }],
        markers: &[Marker { rel: "models/qwen3-tts/ggml-vulkan.dll", expect: 41_529_856 }],
    };
    pub const QWEN_CUDA13: Parts = Parts {
        size: 142_189_568,
        files: &[FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/ggml-cuda13-windows-x64.dll", dest_rel: "models/qwen3-tts/ggml-cuda.dll", size: 142_189_568, sha256: "bedda4c6bf25d2375d44ec63fe83d0088bd62f28be166784d657038457474d70", extract: Extract::None }],
        markers: &[Marker { rel: "models/qwen3-tts/ggml-cuda.dll", expect: 142_189_568 }],
    };
    pub const QWEN_CUDA12: Parts = Parts {
        size: 170_425_344,
        files: &[FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/ggml-cuda12-windows-x64.dll", dest_rel: "models/qwen3-tts/ggml-cuda.dll", size: 170_425_344, sha256: "c59460d1c50f7295fb56fb70d5847ea9de8697b58b6934afa7aeb69087967ed8", extract: Extract::None }],
        markers: &[Marker { rel: "models/qwen3-tts/ggml-cuda.dll", expect: 170_425_344 }],
    };

    // BSRoformer.cpp v0.1.0, the pins of dub_sep::asset: CUDA 13 and 12.9 the studios' builds (cuBLAS from cuda-runtime /
    // cuda12-runtime), Vulkan and the processor the author's (the processor's static, its VC++ runtime from vcruntime).
    const SEP_CUDA13: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cuda13)];
    const SEP_CUDA12: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cuda12)];
    const SEP_VULKAN: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Vulkan)];
    const SEP_CPU: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cpu)];
    pub const BSROFORMER: Parts = Parts { size: SEP_CUDA13[0].size, files: &SEP_CUDA13, markers: &[Marker { rel: "tools/bsroformer/bs_roformer-cli.exe", expect: 0 }] };
    pub const BSROFORMER_CUDA12: Parts = Parts { size: SEP_CUDA12[0].size, files: &SEP_CUDA12, markers: &[Marker { rel: "tools/bsroformer-cuda12/bs_roformer-cli.exe", expect: 0 }] };
    pub const BSROFORMER_VULKAN: Parts = Parts { size: SEP_VULKAN[0].size, files: &SEP_VULKAN, markers: &[Marker { rel: "tools/bsroformer-vulkan/bs_roformer-cli.exe", expect: 0 }] };
    pub const BSROFORMER_CPU: Parts = Parts { size: SEP_CPU[0].size, files: &SEP_CPU, markers: &[Marker { rel: "tools/bsroformer-cpu/bs_roformer-cli.exe", expect: 0 }] };

    pub const LLAMA: Parts = Parts {
        size: 164_721_212,
        files: &[
            FileSpec { url: GH_LLAMA, dest_rel: "tools/llama/_llama.zip", size: 164_721_212, sha256: "0d75ad36367da00622c146a97e82fac3dcfcbe960afc2f0234eef026886b16f6", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/llama/llama-server.exe", expect: 0 }],
    };
    // the same release's other builds: CUDA 12.4 (cudart and cuBLAS 12 from cuda12-runtime), Vulkan and the processor
    pub const LLAMA_CUDA12: Parts = Parts {
        size: 279_417_051,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-win-cuda-12.4-x64.zip", dest_rel: "tools/llama-cuda12/_llama.zip", size: 279_417_051, sha256: "387ad76c926cdb301971851a11df018165a7b4df4b1b7c64cf4b09a367ec82bc", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-cuda12/llama-server.exe", expect: 0 }],
    };
    pub const LLAMA_VULKAN: Parts = Parts {
        size: 37_051_563,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-win-vulkan-x64.zip", dest_rel: "tools/llama-vulkan/_llama.zip", size: 37_051_563, sha256: "baf0183b1906d7cc0cfe9a0bd3fae34ceb4048acecc371ff3b7a91456783d051", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-vulkan/llama-server.exe", expect: 0 }],
    };
    pub const LLAMA_CPU: Parts = Parts {
        size: 24_228_419,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-win-cpu-x64.zip", dest_rel: "tools/llama-cpu/_llama.zip", size: 24_228_419, sha256: "d3bbbbb7d1b4979b72052fd73cdeeb5a4d5a4e6a3f52aeb40abd6f116558e323", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-cpu/llama-server.exe", expect: 0 }],
    };

    pub const ORT: Parts = Parts {
        size: 78_620_837,
        files: &[
            FileSpec { url: GH_ORT, dest_rel: "models/runtime/_ort.zip", size: 78_620_837, sha256: "c4eedd29489d5feca21866d054638416f3655bf6b18851b3b6b85c8313e95c35", extract: Extract::Tree },
        ],
        // dub-asr::ensure_ort_dylib ищет ровно этот путь под models/runtime.
        markers: &[Marker { rel: "models/runtime/onnxruntime-win-x64-1.28.2/lib/onnxruntime.dll", expect: 0 }],
    };

    pub const ORT_GPU: Parts = Parts {
        size: 365_562_963,
        files: &[
            FileSpec { url: GH_ORT_GPU, dest_rel: "models/runtime/_ort_gpu.zip", size: 365_562_963, sha256: "4b7a2d01a3cc96b12d06c8266af2c8f42c96365c4a0100d45fd874c71b4a2e19", extract: Extract::Tree },
        ],
        markers: &[Marker { rel: "models/runtime/onnxruntime-win-x64-gpu_cuda13-1.28.2/lib/onnxruntime.dll", expect: 0 }],
    };

    pub const FFMPEG: Parts = Parts {
        size: 198_589_424,
        files: &[
            FileSpec { url: GH_FFMPEG, dest_rel: "tools/ffmpeg/_ffmpeg.zip", size: 198_589_424, sha256: "85c4a4636b93d681a07588ffa6f1d8f5c064d9c3fb0622066662906b6fd718a1", extract: Extract::Pick },
        ],
        markers: &[Marker { rel: "tools/ffmpeg/ffmpeg.exe", expect: 0 }, Marker { rel: "tools/ffmpeg/ffprobe.exe", expect: 0 }],
    };

    pub const YTDLP: Parts = Parts {
        size: 60_797_583,
        files: &[
            FileSpec { url: GH_YTDLP, dest_rel: "tools/yt-dlp/yt-dlp.exe", size: 17_880_111, sha256: "33121d192d706e62f684a06f80918e3f3a8ba52ac3a65213b6951c39c7d9f555", extract: Extract::None },
            FileSpec { url: GH_DENO, dest_rel: "tools/yt-dlp/_deno.zip", size: 42_917_472, sha256: "cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf", extract: Extract::Flat },
        ],
        markers: &[
            Marker { rel: "tools/yt-dlp/yt-dlp.exe", expect: 17_880_111 },
            Marker { rel: "tools/yt-dlp/deno.exe", expect: 98_081_568 },
        ],
    };

    pub const CUDA_RUNTIME: Parts = Parts {
        size: 585_648_133,
        files: &[
            FileSpec { url: WHEEL_CUDART, dest_rel: "models/higgs-engine/_cudart.whl", size: 2_778_543, sha256: "08dca5e4aba480c2fd5b55075c0fa71b84ef9dcf0521f2d58baa14a803a7311c", extract: Extract::Libs },
            FileSpec { url: WHEEL_CUBLAS, dest_rel: "models/higgs-engine/_cublas.whl", size: 423_266_897, sha256: "8c5494423bb8a46822cb6b0cb95d7fa4be2d7b96a31155dff083839ec8297910", extract: Extract::Libs },
            // cuFFT (cufft64_12.dll) — обязателен для CUDA-EP onnxruntime (диаризация/Parakeet на GPU).
            FileSpec { url: REDIST_CUFFT, dest_rel: "models/higgs-engine/_cufft.zip", size: 159_602_693, sha256: "69d0ad8dc3a1be66f01a748a8206d0ceaafa56939474663fbe790dc3e91d2009", extract: Extract::Libs },
        ],
        markers: &[
            Marker { rel: "models/higgs-engine/cudart64_13.dll", expect: 0 },
            Marker { rel: "models/higgs-engine/cublas64_13.dll", expect: 0 },
            Marker { rel: "models/higgs-engine/cublasLt64_13.dll", expect: 0 },
            Marker { rel: "models/higgs-engine/cufft64_12.dll", expect: 0 },
        ],
    };

    pub const CUDNN: Parts = Parts {
        size: 436_469_905,
        files: &[
            FileSpec { url: WHEEL_CUDNN, dest_rel: "models/higgs-engine/_cudnn.whl", size: 436_469_905, sha256: "7d96f634adafd55c72231eb0500ca77ab109ec8ebff7b33000b76e081bc4558e", extract: Extract::Libs },
        ],
        markers: &[Marker { rel: "models/higgs-engine/cudnn64_9.dll", expect: 0 }],
    };

}

/// Linux x86-64: tar-сборки GitHub, PyPI-колёса manylinux, redist NVIDIA linux-x86_64. Библиотеки CUDA ищутся
/// через LD_LIBRARY_PATH, который ставит `ensure_library_path` на старте студии.
#[cfg(target_os = "linux")]
mod platform {
    use super::{Extract, FileSpec, Marker, Parts};

    // transcribe.cpp (fork timoncool/transcribe.cpp, scripts/build-engine-linux-docker.sh: Ubuntu 22.04). CUDA 13.3,
    // sm 75/80/86/89/90/100/120, RUNPATH $ORIGIN and ../higgs-engine, where libcudart and libcublas come from
    // cuda-runtime; the CPU build for a computer without a card CUDA 13 runs on. The pins are dub_tcpp's.
    const fn tcpp_file(build: dub_asr::tcpp::Build, dest_rel: &'static str) -> FileSpec {
        let d = build.download();
        FileSpec { url: d.url, dest_rel, size: d.size, sha256: d.sha256, extract: Extract::None }
    }
    const TCPP_CUDA: &str = "models/transcribe-engine/libtranscribe_engine.so";
    const TCPP_CPU: &str = "models/transcribe-engine-cpu/libtranscribe_engine.so";
    const TCPP_CUDA12: &str = "models/transcribe-engine-cuda12/libtranscribe_engine.so";
    pub const TRANSCRIBE_ENGINE: Parts = Parts {
        size: dub_asr::tcpp::Build::Cuda.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Cuda, TCPP_CUDA)],
        markers: &[Marker { rel: TCPP_CUDA, expect: dub_asr::tcpp::Build::Cuda.download().size }],
    };
    pub const TRANSCRIBE_ENGINE_CPU: Parts = Parts {
        size: dub_asr::tcpp::Build::Cpu.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Cpu, TCPP_CPU)],
        markers: &[Marker { rel: TCPP_CPU, expect: dub_asr::tcpp::Build::Cpu.download().size }],
    };
    // CUDA 12.9 with RUNPATH $ORIGIN and ../cuda12-runtime, where cuda12-runtime puts libcudart and libcublas.
    pub const TRANSCRIBE_ENGINE_CUDA12: Parts = Parts {
        size: dub_asr::tcpp::Build::Cuda12.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Cuda12, TCPP_CUDA12)],
        markers: &[Marker { rel: TCPP_CUDA12, expect: dub_asr::tcpp::Build::Cuda12.download().size }],
    };
    pub const TRANSCRIBE_ENGINE_VULKAN: Parts = Parts {
        size: dub_asr::tcpp::Build::Vulkan.download().size,
        files: &[tcpp_file(dub_asr::tcpp::Build::Vulkan, "models/transcribe-engine-vulkan/libtranscribe_engine.so")],
        markers: &[Marker { rel: "models/transcribe-engine-vulkan/libtranscribe_engine.so", expect: dub_asr::tcpp::Build::Vulkan.download().size }],
    };
    // audiocpp_server (acpp_server::asset): one archive a build with libgomp beside the program; RUNPATH $ORIGIN,
    // ../../models/higgs-engine (CUDA 13 runtime of cuda-runtime) and ../../models/cuda12-runtime (CUDA 12.9).
    const fn server_file(asset: acpp_server::asset::Asset, archive: &'static str) -> FileSpec {
        FileSpec { url: asset.url, dest_rel: archive, size: asset.size, sha256: asset.sha256, extract: Extract::Flat }
    }
    const SERVER_CUDA13: Parts = Parts {
        size: acpp_server::asset::SERVER_LINUX_CUDA13.size,
        files: &[server_file(acpp_server::asset::SERVER_LINUX_CUDA13, "tools/audiocpp/_server.tar.gz")],
        markers: &[Marker { rel: "tools/audiocpp/audiocpp_server", expect: 0 }],
    };
    const SERVER_CUDA12: Parts = Parts {
        size: acpp_server::asset::SERVER_LINUX_CUDA12.size,
        files: &[server_file(acpp_server::asset::SERVER_LINUX_CUDA12, "tools/audiocpp-cuda12/_server.tar.gz")],
        markers: &[Marker { rel: "tools/audiocpp-cuda12/audiocpp_server", expect: 0 }],
    };
    const SERVER_VULKAN: Parts = Parts {
        size: acpp_server::asset::SERVER_LINUX_VULKAN.size,
        files: &[server_file(acpp_server::asset::SERVER_LINUX_VULKAN, "tools/audiocpp-vulkan/_server.tar.gz")],
        markers: &[Marker { rel: "tools/audiocpp-vulkan/audiocpp_server", expect: 0 }],
    };
    const SERVER_CPU: Parts = Parts {
        size: acpp_server::asset::SERVER_LINUX_CPU.size,
        files: &[server_file(acpp_server::asset::SERVER_LINUX_CPU, "tools/audiocpp-cpu/_server.tar.gz")],
        markers: &[Marker { rel: "tools/audiocpp-cpu/audiocpp_server", expect: 0 }],
    };

    /// The download of a build of the server.
    pub fn server_parts(build: acpp_server::asset::Build) -> &'static Parts {
        match build {
            acpp_server::asset::Build::Cuda13 => &SERVER_CUDA13,
            acpp_server::asset::Build::Cuda12 => &SERVER_CUDA12,
            acpp_server::asset::Build::Vulkan => &SERVER_VULKAN,
            acpp_server::asset::Build::Cpu => &SERVER_CPU,
            acpp_server::asset::Build::Metal => &Parts { size: 0, files: &[], markers: &[] },
        }
    }

    // CUDA 12.9 runtime for the CUDA 12.9 build (manylinux wheels of the Windows component's versions: cudart 12.9.79,
    // cuBLAS 12.9.2.10, cuFFT 11.4.1.4), in their own folder the build's RUNPATH names.
    pub const CUDA12_RUNTIME: Parts = Parts {
        size: 785_610_881,
        files: &[
            FileSpec { url: "https://files.pythonhosted.org/packages/bc/46/a92db19b8309581092a3add7e6fceb4c301a3fd233969856a8cbf042cd3c/nvidia_cuda_runtime_cu12-12.9.79-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl", dest_rel: "models/cuda12-runtime/_cudart.whl", size: 3_493_179, sha256: "25bba2dfb01d48a9b59ca474a1ac43c6ebf7011f1b0b8cc44f54eb6ac48a96c3", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/cb/c0/0a517bfe63ccd3b92eb254d264e28fca3c7cab75d07daea315250fb1bf73/nvidia_cublas_cu12-12.9.2.10-py3-none-manylinux_2_27_x86_64.whl", dest_rel: "models/cuda12-runtime/_cublas.whl", size: 581_240_110, sha256: "e4f53a8ca8c5d6e8c492d0d0a3d565ecb59a751b19cfdaa4f6da0ab2104c1702", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/95/f4/61e6996dd20481ee834f57a8e9dca28b1869366a135e0d42e2aa8493bdd4/nvidia_cufft_cu12-11.4.1.4-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl", dest_rel: "models/cuda12-runtime/_cufft.whl", size: 200_877_592, sha256: "c67884f2a7d276b4b80eb56a79322a95df592ae5e765cf1243693365ccab4e28", extract: Extract::Libs },
        ],
        markers: &[
            Marker { rel: "models/cuda12-runtime/libcudart.so.12", expect: 0 },
            Marker { rel: "models/cuda12-runtime/libcublas.so.12", expect: 0 },
            Marker { rel: "models/cuda12-runtime/libcublasLt.so.12", expect: 0 },
            Marker { rel: "models/cuda12-runtime/libcufft.so.11", expect: 0 },
        ],
    };

    // Движок Drama 3 (форк timoncool/drama-rs, capi/build-linux-docker.sh: CUDA 13.3, Ubuntu 22.04, sm 75/80/86/89/90/
    // 100/120): RUNPATH $ORIGIN и ../higgs-engine, где libcudart и libcublas ставит cuda-runtime.
    pub const DRAMA_ENGINE: FileSpec = FileSpec { url: "https://github.com/timoncool/drama-rs/releases/download/engine-0.1.3/libdrama_engine.so", dest_rel: "models/drama-engine/libdrama_engine.so", size: 32_642_032, sha256: "b9beb23321fd63025facb2fa1d90087d3dd03069475aa60ad60c3fb3c97892b4", extract: Extract::None };

    // Qwen3-TTS (timoncool/qwentts.cpp, branch studio, ABI 7, scripts/build-engine-linux-docker.sh, Ubuntu 22.04):
    // libqwen.so with ggml, its CPU backend and libgomp, and the backend module of each other device beside it
    // (GGML_BACKEND_DL); RUNPATH $ORIGIN, ../higgs-engine (CUDA 13 runtime) and ../cuda12-runtime.
    pub const QWEN_ENGINE: Parts = Parts {
        size: super::QWEN_CODEC.size + 1_235_196,
        files: &[super::QWEN_CODEC, FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/qwen-linux-x64.tar.gz", dest_rel: "models/qwen3-tts/_engine.tar.gz", size: 1_235_196, sha256: "a2a5a4d355f8e4e58e87130f075c035aa0c62d0e43cec31da7c0c87c22880e8a", extract: Extract::Flat }],
        markers: &[Marker { rel: super::QWEN_CODEC.dest_rel, expect: super::QWEN_CODEC.size }, Marker { rel: "models/qwen3-tts/libqwen.so", expect: 675_808 }, Marker { rel: "models/qwen3-tts/libggml-cpu.so", expect: 1_272_208 }],
    };
    pub const QWEN_VULKAN: Parts = Parts {
        size: 42_242_640,
        files: &[FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/libggml-vulkan-linux-x64.so", dest_rel: "models/qwen3-tts/libggml-vulkan.so", size: 42_242_640, sha256: "efce535b0ba4a3d7803ab2ec75f0a1a06693fb44699baebbdd61dbc032c2177d", extract: Extract::None }],
        markers: &[Marker { rel: "models/qwen3-tts/libggml-vulkan.so", expect: 42_242_640 }],
    };
    pub const QWEN_CUDA13: Parts = Parts {
        size: 157_457_144,
        files: &[FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/libggml-cuda13-linux-x64.so", dest_rel: "models/qwen3-tts/libggml-cuda.so", size: 157_457_144, sha256: "ded156c13202ed4a27647eccde8ec7e30b139e5e948f8d98a9f67f0d8494d6eb", extract: Extract::None }],
        markers: &[Marker { rel: "models/qwen3-tts/libggml-cuda.so", expect: 157_457_144 }],
    };
    pub const QWEN_CUDA12: Parts = Parts {
        size: 181_025_560,
        files: &[FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.0/libggml-cuda12-linux-x64.so", dest_rel: "models/qwen3-tts/libggml-cuda.so", size: 181_025_560, sha256: "bb4123ff8521c38ecf7704e71d2ee2814ae21704d44240b9489033ce56474837", extract: Extract::None }],
        markers: &[Marker { rel: "models/qwen3-tts/libggml-cuda.so", expect: 181_025_560 }],
    };

    // BSRoformer.cpp v0.1.0, the pins of dub_sep::asset: the studios' CUDA 13 and 12.9 builds (Ubuntu 22.04, libgomp
    // beside the program, RUNPATH $ORIGIN, ../../models/higgs-engine and ../../models/cuda12-runtime), Vulkan and the
    // processor the author's.
    const SEP_CUDA13: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cuda13)];
    const SEP_CUDA12: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cuda12)];
    const SEP_VULKAN: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Vulkan)];
    const SEP_CPU: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cpu)];
    pub const BSROFORMER: Parts = Parts { size: SEP_CUDA13[0].size, files: &SEP_CUDA13, markers: &[Marker { rel: "tools/bsroformer/bs_roformer-cli", expect: 0 }] };
    pub const BSROFORMER_CUDA12: Parts = Parts { size: SEP_CUDA12[0].size, files: &SEP_CUDA12, markers: &[Marker { rel: "tools/bsroformer-cuda12/bs_roformer-cli", expect: 0 }] };
    pub const BSROFORMER_VULKAN: Parts = Parts { size: SEP_VULKAN[0].size, files: &SEP_VULKAN, markers: &[Marker { rel: "tools/bsroformer-vulkan/bs_roformer-cli", expect: 0 }] };
    pub const BSROFORMER_CPU: Parts = Parts { size: SEP_CPU[0].size, files: &SEP_CPU, markers: &[Marker { rel: "tools/bsroformer-cpu/bs_roformer-cli", expect: 0 }] };

    // llama.cpp b11900 ubuntu-cuda-13.4: RUNPATH $ORIGIN для своих библиотек, cudart/cuBLAS 13 — из cuda-runtime,
    // libssl.so.3 — системная.
    pub const LLAMA: Parts = Parts {
        size: 163_678_003,
        files: &[
            FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-ubuntu-cuda-13.4-x64.tar.gz", dest_rel: "tools/llama/_llama.tar.gz", size: 163_678_003, sha256: "925078bca4277d495e7fc07cfd20aa1a9ed57640b218bce3ad49de9bcc1c5799", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/llama/llama-server", expect: 0 }],
    };
    // the same release's other builds: CUDA 12.8 (cudart and cuBLAS 12 from cuda12-runtime), Vulkan and the processor
    pub const LLAMA_CUDA12: Parts = Parts {
        size: 182_211_176,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-ubuntu-cuda-12.8-x64.tar.gz", dest_rel: "tools/llama-cuda12/_llama.tar.gz", size: 182_211_176, sha256: "618c17e4e4ccc11953f21452ca34e4786a1554dc405e98a2c4d53d7c2ed5b415", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-cuda12/llama-server", expect: 0 }],
    };
    pub const LLAMA_VULKAN: Parts = Parts {
        size: 35_162_561,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-ubuntu-vulkan-x64.tar.gz", dest_rel: "tools/llama-vulkan/_llama.tar.gz", size: 35_162_561, sha256: "111ee8d3f61723859e57195bb9ba21241eb598163621840aaf3b5c066950c092", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-vulkan/llama-server", expect: 0 }],
    };
    pub const LLAMA_CPU: Parts = Parts {
        size: 22_497_562,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-ubuntu-x64.tar.gz", dest_rel: "tools/llama-cpu/_llama.tar.gz", size: 22_497_562, sha256: "3effcc3009af64d6f30c217827d7982b3934577d282feb243b11fb6789ed124e", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-cpu/llama-server", expect: 0 }],
    };

    pub const ORT: Parts = Parts {
        size: 9_128_991,
        files: &[
            FileSpec { url: "https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-linux-x64-1.28.2.tgz", dest_rel: "models/runtime/_ort.tgz", size: 9_128_991, sha256: "d7209b8751b27b862b0c76332c2e20e203396edb5dab700ecf4bb485cf147415", extract: Extract::Tree },
        ],
        markers: &[Marker { rel: "models/runtime/onnxruntime-linux-x64-1.28.2/lib/libonnxruntime.so.1.28.2", expect: 0 }],
    };

    // CUDA-провайдер на Linux связан с cuBLAS/cuBLASLt/cuRAND/cudart 13 (cuda-runtime) и без RUNPATH, cuDNN и
    // cuFFT грузит сам во время работы.
    pub const ORT_GPU: Parts = Parts {
        size: 240_868_705,
        files: &[
            FileSpec { url: "https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-linux-x64-gpu_cuda13-1.28.2.tgz", dest_rel: "models/runtime/_ort_gpu.tgz", size: 240_868_705, sha256: "118ca8dbc4e4bb9b3b7fea137d796a89d957c9aa70e1dc3a5199a302cdd5bb32", extract: Extract::Tree },
        ],
        markers: &[Marker { rel: "models/runtime/onnxruntime-linux-x64-gpu_cuda13-1.28.2/lib/libonnxruntime.so.1.28.2", expect: 0 }],
    };

    // ffmpeg static linux64 GPL (BtbN), та же сборка последнего дня месяца, что на Windows.
    pub const FFMPEG: Parts = Parts {
        size: 154_317_776,
        files: &[
            FileSpec { url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-30-13-08/ffmpeg-N-127021-ge0c94b2d1c-linux64-gpl.tar.xz", dest_rel: "tools/ffmpeg/_ffmpeg.tar.xz", size: 154_317_776, sha256: "3a2a94e704752287833604501a79505c2319ad1d8b2f5577c63f2b1b7a6f2b66", extract: Extract::Pick },
        ],
        markers: &[Marker { rel: "tools/ffmpeg/ffmpeg", expect: 0 }, Marker { rel: "tools/ffmpeg/ffprobe", expect: 0 }],
    };

    pub const YTDLP: Parts = Parts {
        size: 82_532_144,
        files: &[
            FileSpec { url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.21/yt-dlp_linux", dest_rel: "tools/yt-dlp/yt-dlp", size: 40_569_080, sha256: "d8bd33fca9d5b23bd5b9952b3fca36db5c46331933fda9f598a3da14ce2c243f", extract: Extract::None },
            FileSpec { url: "https://github.com/denoland/deno/releases/download/v2.9.8/deno-x86_64-unknown-linux-gnu.zip", dest_rel: "tools/yt-dlp/_deno.zip", size: 41_963_064, sha256: "e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609", extract: Extract::Flat },
        ],
        markers: &[
            Marker { rel: "tools/yt-dlp/yt-dlp", expect: 40_569_080 },
            Marker { rel: "tools/yt-dlp/deno", expect: 96_055_992 },
        ],
    };

    // Колёса manylinux тех же версий, что win_amd64 (cudart 13.4.92, cuBLAS 13.8.0.4, cuFFT 12.4.0.43) и cuRAND
    // 10.4.4.72 (CUDA 13.4 Update 2): CUDA-провайдер onnxruntime на Linux связан с libcurand.so.10.
    pub const CUDA_RUNTIME: Parts = Parts {
        size: 665_060_003,
        files: &[
            FileSpec { url: "https://files.pythonhosted.org/packages/98/8a/3431271f6344874b8f1ac03f16b3d679c91493f8da63f716160403e6d0a0/nvidia_cuda_runtime-13.4.92-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl", dest_rel: "models/higgs-engine/_cudart.whl", size: 2_494_438, sha256: "9641f797da20ce1dd8e779b6e96d08cf9ba564cec8e8225458811ee26423f3a5", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/7a/38/bdd540bf511d2c9b6f9efc71a81c60cb88e295be0b9312b61d19bbed2212/nvidia_cublas-13.8.0.4-py3-none-manylinux_2_27_x86_64.whl", dest_rel: "models/higgs-engine/_cublas.whl", size: 439_317_144, sha256: "9f17797dfcc048694461f4e47de17d2e3c25adf172ef723d2db0a07cd8744b89", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/76/bf/3fea3d1c6262bded26ae00e3106432d63235954965d7785f5051ac146651/nvidia_cufft-12.4.0.43-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl", dest_rel: "models/higgs-engine/_cufft.whl", size: 161_750_089, sha256: "0e8385013596b112d29c9ce8c63dc575b308d77636c7169104e18714f03961a8", extract: Extract::Libs },
            FileSpec { url: "https://files.pythonhosted.org/packages/07/73/3ee8e5b4cb891401e603ffd3a59b35c6afe785fd2de123afe7c7029603dc/nvidia_curand-10.4.4.72-py3-none-manylinux_2_27_x86_64.whl", dest_rel: "models/higgs-engine/_curand.whl", size: 61_498_332, sha256: "25c3457ae7a224fdd484dab90b0fc5dc0e842fab5db3012afa4a5bd2af4eb7e5", extract: Extract::Libs },
        ],
        markers: &[
            Marker { rel: "models/higgs-engine/libcudart.so.13", expect: 0 },
            Marker { rel: "models/higgs-engine/libcublas.so.13", expect: 0 },
            Marker { rel: "models/higgs-engine/libcublasLt.so.13", expect: 0 },
            Marker { rel: "models/higgs-engine/libcufft.so.12", expect: 0 },
            Marker { rel: "models/higgs-engine/libcurand.so.10", expect: 0 },
        ],
    };

    pub const CUDNN: Parts = Parts {
        size: 536_771_498,
        files: &[
            FileSpec { url: "https://files.pythonhosted.org/packages/af/75/96ea5c5368eb595c39d629cde08a66227a864e71ccb0e593add2612bb952/nvidia_cudnn_cu13-9.27.0.42-py3-none-manylinux_2_27_x86_64.whl", dest_rel: "models/higgs-engine/_cudnn.whl", size: 536_771_498, sha256: "9677e76f21862eb5da7ee5ed69d544738b2d8b5c3ce7e5ec125c5592e6cdbdc8", extract: Extract::Libs },
        ],
        markers: &[Marker { rel: "models/higgs-engine/libcudnn.so.9", expect: 0 }],
    };
}

#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
compile_error!("Dub Studio for macOS is built for Apple silicon only: ONNX Runtime 1.28.2 has no Intel build");

/// macOS on Apple silicon: one build of each engine, Metal and the processor in it, in the processor build's folders
/// (a stage set to the processor or to «auto» runs it); no CUDA, no Vulkan, no Drama 3 and no Qwen3-TTS engine. The pins
/// are Speech Studio's.
#[cfg(target_os = "macos")]
mod platform {
    use super::{Extract, FileSpec, Marker, Parts};

    const NONE: Parts = Parts { size: 0, files: &[], markers: &[] };

    // transcribe.cpp (dub_tcpp's pin): Metal and the processor in one library, in the processor build's folder.
    pub const TRANSCRIBE_ENGINE: Parts = NONE;
    pub const TRANSCRIBE_ENGINE_CUDA12: Parts = NONE;
    pub const TRANSCRIBE_ENGINE_VULKAN: Parts = NONE;
    pub const TRANSCRIBE_ENGINE_CPU: Parts = Parts {
        size: dub_asr::tcpp::Build::Cpu.download().size,
        files: &[FileSpec {
            url: dub_asr::tcpp::Build::Cpu.download().url,
            dest_rel: "models/transcribe-engine-cpu/libtranscribe_engine.dylib",
            size: dub_asr::tcpp::Build::Cpu.download().size,
            sha256: dub_asr::tcpp::Build::Cpu.download().sha256,
            extract: Extract::None,
        }],
        markers: &[Marker { rel: "models/transcribe-engine-cpu/libtranscribe_engine.dylib", expect: dub_asr::tcpp::Build::Cpu.download().size }],
    };

    // audiocpp_server (acpp_server::asset::SERVER_MACOS): Metal and the processor in one program.
    const SERVER_METAL: Parts = Parts {
        size: acpp_server::asset::SERVER_MACOS.size,
        files: &[FileSpec {
            url: acpp_server::asset::SERVER_MACOS.url,
            dest_rel: "tools/audiocpp-metal/_server.tar.gz",
            size: acpp_server::asset::SERVER_MACOS.size,
            sha256: acpp_server::asset::SERVER_MACOS.sha256,
            extract: Extract::Flat,
        }],
        markers: &[Marker { rel: "tools/audiocpp-metal/audiocpp_server", expect: 0 }],
    };

    /// The download of a build of the server: a Mac has its one Metal build.
    pub fn server_parts(build: acpp_server::asset::Build) -> &'static Parts {
        match build {
            acpp_server::asset::Build::Metal => &SERVER_METAL,
            _ => &NONE,
        }
    }

    pub const CUDA12_RUNTIME: Parts = NONE;
    pub const CUDA_RUNTIME: Parts = NONE;
    pub const CUDNN: Parts = NONE;
    pub const ORT_GPU: Parts = NONE;

    // Drama 3 is a CUDA engine: macOS does not offer it (`models::drama_unavailable`).
    pub const DRAMA_ENGINE: FileSpec = FileSpec { url: "", dest_rel: "models/drama-engine/libdrama_engine.dylib", size: 0, sha256: "", extract: Extract::None };

    // Qwen3-TTS (timoncool/qwentts.cpp engine-0.2.1, scripts/build-engine-macos.sh, ABI 7): libqwen.dylib with ggml,
    // the CPU backend and Metal linked in, no backend modules.
    pub const QWEN_ENGINE: Parts = Parts {
        size: super::QWEN_CODEC.size + QWEN_MACOS_SIZE,
        files: &[super::QWEN_CODEC, FileSpec { url: "https://github.com/timoncool/qwentts.cpp/releases/download/engine-0.2.1/qwen-macos-arm64.tar.gz", dest_rel: "models/qwen3-tts/_engine.tar.gz", size: QWEN_MACOS_SIZE, sha256: "116a8ff3686375974fe9a0f8d39970b36aa86e12e65e02dd4b7b3ee502afa9e7", extract: Extract::Flat }],
        markers: &[Marker { rel: super::QWEN_CODEC.dest_rel, expect: super::QWEN_CODEC.size }, Marker { rel: "models/qwen3-tts/libqwen.dylib", expect: 594_416 }],
    };
    const QWEN_MACOS_SIZE: u64 = 1_953_301;
    pub const QWEN_VULKAN: Parts = NONE;
    pub const QWEN_CUDA13: Parts = NONE;
    pub const QWEN_CUDA12: Parts = NONE;

    // BSRoformer.cpp v0.1.0, dub_sep::asset's one Mac build (the author's: Metal and the processor).
    pub const BSROFORMER: Parts = NONE;
    pub const BSROFORMER_CUDA12: Parts = NONE;
    pub const BSROFORMER_VULKAN: Parts = NONE;
    const SEP_CPU: [FileSpec; 1] = [super::sep_file(dub_sep::asset::Build::Cpu)];
    pub const BSROFORMER_CPU: Parts = Parts { size: SEP_CPU[0].size, files: &SEP_CPU, markers: &[Marker { rel: "tools/bsroformer-cpu/bs_roformer-cli", expect: 0 }] };

    // llama.cpp b11900 macos-arm64: Metal and the processor, rpath @loader_path.
    pub const LLAMA: Parts = NONE;
    pub const LLAMA_CUDA12: Parts = NONE;
    pub const LLAMA_VULKAN: Parts = NONE;
    pub const LLAMA_CPU: Parts = Parts {
        size: 16_295_973,
        files: &[FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-macos-arm64.tar.gz", dest_rel: "tools/llama-cpu/_llama.tar.gz", size: 16_295_973, sha256: "6d3af8649e687b288395ee79f4423a21376892edb94520fa6320bfd3eb81700b", extract: Extract::Flat }],
        markers: &[Marker { rel: "tools/llama-cpu/llama-server", expect: 0 }],
    };

    pub const ORT: Parts = Parts {
        size: 32_093_967,
        files: &[
            FileSpec { url: "https://github.com/microsoft/onnxruntime/releases/download/v1.28.2/onnxruntime-osx-arm64-1.28.2.tgz", dest_rel: "models/runtime/_ort.tgz", size: 32_093_967, sha256: "c4fceacfc53765d0869dc9180c31ec91054d149017a99d1e80ffe28dc79596de", extract: Extract::Tree },
        ],
        markers: &[Marker { rel: "models/runtime/onnxruntime-osx-arm64-1.28.2/lib/libonnxruntime.1.28.2.dylib", expect: 0 }],
    };

    // ffmpeg 9.0.2 of ffmpeg.martin-riedl.de (static, signed): ffmpeg and ffprobe, a zip each.
    pub const FFMPEG: Parts = Parts {
        size: 56_713_400,
        files: &[
            FileSpec { url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip", dest_rel: "tools/ffmpeg/_ffmpeg.zip", size: 28_395_699, sha256: "c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924", extract: Extract::Pick },
            FileSpec { url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffprobe.zip", dest_rel: "tools/ffmpeg/_ffprobe.zip", size: 28_317_701, sha256: "fcbe839537485eaee7a7a8bc5cbc0f90d53617e80943e8a5b2e31cb851197ea6", extract: Extract::Pick },
        ],
        markers: &[Marker { rel: "tools/ffmpeg/ffmpeg", expect: 66_332_880 }, Marker { rel: "tools/ffmpeg/ffprobe", expect: 66_158_064 }],
    };

    pub const YTDLP: Parts = Parts {
        size: 75_951_531,
        files: &[
            FileSpec { url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.21/yt-dlp_macos", dest_rel: "tools/yt-dlp/yt-dlp", size: 37_180_736, sha256: "7a6fe71d7d680f1fb0ac18d4949873d77db2859e90017c0f44bd94b23be4ee40", extract: Extract::None },
            FileSpec { url: "https://github.com/denoland/deno/releases/download/v2.9.8/deno-aarch64-apple-darwin.zip", dest_rel: "tools/yt-dlp/_deno.zip", size: 38_770_795, sha256: "9f0bf660393da2eaff044cdc7444cb08c73f235849b61d224d825a194ce7c98e", extract: Extract::Flat },
        ],
        markers: &[
            Marker { rel: "tools/yt-dlp/yt-dlp", expect: 37_180_736 },
            Marker { rel: "tools/yt-dlp/deno", expect: 81_512_928 },
        ],
    };
}

// Страница драйверов NVIDIA (кнопка «Открыть сайт» — драйвер DLL-кой не ставится).
pub const NVIDIA_DRIVER_URL: &str = "https://www.nvidia.com/Download/index.aspx";

// ═══════════════════════════════════════════════════════════════════════════
//  МАНИФЕСТ
// ═══════════════════════════════════════════════════════════════════════════

/// Fish Audio Drama 3: веса apiplant/drama-q4 закреплённой ревизии и её движок одним компонентом (одно без другого
/// не работает). Целиком — 21 ГБ видеопамяти; на меньшей карте эксперты живут в оперативной памяти (`models::drama_unavailable`).
const DRAMA3_FILES: &[FileSpec] = &[
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/.gitattributes", dest_rel: "models/drama3/.gitattributes", size: 1_570, sha256: "34448b82c17d60fec9b65b1f093c115ddbaadc04beb1b0140b6bfed2e012a930", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/LICENSE", dest_rel: "models/drama3/LICENSE", size: 19_685, sha256: "733a32442087acbda12ecafe8e71eb8ee5890c89b50ce7ac4ea3777faa3c823e", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/NOTICE", dest_rel: "models/drama3/NOTICE", size: 1_121, sha256: "a513aae85e0f74a5d0a7a6ca4fe2627fd5556bdc0e0076412f21a2296e5d3fe0", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/README.md", dest_rel: "models/drama3/README.md", size: 3_137, sha256: "e3c9172d892f7031663cb60c9fc616d8dca799b9d5b5c896dbfd1df0bc9551e8", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/codec.safetensors", dest_rel: "models/drama3/codec.safetensors", size: 837_939_684, sha256: "84bb73df0d9fe192018ecd5e5cddffebcad21b3c2f28e30ec2e1d5fc15083a41", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/drama.json", dest_rel: "models/drama3/drama.json", size: 5_307, sha256: "4fe5640dd94e208e696935aeb19710b073caf3aafb726a4b3e667ee7f1f92a3f", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/globals.safetensors", dest_rel: "models/drama3/globals.safetensors", size: 1_050_177_916, sha256: "1b3dbea1286ac4a0cb9f75671c1aed6e0d51da2e0218c1586a8ffa9fb179f886", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer00.safetensors", dest_rel: "models/drama3/layer00.safetensors", size: 491_594_072, sha256: "6541d5e1bd20cacd532a78ac46d19f5a3ef9c4ec880ada97f0eaa7f3f5db5c05", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer01.safetensors", dest_rel: "models/drama3/layer01.safetensors", size: 491_594_072, sha256: "7000cc6aead0bffd686dcfd7c94d4e3cb8d04ec37d16576c42c883371725d0de", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer02.safetensors", dest_rel: "models/drama3/layer02.safetensors", size: 491_594_072, sha256: "383b9d5afbedfcf058a63cebbb98130418b2015b3b408e8d02946d6fc4b4ec84", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer03.safetensors", dest_rel: "models/drama3/layer03.safetensors", size: 484_941_240, sha256: "4734df7ad21bcedaa0566a730d932db54a08dd7a501937a693f07e87c15f833b", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer04.safetensors", dest_rel: "models/drama3/layer04.safetensors", size: 491_594_072, sha256: "9a40ac54d3aba510eebc0956952e4e1c266e3796b9cf509c974023c5b529a532", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer05.safetensors", dest_rel: "models/drama3/layer05.safetensors", size: 491_594_072, sha256: "bd0c8c665a7fcd31bdc8b1d0d7c0b7a2176d04b245234b13c2812189330495bd", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer06.safetensors", dest_rel: "models/drama3/layer06.safetensors", size: 491_594_072, sha256: "a90d446c3a3f919da83002b871f132521064164b433b506becfdc6021b709caf", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer07.safetensors", dest_rel: "models/drama3/layer07.safetensors", size: 484_941_240, sha256: "265ad99ec624e82f513d2263593b8bbc5fd51067f8eef948fed101f6dbee76d7", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer08.safetensors", dest_rel: "models/drama3/layer08.safetensors", size: 491_594_072, sha256: "d428ad1704c59adfc9f4a703c3c1090dfb8acf925c4def6df2d8ce3ae427eae3", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer09.safetensors", dest_rel: "models/drama3/layer09.safetensors", size: 491_594_072, sha256: "8a4650096f0ebf418bf348c2a12605bf552656c9497a0d5b5de9ed33c5a9eb32", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer10.safetensors", dest_rel: "models/drama3/layer10.safetensors", size: 491_594_072, sha256: "2ec2fa321ff483b962be2db48369d0e867fb3bb853ba476867108ef9bbb8027e", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer11.safetensors", dest_rel: "models/drama3/layer11.safetensors", size: 484_941_240, sha256: "2c734336b700fdf43cd4c5da21ffce819669b291b0f282482b4741d1e174c987", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer12.safetensors", dest_rel: "models/drama3/layer12.safetensors", size: 491_594_072, sha256: "584c5c6c126c6c02247688afb7d8bdc20b62cb8318504e151a1f4f20431fb422", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer13.safetensors", dest_rel: "models/drama3/layer13.safetensors", size: 491_594_072, sha256: "2b93cddc4718a0801d18153e6254f932df53bb65f923a454c63e22553d0d2998", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer14.safetensors", dest_rel: "models/drama3/layer14.safetensors", size: 491_594_072, sha256: "3e2b5bcd065e2a3a8f23db7442fcc0ed2def45b9a45ef7733cb5cf5dcd567017", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer15.safetensors", dest_rel: "models/drama3/layer15.safetensors", size: 484_941_240, sha256: "7ae3cd1138be5741c854bb5833a79b45f3a075da38da4d3091d05323a0f6f65c", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer16.safetensors", dest_rel: "models/drama3/layer16.safetensors", size: 491_594_072, sha256: "c2a1f91e6dad93feaac77f2bfced4cb11eab64917cccc5b90da893c8cceb9303", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer17.safetensors", dest_rel: "models/drama3/layer17.safetensors", size: 491_594_072, sha256: "9963861e30334e6e338af27fadbe09edac6a5e5a8e497703d0389b04d65fc7b9", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer18.safetensors", dest_rel: "models/drama3/layer18.safetensors", size: 491_594_072, sha256: "a59bd2b492afdaaea1f8839dbf3917a61af7da9decccd3df95fa9762fdec9827", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer19.safetensors", dest_rel: "models/drama3/layer19.safetensors", size: 484_941_240, sha256: "ab0aebd5f49d357e257f363dec847fd8a2acf1e9a9dd4b00673a20a1df6026aa", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer20.safetensors", dest_rel: "models/drama3/layer20.safetensors", size: 491_594_072, sha256: "fad98d824c1a48fbd695b1478339d366bbec776ff2b81187ea6092103eb76c89", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer21.safetensors", dest_rel: "models/drama3/layer21.safetensors", size: 491_594_072, sha256: "b9cfc9357749d2917601cbc46a36c21f2e6421071e26525a58bfb1219160ce1e", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer22.safetensors", dest_rel: "models/drama3/layer22.safetensors", size: 491_594_072, sha256: "8047125966fae6c68c5877d32143468b0d1356315b4ad884085f12354bab2cef", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer23.safetensors", dest_rel: "models/drama3/layer23.safetensors", size: 484_941_240, sha256: "2fb6032e99495b69c88f696f7b394cd173b4f6653c2e8821ea906f0c3b6ce6d4", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer24.safetensors", dest_rel: "models/drama3/layer24.safetensors", size: 491_594_072, sha256: "8ab72775ec73f6a6d4c03563b5247a206ca43d68cc5a15d6b49866642ee4e829", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer25.safetensors", dest_rel: "models/drama3/layer25.safetensors", size: 491_594_072, sha256: "7adb4c2e25aab02979a2a66f9e722750a81bc81b952759323a304fa50155051e", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer26.safetensors", dest_rel: "models/drama3/layer26.safetensors", size: 491_594_072, sha256: "3183953c8f728446f070459f5339fe15ac97cb8d58a3dad3e4e035096ed39467", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer27.safetensors", dest_rel: "models/drama3/layer27.safetensors", size: 484_941_240, sha256: "3bd9566925b16fdaebbafcfca7f87933388410eeefeeacbcc694b9f19c8c8c77", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer28.safetensors", dest_rel: "models/drama3/layer28.safetensors", size: 491_594_072, sha256: "2095bc8e0b53ec8737605d73f41b392ddae53332d99cc6ca18c72c9c9c5967d4", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer29.safetensors", dest_rel: "models/drama3/layer29.safetensors", size: 491_594_072, sha256: "ff9133c5fb2510d44bc86ff47861e8c7cab992b27be33768fb36c8c0754bdf54", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer30.safetensors", dest_rel: "models/drama3/layer30.safetensors", size: 491_594_072, sha256: "31a56962fba00360cc4f8785d4530321bc65217ea9adc69e294edc85e09cd256", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer31.safetensors", dest_rel: "models/drama3/layer31.safetensors", size: 484_941_240, sha256: "d6fb33473b61f81457318b2de4de5d6ff102a4db13fa27a42a7415a23232f909", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer32.safetensors", dest_rel: "models/drama3/layer32.safetensors", size: 491_594_072, sha256: "d958faa82b5a713dbd0fc4d52b6190c23ce4bbb0b33a554c958bd96b2b4e58ab", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer33.safetensors", dest_rel: "models/drama3/layer33.safetensors", size: 491_594_072, sha256: "9ae3497ec8f5098b86ab3449364476bf950b01c12b25334ba1af105705f65712", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer34.safetensors", dest_rel: "models/drama3/layer34.safetensors", size: 491_594_072, sha256: "9d4b0aee182f40c2747708d3b8a6a05c5a93364a664e215559c63197d86d5a6d", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer35.safetensors", dest_rel: "models/drama3/layer35.safetensors", size: 484_941_240, sha256: "1ef286bbb7bdf15cae350dc45dc43f89e8de435f88d8f18290d3c965c44b9eba", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer36.safetensors", dest_rel: "models/drama3/layer36.safetensors", size: 491_594_072, sha256: "e097425a075c208948e558ded50c85fb3b680581d6c654e81ce1b657cbe385b5", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer37.safetensors", dest_rel: "models/drama3/layer37.safetensors", size: 491_594_072, sha256: "050478806fbe737cf84a4ee57a1f99ebd8908e4b5dc474396079b6980118d7ae", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer38.safetensors", dest_rel: "models/drama3/layer38.safetensors", size: 491_594_072, sha256: "16c4d89cf6421a66ab818bd70ae6ebb865f10fbb064b1bea7e4d55a13d92c77a", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/layer39.safetensors", dest_rel: "models/drama3/layer39.safetensors", size: 484_941_240, sha256: "efadbb0557562ee96b1cdd0551b11c2cb390b31bbeb713e3fc33d4b76465ae0d", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/tokenizer.json", dest_rel: "models/drama3/tokenizer.json", size: 20_791_070, sha256: "0e9b9db76b2f0389b04b6ef057a2ac3b89100b7e96f2dd7034d19580a09654d6", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/apiplant/drama-q4/resolve/1fce9bd21d46a0311294aebfad19f994df90902f/tower.safetensors", dest_rel: "models/drama3/tower.safetensors", size: 1_432_245_128, sha256: "52b5ad6ff0b85375e7ab30c84fe46828b17dfe390b8548024e304305102c6908", extract: Extract::None },
    platform::DRAMA_ENGINE,
];
const DRAMA3_WEIGHTS: u64 = 22_938_419_178;
const DRAMA3_MARKERS: &[Marker] = &[
    Marker { rel: "models/drama3/layer39.safetensors", expect: 484_941_240 },
    Marker { rel: "models/drama3/tokenizer.json", expect: 20_791_070 },
    Marker { rel: platform::DRAMA_ENGINE.dest_rel, expect: platform::DRAMA_ENGINE.size },
];

/// Qwen3-TTS 1.7B GGUF (Serveurperso/Qwen3-TTS-GGUF, Apache 2.0): кодек 12 Гц и талкеры CustomVoice и Base каждого кванта.
/// Пути и SHA — как у Speech Studio, чтобы файлы брались у неё.
const QWEN_CODEC: FileSpec = FileSpec { url: "https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-tokenizer-12hz-Q8_0.gguf", dest_rel: "models/qwen3-tts/qwen-tokenizer-12hz-Q8_0.gguf", size: 291_150_624, sha256: "1883beeed99348fc35e23dd225e9082f93f6f8c109330a33d935baa8acdbfd94", extract: Extract::None };
const QWEN_Q4_K_M: FileSpec = FileSpec { url: "https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-customvoice-Q4_K_M.gguf", dest_rel: "models/qwen3-tts/qwen-talker-1.7b-customvoice-Q4_K_M.gguf", size: 1_182_631_296, sha256: "cc328834a631bc08bf9f43e62fa23f8a1383d9b429864ce6690cfb172077fc4a", extract: Extract::None };
const QWEN_BASE_Q4_K_M: FileSpec = FileSpec { url: "https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-base-Q4_K_M.gguf", dest_rel: "models/qwen3-tts/qwen-talker-1.7b-base-Q4_K_M.gguf", size: 1_219_245_248, sha256: "ea393ebaf2167ea23ce9fc18b093822851358a950d7075cd47ab4f6ce23e887d", extract: Extract::None };
const QWEN_Q8_0: FileSpec = FileSpec { url: "https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-customvoice-Q8_0.gguf", dest_rel: "models/qwen3-tts/qwen-talker-1.7b-customvoice-Q8_0.gguf", size: 2_042_834_304, sha256: "cab2cff67a0a557310febe558dc83076b28ed790e491867eb2751759f4cd89fa", extract: Extract::None };
// HF: RUAccent (ruaccent/accentuator, MIT), ревизия закреплена sha коммита.
const HF_RUACCENT_ACCENT: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_accent/model.onnx";
const HF_RUACCENT_ACCENT_CONFIG: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_accent/config.json";
const HF_RUACCENT_ACCENT_VOCAB: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_accent/vocab.txt";
const HF_RUACCENT_OMOGRAPH: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_omograph/tiny2/model.onnx";
const HF_RUACCENT_OMOGRAPH_TOKENIZER: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/nn/nn_omograph/tiny2/tokenizer.json";
const HF_RUACCENT_ACCENTS: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/accents.json.gz";
const HF_RUACCENT_OMOGRAPHS: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/omographs.json.gz";
const HF_RUACCENT_YO_WORDS: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/yo_words.json.gz";
const HF_RUACCENT_YO_HOMOGRAPHS: &str = "https://huggingface.co/ruaccent/accentuator/resolve/b78ae5ea1e62beaf138bed1865cd8c3b0b5ca855/dictionary/yo_homographs.json.gz";
/// Supertonic 3 int8 (askurios8, weight-only int8 of Supertone/supertonic-3; Open RAIL-M): four ONNX graphs, the text
/// indexer and the ten preset voices.
const SUPERTONIC3_FILES: &[FileSpec] = &[
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/LICENSE", dest_rel: "models/supertonic3/LICENSE", size: 15_007, sha256: "0d944a9110fed9a9602d60e0423a272903e7bd21ab060490774efc77c2275e9f", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/duration_predictor.onnx", dest_rel: "models/supertonic3/onnx/duration_predictor.onnx", size: 3_700_147, sha256: "c3eb91414d5ff8a7a239b7fe9e34e7e2bf8a8140d8375ffb14718b1c639325db", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/text_encoder.onnx", dest_rel: "models/supertonic3/onnx/text_encoder.onnx", size: 9_734_378, sha256: "67b0975e50689f599a7834255d53a7cfd72a9f028b0f56e0040ba7dc5007d9cf", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/vector_estimator.onnx", dest_rel: "models/supertonic3/onnx/vector_estimator.onnx", size: 65_769_855, sha256: "f903a5ed046bb92cf1e557393623d7e709b527d338b042cc0bf9bce417185e5a", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/vocoder.onnx", dest_rel: "models/supertonic3/onnx/vocoder.onnx", size: 25_820_373, sha256: "ab337d1a10ed54c24cb42e0ea9be3733603149517fa0a8f54d07ef816cb766df", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/tts.json", dest_rel: "models/supertonic3/onnx/tts.json", size: 8_253, sha256: "42078d3aef1cd43ab43021f3c54f47d2d75ceb4e75f627f118890128b06a0d09", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/onnx/unicode_indexer.json", dest_rel: "models/supertonic3/onnx/unicode_indexer.json", size: 277_676, sha256: "9bf7346e43883a81f8645c81224f786d43c5b57f3641f6e7671a7d6c493cb24f", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F1.json", dest_rel: "models/supertonic3/voice_styles/F1.json", size: 292_046, sha256: "bbdec6ee00231c2c742ad05483df5334cab3b52fda3ba38e6a07059c4563dbc2", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F2.json", dest_rel: "models/supertonic3/voice_styles/F2.json", size: 292_423, sha256: "7c722c6a72707b1a77f035d67f0d1351ba187738e06f7683e8c72b1df3477fc6", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F3.json", dest_rel: "models/supertonic3/voice_styles/F3.json", size: 290_794, sha256: "12f6ef2573baa2defa1128069cb59f203e3ab67c92af77b42df8a0e3a2f7c6ab", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F4.json", dest_rel: "models/supertonic3/voice_styles/F4.json", size: 291_808, sha256: "c2fa764c1225a76dfc3e2c73e8aa4f70d9ee48793860eb34c295fff01c2e032b", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/F5.json", dest_rel: "models/supertonic3/voice_styles/F5.json", size: 291_479, sha256: "45966e73316415626cf41a7d1c6f3b4c70dbc1ba2bee5c1978ef0ce33244fc8d", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M1.json", dest_rel: "models/supertonic3/voice_styles/M1.json", size: 291_748, sha256: "e35604687f5d23694b8e91593a93eec0e4eca6c0b02bb8ed69139ab2ea6b0a5b", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M2.json", dest_rel: "models/supertonic3/voice_styles/M2.json", size: 292_055, sha256: "b76cbf62bac707c710cf0ae5aba5e31eea1a6339a9734bfae33ab98499534a50", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M3.json", dest_rel: "models/supertonic3/voice_styles/M3.json", size: 290_198, sha256: "ea1ac35ccb91b0d7ecad533a2fbd0eec10c91513d8951e3b25fbba99954e159b", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M4.json", dest_rel: "models/supertonic3/voice_styles/M4.json", size: 291_522, sha256: "ca8eefad4fcd989c9379032ff3e50738adc547eeb5e221b82593a6d7b3bac303", extract: Extract::None },
    FileSpec { url: "https://huggingface.co/askurios8/supertonic-3-int8/resolve/95618f5d61cef4923c3b802a332fd603723718f7/voice_styles/M5.json", dest_rel: "models/supertonic3/voice_styles/M5.json", size: 291_469, sha256: "dd22b92740314321f8ae11c5e87f8dd60d060f15dd3a632b5adf77f471f77af2", extract: Extract::None },
];
const SUPERTONIC3_SIZE: u64 = 108_241_231;
const SUPERTONIC3_MARKERS: &[Marker] = &[
    Marker { rel: "models/supertonic3/onnx/vector_estimator.onnx", expect: 65_769_855 },
    Marker { rel: "models/supertonic3/onnx/vocoder.onnx", expect: 25_820_373 },
    Marker { rel: "models/supertonic3/voice_styles/M5.json", expect: 291_469 },
];
const QWEN_RU_STRESS: FileSpec = FileSpec { url: "https://huggingface.co/siriusfreak/qwen3-tts-12hz-1.7b-ru-stress-cf/resolve/c16b3af3db3095120efc1ebb73f6522442bcb3b1/lora/adapter.safetensors", dest_rel: "models/qwen3-tts/qwen-talker-1.7b-base-ru-stress-lora.safetensors", size: 34_908_456, sha256: "88a94c0fbe85800d309f94bb9c28d698bf9c86d9494617b6cf4dd996cc13648b", extract: Extract::None };
const QWEN_BASE_Q8_0: FileSpec = FileSpec { url: "https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF/resolve/b7ee2e8c7459c3bea99da23e3d178125a7d1713c/qwen-talker-1.7b-base-Q8_0.gguf", dest_rel: "models/qwen3-tts/qwen-talker-1.7b-base-Q8_0.gguf", size: 2_079_448_256, sha256: "4b9a33a236908dd9435a42f7a396e38038329d053b704342a6413c08544c4fda", extract: Extract::None };

/// Полный список компонентов. Порядок = порядок показа в панели «Первый запуск».
pub fn manifest() -> Vec<Component> {
    let mut all = vec![
        // ── МОДЕЛИ ──────────────────────────────────────────────────────────
        Component {
            id: "higgs",
            name: "Higgs Audio v3 (Q8_0)".into(),
            purpose: t!("setup-comp-higgs-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: 5_530_678_590,
            files: &[
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/q8_0.gguf", dest_rel: "models/higgs-q8_0/q8_0.gguf", size: 5_519_235_296, sha256: "b857344af06b1b2497f4f8c1d0f0c134d0eeaf9c089c0d28ae6e58084d90f901", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/config.json", dest_rel: "models/higgs-q8_0/config.json", size: 2_755, sha256: "2ead4442c079ee35c2123a5b197e126e18eccfc0bdb65d31c94767e75d7864d4", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/chat_template.jinja", dest_rel: "models/higgs-q8_0/chat_template.jinja", size: 2_427, sha256: "44d5f08f3f72b837eaad09f13a54c1f9f4eb58d75240334548b7fd52a5437fa5", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer.json", dest_rel: "models/higgs-q8_0/tokenizer.json", size: 11_433_924, sha256: "eb883de2de5adc5113f1f02b54830a0ea7cd6ef191cde65c41aceb3737d4d1c1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/tokenizer_config.json", dest_rel: "models/higgs-q8_0/tokenizer_config.json", size: 1_937, sha256: "b4d632e1239569fb1829bf0bfa3c674fa54f22c42e9cb2669d77c438d4b4e02c", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json", dest_rel: "models/higgs-q8_0/higgs_audio_v2_tokenizer_config.json", size: 2_251, sha256: "1f96f10516c2bb59d5a127e04e659a331a50fb1684217b1492fabd1dc94def26", extract: Extract::None },
            ],
            markers: &[
                Marker { rel: "models/higgs-q8_0/q8_0.gguf", expect: 5_519_235_296 },
                Marker { rel: "models/higgs-q8_0/config.json", expect: 2_755 },
                Marker { rel: "models/higgs-q8_0/tokenizer.json", expect: 11_433_924 },
            ],
            external_url: None,
        },
        Component {
            id: "higgs-engine",
            name: t!("setup-comp-higgs-engine-name"),
            purpose: t!("setup-comp-higgs-engine-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: platform::server_parts(crate::acpp::build()).size,
            files: platform::server_parts(crate::acpp::build()).files,
            markers: platform::server_parts(crate::acpp::build()).markers,
            external_url: None,
        },
        Component {
            id: "gemma",
            name: "Gemma-4 12B QAT q4_0 + vision".into(),
            purpose: t!("setup-comp-gemma-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: 7_150_992_992,
            files: &[
                FileSpec { url: "https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/gemma-4-12b-it-qat-q4_0.gguf", dest_rel: "models/mt/gemma-4-12b-it-qat-q4_0.gguf", size: 6_975_877_728, sha256: "faff1a63667fac17ac5e777f47114688fcefea96e220e211aaa8d62c2c4561f1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/google/gemma-4-12b-it-qat-q4_0-gguf/resolve/2b318d6ebebf093f50ca4376e858325f10703358/mmproj-gemma-4-12b-it-qat-q4_0.gguf", dest_rel: "models/mt/mmproj-gemma-4-12b-it-qat-q4_0.gguf", size: 175_115_264, sha256: "e70b0e5cd80323d5d588b4ed06780356b7b1ba03995a4b8164c6ae9db0ff5989", extract: Extract::None },
            ],
            markers: &[
                Marker { rel: "models/mt/gemma-4-12b-it-qat-q4_0.gguf", expect: 6_975_877_728 },
                Marker { rel: "models/mt/mmproj-gemma-4-12b-it-qat-q4_0.gguf", expect: 175_115_264 },
            ],
            external_url: None,
        },
        // Альтернативные кванты Gemma (выбор в настройках; тяжелее q4_0, чуть точнее). Свой mmproj на квант.
        Component {
            id: "gemma-q5_0",
            name: "Gemma-4 12B (Q5_K_M) + vision".into(),
            purpose: t!("setup-comp-gemma-q5-0-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 8_588_690_400,
            files: &[
                FileSpec { url: "https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q5_K_M.gguf", dest_rel: "models/mt-q5_0/gemma-4-12b-it-Q5_K_M.gguf", size: 8_413_574_560, sha256: "1bc633ec98817858bec10f73fa026481c9662449aae4b80a05dfb28ef784c278", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf", dest_rel: "models/mt-q5_0/mmproj-F16.gguf", size: 175_115_840, sha256: "91f086971e56d7a7d8d39e271873fccdb49541bd259d6e02c401a4f1cb7a219e", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/mt-q5_0/gemma-4-12b-it-Q5_K_M.gguf", expect: 8_413_574_560 }, Marker { rel: "models/mt-q5_0/mmproj-F16.gguf", expect: 175_115_840 }],
            external_url: None,
        },
        Component {
            id: "gemma-q6_k",
            name: "Gemma-4 12B (Q6_K) + vision".into(),
            purpose: t!("setup-comp-gemma-q6-k-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 9_961_137_120,
            files: &[
                FileSpec { url: "https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q6_K.gguf", dest_rel: "models/mt-q6_k/gemma-4-12b-it-Q6_K.gguf", size: 9_786_021_280, sha256: "e1602ddc224c159584eb4c7d6a6c8d682fc6afb2efb8f76c10bfd63ba71436a2", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf", dest_rel: "models/mt-q6_k/mmproj-F16.gguf", size: 175_115_840, sha256: "91f086971e56d7a7d8d39e271873fccdb49541bd259d6e02c401a4f1cb7a219e", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/mt-q6_k/gemma-4-12b-it-Q6_K.gguf", expect: 9_786_021_280 }, Marker { rel: "models/mt-q6_k/mmproj-F16.gguf", expect: 175_115_840 }],
            external_url: None,
        },
        Component {
            id: "gemma-q8_0",
            name: "Gemma-4 12B (Q8_0) + vision".into(),
            purpose: t!("setup-comp-gemma-q8-0-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 12_844_762_080,
            files: &[
                FileSpec { url: "https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/gemma-4-12b-it-Q8_0.gguf", dest_rel: "models/mt-q8_0/gemma-4-12b-it-Q8_0.gguf", size: 12_669_646_240, sha256: "74d2d4f0b5b08ca8589d1a5f50e689c0984469f3cedbdc7d67458c6e9e35496a", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/unsloth/gemma-4-12b-it-GGUF/resolve/d997c805aafe035a8024f961c6e1afd6b30d79a5/mmproj-F16.gguf", dest_rel: "models/mt-q8_0/mmproj-F16.gguf", size: 175_115_840, sha256: "91f086971e56d7a7d8d39e271873fccdb49541bd259d6e02c401a4f1cb7a219e", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/mt-q8_0/gemma-4-12b-it-Q8_0.gguf", expect: 12_669_646_240 }, Marker { rel: "models/mt-q8_0/mmproj-F16.gguf", expect: 175_115_840 }],
            external_url: None,
        },
        Component {
            id: "parakeet",
            name: "Parakeet TDT 0.6B v3 (GGUF Q8_0, transcribe.cpp)".into(),
            purpose: t!("setup-comp-parakeet-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: 739_508_576,
            files: &[FileSpec { url: "https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v3-gguf/resolve/d8d8c32cb2017c57e5307767343e97870f8a7f53/parakeet-tdt-0.6b-v3-Q8_0.gguf", dest_rel: "models/asr-gguf-parakeet-v3/parakeet-tdt-0.6b-v3-Q8_0.gguf", size: 739_508_576, sha256: "5859f77944efcd8eafa23a6350731960b2b55b2203df51f319665c807d802cc7", extract: Extract::None }],
            markers: &[Marker { rel: "models/asr-gguf-parakeet-v3/parakeet-tdt-0.6b-v3-Q8_0.gguf", expect: 739_508_576 }],
            external_url: None,
        },
        // Recognition by transcribe.cpp (ggml), the same engine and files as Speech Studio (dub_asr::tcpp), so either
        // program takes them from the other: the CUDA build on the card, the CPU build without one (asr_backend).
        Component {
            id: "transcribe-engine",
            name: "transcribe.cpp (CUDA)".into(),
            purpose: t!("setup-comp-transcribe-engine-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: platform::TRANSCRIBE_ENGINE.size,
            files: platform::TRANSCRIBE_ENGINE.files,
            markers: platform::TRANSCRIBE_ENGINE.markers,
            external_url: None,
        },
        Component {
            id: "transcribe-engine-cpu",
            name: "transcribe.cpp (CPU)".into(),
            purpose: t!("setup-comp-transcribe-engine-cpu-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::TRANSCRIBE_ENGINE_CPU.size,
            files: platform::TRANSCRIBE_ENGINE_CPU.files,
            markers: platform::TRANSCRIBE_ENGINE_CPU.markers,
            external_url: None,
        },
        Component {
            id: "transcribe-engine-cuda12",
            name: "transcribe.cpp (CUDA 12)".into(),
            purpose: t!("setup-comp-transcribe-engine-cuda12-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::TRANSCRIBE_ENGINE_CUDA12.size,
            files: platform::TRANSCRIBE_ENGINE_CUDA12.files,
            markers: platform::TRANSCRIBE_ENGINE_CUDA12.markers,
            external_url: None,
        },
        Component {
            id: "transcribe-engine-vulkan",
            name: "transcribe.cpp (Vulkan)".into(),
            purpose: t!("setup-comp-transcribe-engine-vulkan-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::TRANSCRIBE_ENGINE_VULKAN.size,
            files: platform::TRANSCRIBE_ENGINE_VULKAN.files,
            markers: platform::TRANSCRIBE_ENGINE_VULKAN.markers,
            external_url: None,
        },
        // Альтернативные кванты TTS Higgs (выбор в настройках; своя папка на квант, aux те же).
        Component {
            id: "higgs-q6_k",
            name: "Higgs Audio v3 (Q6_K)".into(),
            purpose: t!("setup-comp-higgs-q6-k-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 5_035_080_542,
            files: &[
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/q6_k.gguf", dest_rel: "models/higgs-q6_k/q6_k.gguf", size: 5_023_637_248, sha256: "764399ced4439adaf3d5d3ca95720276b8ab06fc92a6439f9ce28f3ad671ba4c", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/config.json", dest_rel: "models/higgs-q6_k/config.json", size: 2_755, sha256: "2ead4442c079ee35c2123a5b197e126e18eccfc0bdb65d31c94767e75d7864d4", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/chat_template.jinja", dest_rel: "models/higgs-q6_k/chat_template.jinja", size: 2_427, sha256: "44d5f08f3f72b837eaad09f13a54c1f9f4eb58d75240334548b7fd52a5437fa5", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer.json", dest_rel: "models/higgs-q6_k/tokenizer.json", size: 11_433_924, sha256: "eb883de2de5adc5113f1f02b54830a0ea7cd6ef191cde65c41aceb3737d4d1c1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/tokenizer_config.json", dest_rel: "models/higgs-q6_k/tokenizer_config.json", size: 1_937, sha256: "b4d632e1239569fb1829bf0bfa3c674fa54f22c42e9cb2669d77c438d4b4e02c", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json", dest_rel: "models/higgs-q6_k/higgs_audio_v2_tokenizer_config.json", size: 2_251, sha256: "1f96f10516c2bb59d5a127e04e659a331a50fb1684217b1492fabd1dc94def26", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/higgs-q6_k/q6_k.gguf", expect: 5_023_637_248 }, Marker { rel: "models/higgs-q6_k/tokenizer.json", expect: 11_433_924 }],
            external_url: None,
        },
        Component {
            id: "higgs-q4_k_m",
            name: "Higgs Audio v3 (Q4_K_M)".into(),
            purpose: t!("setup-comp-higgs-q4-k-m-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 4_098_366_270,
            files: &[
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/q4_k_m.gguf", dest_rel: "models/higgs-q4_k_m/q4_k_m.gguf", size: 4_086_922_976, sha256: "a6c8a9b5c8c72965865988c6ef411d32446aeb0df730f59e7bd4a3e801cea3a1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/config.json", dest_rel: "models/higgs-q4_k_m/config.json", size: 2_755, sha256: "2ead4442c079ee35c2123a5b197e126e18eccfc0bdb65d31c94767e75d7864d4", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/chat_template.jinja", dest_rel: "models/higgs-q4_k_m/chat_template.jinja", size: 2_427, sha256: "44d5f08f3f72b837eaad09f13a54c1f9f4eb58d75240334548b7fd52a5437fa5", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer.json", dest_rel: "models/higgs-q4_k_m/tokenizer.json", size: 11_433_924, sha256: "eb883de2de5adc5113f1f02b54830a0ea7cd6ef191cde65c41aceb3737d4d1c1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/tokenizer_config.json", dest_rel: "models/higgs-q4_k_m/tokenizer_config.json", size: 1_937, sha256: "b4d632e1239569fb1829bf0bfa3c674fa54f22c42e9cb2669d77c438d4b4e02c", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/drbaph/Higgs-Audio-v3-Studio/resolve/c6e9db5a2062c15accc1b9bfa54d927bbdb124dc/models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json", dest_rel: "models/higgs-q4_k_m/higgs_audio_v2_tokenizer_config.json", size: 2_251, sha256: "1f96f10516c2bb59d5a127e04e659a331a50fb1684217b1492fabd1dc94def26", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/higgs-q4_k_m/q4_k_m.gguf", expect: 4_086_922_976 }, Marker { rel: "models/higgs-q4_k_m/tokenizer.json", expect: 11_433_924 }],
            external_url: None,
        },
        // Второй движок озвучки (выбор в настройках, тот же слот tts, что у квантов Higgs).
        Component {
            id: "drama3",
            name: "Fish Audio Drama 3 (Q4)".into(),
            purpose: t!("setup-comp-drama3-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: DRAMA3_WEIGHTS + platform::DRAMA_ENGINE.size,
            files: DRAMA3_FILES,
            markers: DRAMA3_MARKERS,
            external_url: None,
        },
        // Третий движок озвучки: клон по эталону (талкер Base) и девять голосов модели (CustomVoice), тот же слот tts.
        // Движок — библиотека и кодек, общие для обоих квантов; кванты — пары талкеров.
        Component {
            id: "qwen3-tts-engine",
            name: t!("setup-comp-qwen3-tts-engine-name"),
            purpose: t!("setup-comp-qwen3-tts-engine-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::QWEN_ENGINE.size,
            files: platform::QWEN_ENGINE.files,
            markers: platform::QWEN_ENGINE.markers,
            external_url: None,
        },
        Component {
            id: "qwen3-tts",
            name: "Qwen3-TTS 1.7B (Q4_K_M)".into(),
            purpose: t!("setup-comp-qwen3-tts-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: QWEN_Q4_K_M.size + QWEN_BASE_Q4_K_M.size,
            files: &[QWEN_Q4_K_M, QWEN_BASE_Q4_K_M],
            markers: &[
                Marker { rel: QWEN_Q4_K_M.dest_rel, expect: QWEN_Q4_K_M.size },
                Marker { rel: QWEN_BASE_Q4_K_M.dest_rel, expect: QWEN_BASE_Q4_K_M.size },
            ],
            external_url: None,
        },
        Component {
            id: "qwen3-tts-q8_0",
            name: "Qwen3-TTS 1.7B (Q8_0)".into(),
            purpose: t!("setup-comp-qwen3-tts-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: QWEN_Q8_0.size + QWEN_BASE_Q8_0.size,
            files: &[QWEN_Q8_0, QWEN_BASE_Q8_0],
            markers: &[
                Marker { rel: QWEN_Q8_0.dest_rel, expect: QWEN_Q8_0.size },
                Marker { rel: QWEN_BASE_Q8_0.dest_rel, expect: QWEN_BASE_Q8_0.size },
            ],
            external_url: None,
        },
        Component {
            id: "qwen3-tts-ru-stress",
            name: t!("setup-comp-qwen3-tts-ru-stress-name"),
            purpose: t!("setup-comp-qwen3-tts-ru-stress-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: QWEN_RU_STRESS.size,
            files: &[QWEN_RU_STRESS],
            markers: &[Marker { rel: QWEN_RU_STRESS.dest_rel, expect: QWEN_RU_STRESS.size }],
            external_url: None,
        },
        Component {
            id: "supertonic3",
            name: t!("setup-comp-supertonic3-name"),
            purpose: t!("setup-comp-supertonic3-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: SUPERTONIC3_SIZE,
            files: SUPERTONIC3_FILES,
            markers: SUPERTONIC3_MARKERS,
            external_url: None,
        },
        Component {
            id: "ruaccent",
            name: "RUAccent".into(),
            purpose: t!("setup-comp-ruaccent-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 33_942_783,
            files: &[
                FileSpec { url: HF_RUACCENT_ACCENT, dest_rel: "models/ruaccent/nn/nn_accent/model.onnx", size: 803_402, sha256: "4e393144e45626f6f1062a0784ef06f921b97321a8e7b87ac2a09a892286500a", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_ACCENT_CONFIG, dest_rel: "models/ruaccent/nn/nn_accent/config.json", size: 841, sha256: "e377f159a8fc8673e211f100d4a237d5fe3dceb3b6789a5a738bf50bda23be49", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_ACCENT_VOCAB, dest_rel: "models/ruaccent/nn/nn_accent/vocab.txt", size: 140, sha256: "242882b2f27800e49a6babf46867dabf6dc9b6535b600776a68815fe4d4f4382", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_OMOGRAPH, dest_rel: "models/ruaccent/nn/nn_omograph/tiny2/model.onnx", size: 10_528_004, sha256: "c28840ab333be5870451a57b28839d3896e3332b791e8be29e75eb1c13338882", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_OMOGRAPH_TOKENIZER, dest_rel: "models/ruaccent/nn/nn_omograph/tiny2/tokenizer.json", size: 882_532, sha256: "d39943ce113bf88db2556a096c7929080b8c6afedfeefa7e3c3c736ac1eb0090", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_ACCENTS, dest_rel: "models/ruaccent/dictionary/accents.json.gz", size: 20_954_156, sha256: "aa460ebba90de00fbbf3d41d121961f605b98667e45efb7920f127473b15515e", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_OMOGRAPHS, dest_rel: "models/ruaccent/dictionary/omographs.json.gz", size: 219_047, sha256: "04a9e81c68d65f65ba493fe0110f99e79087548c2beeec3032e2b66e28706f36", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_YO_WORDS, dest_rel: "models/ruaccent/dictionary/yo_words.json.gz", size: 548_914, sha256: "a19fa89a964a0691d9fe4ee384783e3934904891843d8f59a1c480d67947a82a", extract: Extract::None },
                FileSpec { url: HF_RUACCENT_YO_HOMOGRAPHS, dest_rel: "models/ruaccent/dictionary/yo_homographs.json.gz", size: 5_747, sha256: "c4ee777bbbab87f9eac838f370ad92974e079d02b21903e480c54b5f0c8c60d1", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/ruaccent/dictionary/accents.json.gz", expect: 20_954_156 }],
            external_url: None,
        },
        Component {
            id: "ruaccent-omograph-big",
            name: t!("setup-comp-ruaccent-omograph-big-name"),
            purpose: t!("setup-comp-ruaccent-omograph-big-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 181_553_054 + 3_715_061,
            files: &[
                FileSpec { url: "https://huggingface.co/nerualdreming/ruaccent-omograph-int8/resolve/d5c4eb6864d55cfc24ae45530eb4ef96843efb2b/big_poetry/model.onnx", dest_rel: "models/ruaccent/nn/nn_omograph/big_poetry/model.onnx", size: 181_553_054, sha256: "028407ff85d499b56efd25c31c326ecb7bd6496399f52a002b11789a7fc63964", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nerualdreming/ruaccent-omograph-int8/resolve/d5c4eb6864d55cfc24ae45530eb4ef96843efb2b/big_poetry/tokenizer.json", dest_rel: "models/ruaccent/nn/nn_omograph/big_poetry/tokenizer.json", size: 3_715_061, sha256: "0f120fc8462d8684194e7fff76bd43a3b8535bb63df158ee7cd37b8ba5c92f03", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/ruaccent/nn/nn_omograph/big_poetry/model.onnx", expect: 181_553_054 }],
            external_url: None,
        },
        Component {
            id: "parakeet-ultra",
            name: "Parakeet Ultra 0.6B (GGUF Q8_0, transcribe.cpp)".into(),
            purpose: t!("setup-comp-parakeet-ultra-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 740_363_168,
            files: &[FileSpec { url: "https://huggingface.co/handy-computer/parakeet-ultra-gguf/resolve/a1539be1b849ea7463b88343e5277b373d0c4ea9/parakeet-ultra-Q8_0.gguf", dest_rel: "models/asr-gguf-parakeet-ultra/parakeet-ultra-Q8_0.gguf", size: 740_363_168, sha256: "007a59761e9258779f189df396b50d12b83bb5b79e66df4b955c230d2f2a0a59", extract: Extract::None }],
            markers: &[Marker { rel: "models/asr-gguf-parakeet-ultra/parakeet-ultra-Q8_0.gguf", expect: 740_363_168 }],
            external_url: None,
        },
        // ── Whisper: CrisperWhisper 2.0 turbo (nyralabs, лицензия nyra health Non-Commercial, LICENSE.md рядом) ──
        // Репозиторий Hugging Face как есть, ревизия закреплена: audio.cpp читает папку. Идёт в audiocpp_server студии.
        Component {
            id: "crisperwhisper-turbo",
            name: "CrisperWhisper 2.0 turbo".into(),
            purpose: t!("setup-comp-crisperwhisper-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 1_623_586_704,
            files: &[
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/LICENSE.md", dest_rel: "models/crisperwhisper2-turbo/LICENSE.md", size: 17_784, sha256: "42f43bf4dc72ef422f91f6ca143f4514e21a68b353940d99091c1addcd0aee79", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/README.md", dest_rel: "models/crisperwhisper2-turbo/README.md", size: 10_974, sha256: "ddc82a301a1b59035eefcc1a8b2ec90d56257104df7365b00493c58135a6b029", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/added_tokens.json", dest_rel: "models/crisperwhisper2-turbo/added_tokens.json", size: 35_319, sha256: "228ddf33a2c2b94033dc0fad8e7847b554b1c9f7d2054f5455ffd562b6a30dc7", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/config.json", dest_rel: "models/crisperwhisper2-turbo/config.json", size: 1_211, sha256: "9d823d0eacddfc5ee920fac73897abfd29fbecac7d9e49b5d8e7f0bd63a56144", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/generation_config.json", dest_rel: "models/crisperwhisper2-turbo/generation_config.json", size: 3_880, sha256: "e20fb70a44ce2b3cb3b2bbbea852e064638563b072719640a286d2a6865066e1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/merges.txt", dest_rel: "models/crisperwhisper2-turbo/merges.txt", size: 493_869, sha256: "2df2990a395e35e8dfbc7511e08c12d56018d8d04691e0133e5d63b21e154dc6", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/model.safetensors", dest_rel: "models/crisperwhisper2-turbo/model.safetensors", size: 1_617_907_538, sha256: "0121a185b87ada7497f22dfbcd6c86b5f599b69dfcc8676e37f36447caf0b630", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/normalizer.json", dest_rel: "models/crisperwhisper2-turbo/normalizer.json", size: 52_666, sha256: "bf1c507dc8724ca9cf9903640dacfb69dae2f00edee4f21ceba106a7392f26dd", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/preprocessor_config.json", dest_rel: "models/crisperwhisper2-turbo/preprocessor_config.json", size: 357, sha256: "654cf18d3e163b948ceaf9766da56ce0b52de265d58673cf61c9376f126bd499", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/special_tokens_map.json", dest_rel: "models/crisperwhisper2-turbo/special_tokens_map.json", size: 2_186, sha256: "baea4ea09372eb4fca86b4e4346139fd73cb807d5087e9de0948e971739c3e74", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/tokenizer.json", dest_rel: "models/crisperwhisper2-turbo/tokenizer.json", size: 3_937_105, sha256: "00e6dce8d06acafefaea8d5d1e57e89c69b3237d08ecbe2bf6eb7124e248bcde", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/tokenizer_config.json", dest_rel: "models/crisperwhisper2-turbo/tokenizer_config.json", size: 288_287, sha256: "3089ab61ce4745667a41e91620081cd3e7d61a9ae1f79fdf91b1bc8983916f4e", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/vocab.json", dest_rel: "models/crisperwhisper2-turbo/vocab.json", size: 835_528, sha256: "6788c80b082e9b0d1393147d3a3e62ba19285ac0c82ace8e5ef00f37ead58971", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/crisperwhisper2-turbo/model.safetensors", expect: 1_617_907_538 }],
            external_url: None,
        },
        // Noise suppression of a library voice and of a recording to transcribe (DPDFNet, ONNX), as in Speech Studio.
        Component {
            id: "dpdfnet",
            name: t!("setup-comp-dpdfnet-name"),
            purpose: t!("setup-comp-dpdfnet-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: 10_493_337,
            files: &[FileSpec { url: "https://huggingface.co/Ceva-IP/DPDFNet/resolve/dd6818d00f50c836fed43a6243ebe49116de5964/onnx/dpdfnet2_48khz_hr.onnx", dest_rel: "models/dpdfnet/dpdfnet2_48khz_hr.onnx", size: 10_493_337, sha256: "7f0575a5cec0ba4ffd8f8bd657e06d007e4ccdd955d76faab922b9d3291dc14b", extract: Extract::None }],
            markers: &[Marker { rel: "models/dpdfnet/dpdfnet2_48khz_hr.onnx", expect: 10_493_337 }],
            external_url: None,
        },
        // Where the speech is (Silero VAD v6.2 on transcribe.cpp): a long recording goes to Parakeet Ultra in pieces cut
        // at the pauses, as in Speech Studio.
        Component {
            id: "silero-vad",
            name: t!("setup-comp-silero-vad-name"),
            purpose: t!("setup-comp-silero-vad-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: 1_241_056,
            files: &[FileSpec { url: "https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/a9b2d61c20ae4ddf3644edb34969e8185a485448/silero-vad-v6.2-F32.gguf", dest_rel: "models/silero-vad/silero-vad-v6.2-F32.gguf", size: 1_241_056, sha256: "0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6", extract: Extract::None }],
            markers: &[Marker { rel: "models/silero-vad/silero-vad-v6.2-F32.gguf", expect: 1_241_056 }],
            external_url: None,
        },
        // Определение языка речи (VoxLingua107, transcribe.cpp): язык оригинала «авто» для CrisperWhisper.
        Component {
            id: "langid",
            name: "VoxLingua107 ECAPA (Q8_0, transcribe.cpp)".into(),
            purpose: t!("setup-comp-langid-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 26_693_632,
            files: &[FileSpec { url: "https://huggingface.co/handy-computer/lang-id-voxlingua107-ecapa-gguf/resolve/e59f0f3847e77723b7d58447ef9456fa0c7e9436/lang-id-voxlingua107-ecapa-Q8_0.gguf", dest_rel: "models/langid-voxlingua107/lang-id-voxlingua107-ecapa-Q8_0.gguf", size: 26_693_632, sha256: "063e6cb0af3d53085fc7d68356dfde5ecee2ffa7bc79855f7e4ff930081bfe3a", extract: Extract::None }],
            markers: &[Marker { rel: "models/langid-voxlingua107/lang-id-voxlingua107-ecapa-Q8_0.gguf", expect: 26_693_632 }],
            external_url: None,
        },
        // Диаризация. Маркер — только файл Nemotron: у старых установок лежит лишь Sortformer v2
        // (models/sortformer/…4spk-v2.onnx, его parakeet-rs 0.3.8 не грузит), и компонент для них недостающий —
        // догружается первым запуском или on-demand перед анализом. Старый файл не трогаем.
        Component {
            id: "sortformer",
            name: t!("setup-comp-sortformer-name"),
            purpose: t!("setup-comp-sortformer-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: 400_509_316,
            files: &[
                FileSpec { url: HF_NEMOTRON_DIAR, dest_rel: "models/nemotron-diar/nemotron3_diar_v3.onnx", size: 400_506_656, sha256: "915e4fa23b0192ed9fadeb1cdd26847df986d50c92012d177be28d0343bbe03a", extract: Extract::None },
                FileSpec { url: HF_NEMOTRON_DIAR_LICENSE, dest_rel: "models/nemotron-diar/LICENSE", size: 2_660, sha256: "14cf93aed5ee7c72516170ecb65fb6d7e54ef19217d328c8b00b78eaf61c8b36", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/nemotron-diar/nemotron3_diar_v3.onnx", expect: 400_506_656 }],
            external_url: None,
        },
        Component {
            id: "roformer",
            name: "Mel-Band Roformer voc_fv6 (Q8_0)".into(),
            purpose: t!("setup-comp-roformer-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: 251_707_744,
            files: &[
                FileSpec { url: HF_ROFORMER, dest_rel: "models/bsroformer/voc_fv6-Q8_0.gguf", size: 251_707_744, sha256: "2cd84c9f24513749b0cb1a6ab3e3be5c5e2f7d0e8533e50512c1f394d2828a73", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/bsroformer/voc_fv6-Q8_0.gguf", expect: 251_707_744 }],
            external_url: None,
        },
        // Альтернативные кванты сепарации (выбор в настройках; лёгкие, качество чуть ниже Q8_0).
        Component {
            id: "roformer-q5",
            name: "Mel-Band Roformer voc_fv6 (Q5_0)".into(),
            purpose: t!("setup-comp-roformer-q5-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 167_303_008,
            files: &[
                FileSpec { url: "https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q5_0.gguf", dest_rel: "models/bsroformer/voc_fv6-Q5_0.gguf", size: 167_303_008, sha256: "85e465d209684c5269f595a2982ab21dce18d192a591b9f3309386bb57e6a86b", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/bsroformer/voc_fv6-Q5_0.gguf", expect: 167_303_008 }],
            external_url: None,
        },
        Component {
            id: "roformer-q4",
            name: "Mel-Band Roformer voc_fv6 (Q4_0)".into(),
            purpose: t!("setup-comp-roformer-q4-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 139_168_096,
            files: &[
                FileSpec { url: "https://huggingface.co/chenmozhijin/BSRoformer-GGUF/resolve/df802a6773d25ba6ef785ff619daa3e510503168/GaboxR67/MelBandRoformers/melbandroformers/vocals/voc_fv6-Q4_0.gguf", dest_rel: "models/bsroformer/voc_fv6-Q4_0.gguf", size: 139_168_096, sha256: "11441e362f9815f4f06b1f4dea8c4b33cafae8083aefb7b9e10e43fe9c2841a1", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/bsroformer/voc_fv6-Q4_0.gguf", expect: 139_168_096 }],
            external_url: None,
        },
        // ── КАСТИНГ ПЕРСОНАЖЕЙ (#115): лица + голос ──────────────────────────
        // Полный набор моделей кастинга. Резолвятся dub_faces (<models>/faces/…). ВСЕ из первоисточников
        // (сверено по точному размеру файла). Без них кастинг мёртв: «лиц привязано 0», все спикеры SPK0.
        //   • det_10g (SCRFD-10GF) — детектор реальных лиц (insightface buffalo_l → immich-app/buffalo_l).
        //   • LVFace-L_Glint360K — эмбеддер реального лица (bytedance-research/LVFace).
        //   • ccip model_feat — эмбеддер нарисованного персонажа (deepghs/ccip_onnx, caformer-24).
        //   • anime_face model — детектор аниме/мульт-лиц (deepghs/anime_face_detection v1.4_s).
        //   • xseg_1 — окклюдер лица для чистого аватар-кропа (facefusion/models-3.1.0).
        //   • WeSpeaker ResNet34-LM — голосовой эмбеддинг: cross-episode матч голосов (на Xet-CAS).
        Component {
            id: "casting",
            name: t!("setup-comp-casting-name"),
            purpose: t!("setup-comp-casting-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: 1_331_548_084,
            files: &[
                FileSpec { url: "https://huggingface.co/immich-app/buffalo_l/resolve/d09715916a0778919a770c343533641e250b8699/detection/model.onnx", dest_rel: "models/faces/det_10g.onnx", size: 16_923_827, sha256: "5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/bytedance-research/LVFace/resolve/b12702ab1f5c721748e054a66dc90e1edd1f0724/LVFace-L_Glint360K/LVFace-L_Glint360K.onnx", dest_rel: "models/faces/LVFace-L_Glint360K.onnx", size: 1_022_938_188, sha256: "49389036a4a5b69e0efcddfe34839ac72c7a71ce6b4dc1b6821e2ac368c87063", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx", dest_rel: "models/faces/ccip/model_feat.onnx", size: 150_248_245, sha256: "4ea118d16496274f4f6e08d3afc768cc592389e8f7f32f8732ce2215c228ac5f", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx", dest_rel: "models/faces/anime_face/model.onnx", size: 44_583_229, sha256: "403b5bc93b6ff789b7d183418df4a1364049bac00c24acd927604a7ff6891483", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/facefusion/models-3.1.0/resolve/c9e3a503d8e84e91c5cd89ee2d510fe5e793e570/xseg_1.onnx", dest_rel: "models/faces/occluder/xseg_1.onnx", size: 70_324_286, sha256: "c4d1498b8a03b5fe2a3a5d2ef2a0402ab03bd51edaf5b2d8d5fb764702a97dd3", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/f0c48c298fd835726c27956a5d617bad7115627e/voxceleb_resnet34_LM.onnx", dest_rel: "models/faces/wespeaker/voxceleb_resnet34_LM.onnx", size: 26_530_309, sha256: "7bb2f06e9df17cdf1ef14ee8a15ab08ed28e8d0ef5054ee135741560df2ec068", extract: Extract::None },
            ],
            markers: &[
                Marker { rel: "models/faces/det_10g.onnx", expect: 16_923_827 },
                Marker { rel: "models/faces/LVFace-L_Glint360K.onnx", expect: 1_022_938_188 },
                Marker { rel: "models/faces/ccip/model_feat.onnx", expect: 150_248_245 },
                Marker { rel: "models/faces/anime_face/model.onnx", expect: 44_583_229 },
                Marker { rel: "models/faces/occluder/xseg_1.onnx", expect: 70_324_286 },
                Marker { rel: "models/faces/wespeaker/voxceleb_resnet34_LM.onnx", expect: 26_530_309 },
            ],
            external_url: None,
        },
        // ── СAЙДКАРЫ / ДВИЖКИ ───────────────────────────────────────────────
        Component {
            id: "bsroformer-engine",
            name: t!("setup-comp-bsroformer-engine-name"),
            purpose: t!("setup-comp-bsroformer-engine-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: platform::BSROFORMER.size,
            files: platform::BSROFORMER.files,
            markers: platform::BSROFORMER.markers,
            external_url: None,
        },
        Component {
            id: "bsroformer-engine-cpu",
            name: t!("setup-comp-bsroformer-engine-cpu-name"),
            purpose: t!("setup-comp-bsroformer-engine-cpu-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::BSROFORMER_CPU.size,
            files: platform::BSROFORMER_CPU.files,
            markers: platform::BSROFORMER_CPU.markers,
            external_url: None,
        },
        Component {
            id: "bsroformer-engine-vulkan",
            name: format!("{} · Vulkan", t!("setup-comp-bsroformer-engine-name")),
            purpose: t!("setup-comp-bsroformer-engine-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::BSROFORMER_VULKAN.size,
            files: platform::BSROFORMER_VULKAN.files,
            markers: platform::BSROFORMER_VULKAN.markers,
            external_url: None,
        },
        Component {
            id: "llama",
            name: t!("setup-comp-llama-name"),
            purpose: t!("setup-comp-llama-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            // Размер сжатого zip (для прогресса закачки); распакованный footprint ~183 МБ.
            size: llama_parts(llama_build()).size,
            files: llama_parts(llama_build()).files,
            markers: llama_parts(llama_build()).markers,
            external_url: None,
        },
        Component {
            id: "onnxruntime",
            name: "ONNX Runtime 1.28.2".into(),
            purpose: t!("setup-comp-onnxruntime-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: platform::ORT.size,
            files: platform::ORT.files,
            markers: platform::ORT.markers,
            external_url: None,
        },
        Component {
            id: "onnxruntime-gpu",
            name: "ONNX Runtime 1.28.2 GPU (CUDA)".into(),
            purpose: t!("setup-comp-onnxruntime-gpu-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: platform::ORT_GPU.size,
            files: platform::ORT_GPU.files,
            markers: platform::ORT_GPU.markers,
            external_url: None,
        },
        Component {
            id: "ffmpeg",
            name: "FFmpeg (static)".into(),
            purpose: t!("setup-comp-ffmpeg-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: platform::FFMPEG.size,
            files: platform::FFMPEG.files,
            markers: platform::FFMPEG.markers,
            external_url: None,
        },
        Component {
            id: "ytdlp",
            name: t!("setup-comp-ytdlp-name"),
            purpose: t!("setup-comp-ytdlp-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::YTDLP.size,
            files: platform::YTDLP.files,
            markers: platform::YTDLP.markers,
            external_url: None,
        },
        // ── СИСТЕМНОЕ ────────────────────────────────────────────────────────
        Component {
            id: "cuda-runtime",
            name: "CUDA 13 runtime (cudart + cuBLAS + cuFFT)".into(),
            purpose: t!("setup-comp-cuda-runtime-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: platform::CUDA_RUNTIME.size,
            files: platform::CUDA_RUNTIME.files,
            markers: platform::CUDA_RUNTIME.markers,
            external_url: None,
        },
        Component {
            id: "cudnn",
            name: "cuDNN 9 (CUDA 13)".into(),
            purpose: t!("setup-comp-cudnn-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Download,
            size: platform::CUDNN.size,
            files: platform::CUDNN.files,
            markers: platform::CUDNN.markers,
            external_url: None,
        },
        Component {
            id: "vcruntime",
            name: "Visual C++ Runtime (2015–2022)".into(),
            purpose: t!("setup-comp-vcruntime-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Bundled,
            size: 1_120_664,
            files: &[],
            // MSVCP140_1.dll импортирует onnxruntime 1.28 (ASR/диаризация/OCR на CPU) — без него на машине без
            // VC++ Redistributable рантайм не грузится (scripts/check-dll-imports.ps1).
            markers: &[
                Marker { rel: "models/higgs-engine/MSVCP140.dll", expect: 0 },
                Marker { rel: "models/higgs-engine/MSVCP140_1.dll", expect: 0 },
                Marker { rel: "models/higgs-engine/VCRUNTIME140.dll", expect: 0 },
                Marker { rel: "models/higgs-engine/VCRUNTIME140_1.dll", expect: 0 },
                Marker { rel: "models/higgs-engine/VCOMP140.DLL", expect: 0 },
            ],
            external_url: None,
        },
        Component {
            id: "ocr",
            name: t!("setup-comp-ocr-name"),
            purpose: t!("setup-comp-ocr-purpose"),
            requirement: Requirement::Recommended,
            delivery: Delivery::Bundled,
            size: 31_726_193,
            files: &[],
            markers: &[
                Marker { rel: "models/ocr/det.onnx", expect: 0 },
                Marker { rel: "models/ocr/cls.onnx", expect: 0 },
                Marker { rel: "models/ocr/rec_cyrillic.onnx", expect: 0 },
                Marker { rel: "models/ocr/rec_cyrillic.dict.txt", expect: 0 },
            ],
            external_url: None,
        },
        Component {
            id: "nvidia-driver",
            name: t!("setup-comp-nvidia-driver-name"),
            purpose: t!("setup-comp-nvidia-driver-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::External,
            size: 0,
            files: &[],
            markers: &[],
            external_url: Some(NVIDIA_DRIVER_URL),
        },
    ];
    // Среда исполнения Visual C++ есть только у Windows-сборок движков.
    if !cfg!(windows) {
        all.retain(|c| c.id != "vcruntime");
    }
    // Drama 3 is a CUDA engine: a Mac has none to run it
    if cfg!(target_os = "macos") {
        all.retain(|c| c.id != "drama3");
    }
    // the server's other builds this computer can be set to, fetched when a stage is set to their device: the card's
    // CUDA build, Vulkan and the processor (none on a Mac, whose one build is the server component)
    let card = crate::acpp::build_for("gpu").ok();
    for build in [crate::acpp::Build::Cuda13, crate::acpp::Build::Cuda12, crate::acpp::Build::Vulkan, crate::acpp::Build::Cpu] {
        let cuda = matches!(build, crate::acpp::Build::Cuda13 | crate::acpp::Build::Cuda12);
        if build == crate::acpp::build() || (cuda && Some(build) != card) {
            continue;
        }
        let label = match build {
            crate::acpp::Build::Cuda13 => "CUDA 13",
            crate::acpp::Build::Cuda12 => "CUDA 12.9",
            crate::acpp::Build::Vulkan => "Vulkan",
            crate::acpp::Build::Cpu => "CPU",
            crate::acpp::Build::Metal => "Metal",
        };
        let parts = platform::server_parts(build);
        all.push(Component {
            id: crate::acpp::component(build),
            name: t!("setup-comp-audiocpp-build-name", build = label),
            purpose: t!("setup-comp-audiocpp-build-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: parts.size,
            files: parts.files,
            markers: parts.markers,
            external_url: None,
        });
    }
    // Qwen3-TTS's backend modules, fetched when voicing is set to their device: Vulkan, and the CUDA one of this
    // computer's card (macOS: Metal is in the engine itself, and these have no files there)
    all.push(Component {
        id: "qwen3-tts-vulkan",
        name: format!("{} · Vulkan", t!("setup-comp-qwen3-tts-engine-name")),
        purpose: t!("setup-comp-audiocpp-build-purpose"),
        requirement: Requirement::Optional,
        delivery: Delivery::Download,
        size: platform::QWEN_VULKAN.size,
        files: platform::QWEN_VULKAN.files,
        markers: platform::QWEN_VULKAN.markers,
        external_url: None,
    });
    let qwen_cuda = match card {
        Some(crate::acpp::Build::Cuda13) => Some(("CUDA 13", &platform::QWEN_CUDA13)),
        Some(crate::acpp::Build::Cuda12) => Some(("CUDA 12.9", &platform::QWEN_CUDA12)),
        _ => None,
    };
    if let Some((label, parts)) = qwen_cuda {
        all.push(Component {
            id: "qwen3-tts-cuda",
            name: format!("{} · {label}", t!("setup-comp-qwen3-tts-engine-name")),
            purpose: t!("setup-comp-audiocpp-build-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: parts.size,
            files: parts.files,
            markers: parts.markers,
            external_url: None,
        });
    }
    if crate::acpp::cuda_build() == crate::hw::CudaBuild::Cuda12 {
        all.push(Component {
            id: "cuda12-runtime",
            name: "CUDA 12.9 runtime (cudart + cuBLAS + cuFFT)".into(),
            purpose: t!("setup-comp-cuda-runtime-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: platform::CUDA12_RUNTIME.size,
            files: platform::CUDA12_RUNTIME.files,
            markers: platform::CUDA12_RUNTIME.markers,
            external_url: None,
        });
        // the separation engine's CUDA 12.9 build, for the card that takes it
        all.push(Component {
            id: "bsroformer-engine-cuda12",
            name: format!("{} · CUDA 12.9", t!("setup-comp-bsroformer-engine-name")),
            purpose: t!("setup-comp-bsroformer-engine-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::BSROFORMER_CUDA12.size,
            files: platform::BSROFORMER_CUDA12.files,
            markers: platform::BSROFORMER_CUDA12.markers,
            external_url: None,
        });
    }
    // a build this platform has not got (a Mac's separation engine is the processor's alone)
    all.retain(|c| c.delivery != Delivery::Download || !c.files.is_empty());
    all
}

// ── Статус одного компонента ────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub requirement: Requirement,
    pub delivery: Delivery,
    /// Размер закачки компонента, байт (сумма files).
    pub size: u64,
    /// External — драйвер и видеокарта годятся под CUDA 13; Download — все files закреплённой версии на месте
    /// (прямые файлы точного размера, архивы с записью об установке) и маркеры целы.
    pub installed: bool,
    /// Сколько байт закачки уже на диске: целые файлы, принятые архивы и докачанные куски .part.
    pub bytes_on_disk: u64,
    /// Сколько места на томе моделей нужно, чтобы докачать компонент (остаток + распаковка архивов).
    pub space_needed: u64,
    /// Чего не хватает (пути относительно repo_root).
    pub missing: Vec<String>,
    /// External — видеокарта, драйвер, CUDA и compute capability; ffmpeg — "PATH", если взят оттуда;
    /// vcruntime — "system", если часть DLL даёт системный каталог.
    pub detail: Option<String>,
    /// URL внешней страницы (драйвер).
    pub external_url: Option<String>,
    /// Оценка VRAM при загрузке модели, байт (0 для движков/рантаймов без весов).
    pub vram: u64,
    /// Влезает ли модель в видеопамять карты; None — модель без весов, карты нет или стадии идут на CPU.
    pub fits_vram: Option<bool>,
    /// Id программы (yue2, minimax, dungeon...), у которой компонент взят в этом запуске, вместо закачки.
    pub taken_from: Option<String>,
}

/// Оценка VRAM загруженной модели по id (движки/рантаймы = 0). Грубо, для показа в UI.
fn vram_estimate(id: &str) -> u64 {
    let gb = |g: f64| (g * 1024.0 * 1024.0 * 1024.0) as u64;
    match id {
        "higgs" => gb(5.6),
        "higgs-q6_k" => gb(5.1),
        "higgs-q4_k_m" => gb(4.2),
        // наименьшая карта, на которой Drama 3 идёт с экспертами в оперативной памяти; целиком модель занимает 22,6
        "drama3" => gb(5.0),
        "qwen3-tts" => gb(2.8),
        "qwen3-tts-q8_0" => gb(3.7),
        "qwen3-tts-ru-stress" => gb(0.1),
        "supertonic3" => gb(0.6),
        "gemma" => gb(8.5),
        "gemma-q5_0" => gb(9.9),
        "gemma-q6_k" => gb(11.2),
        "gemma-q8_0" => gb(14.0),
        "parakeet" => gb(1.1),
        "parakeet-ultra" => gb(1.1),

        "sortformer" => gb(0.5),
        "roformer" => gb(0.5),
        "roformer-q5" => gb(0.45),
        "roformer-q4" => gb(0.4),
        _ => 0,
    }
}

/// Компонент — модель (весам нужна видеопамять), а не движок, рантайм или инструмент.
pub fn is_model(id: &str) -> bool {
    vram_estimate(id) > 0
}

/// Файл на месте и (если размер задан) ровно этого размера. Ревизии закреплены, поэтому допуска нет:
/// другой размер — другой или оборванный файл.
fn file_ok(path: &Path, size: u64) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && (size == 0 || m.len() == size))
        .unwrap_or(false)
}

fn marker_ok(repo_root: &Path, m: &Marker) -> bool {
    file_ok(&repo_root.join(m.rel), m.expect)
}

/// Компоненты, чьи DLL годятся и из системного каталога: VC++ Redistributable кладёт их в System32, а
/// загрузчик ищет зависимости движков там раньше PATH (где models/higgs-engine).
const FOUND_IN_SYSTEM_DIR: &[&str] = &["vcruntime"];

/// Маркер-DLL лежит в системном каталоге.
fn in_system_dir(system: &Path, m: &Marker) -> bool {
    Path::new(m.rel).file_name().is_some_and(|name| file_ok(&system.join(name), m.expect))
}

#[cfg(windows)]
fn system_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let mut buf = [0u16; 1024];
    let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        tracing::warn!("GetSystemDirectoryW did not answer ({}); DLLs of the system folder are not counted", std::io::Error::last_os_error());
        return None;
    }
    Some(PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])))
}

#[cfg(not(windows))]
fn system_dir() -> Option<PathBuf> {
    None
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// Каталог архивов на время закачки — на томе моделей: не забивает системный диск и переживает чистку %TEMP%.
fn download_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("models").join(".download")
}

/// Куда качается файл до проверки: прямой — рядом с финальным (публикация = rename на том же томе), архив —
/// в каталог закачек.
fn part_path(repo_root: &Path, f: &FileSpec) -> PathBuf {
    match f.extract {
        Extract::None => with_suffix(&repo_root.join(f.dest_rel), ".part"),
        _ => download_dir(repo_root).join(format!("{}.part", f.dest_rel.replace('/', "_"))),
    }
}

/// Манифест завершённых чанков рядом с .part: офсеты готовых чанков (u64 LE) для докачки.
fn done_manifest_path(part: &Path) -> PathBuf {
    with_suffix(part, ".done")
}

/// Офсеты готовых чанков из манифеста (нет манифеста — ничего не готово).
fn completed_offsets(part: &Path, total: u64) -> std::collections::HashSet<u64> {
    std::fs::read(done_manifest_path(part))
        .map(|b| {
            b.as_chunks::<8>() // рваный хвост (<8 байт при килле) отбрасывается
                .0
                .iter()
                .map(|c| u64::from_le_bytes(*c))
                .filter(|off| *off < total && off % CHUNK == 0)
                .collect()
        })
        .unwrap_or_default()
}

/// Сколько байт файла уже докачано в .part (по манифесту готовых чанков).
fn resumed_bytes(part: &Path, total: u64) -> u64 {
    if total == 0 || file_len(part) != total {
        return 0;
    }
    completed_offsets(part, total).iter().map(|off| CHUNK.min(total - off)).sum()
}

// ── Запись об установке архива ───────────────────────────────────────────────

#[derive(Serialize, serde::Deserialize)]
pub(crate) struct ArchiveRecord {
    pub(crate) sha256: String,
    pub(crate) url: String,
    files: Vec<RecordedFile>,
}

#[derive(Serialize, serde::Deserialize)]
struct RecordedFile {
    /// Относительно repo_root, через '/'.
    path: String,
    size: u64,
}

fn record_path(repo_root: &Path, f: &FileSpec) -> PathBuf {
    with_suffix(&repo_root.join(f.dest_rel), ".json")
}

pub(crate) fn read_record(repo_root: &Path, f: &FileSpec) -> Option<ArchiveRecord> {
    let p = record_path(repo_root, f);
    let text = std::fs::read_to_string(&p).ok()?;
    match serde_json::from_str(&text) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!("the install record {} is unreadable ({e}); the archive counts as not installed", p.display());
            None
        }
    }
}

/// Архив установлен, когда его запись называет закреплённый sha256 и всё, что он положил, на месте того же
/// размера. Так видна и замена версии при тех же именах файлов: cudart64_13.dll из CUDA 13.3 и 13.4 одного
/// размера, различает их только sha256 архива.
fn archive_installed(repo_root: &Path, f: &FileSpec) -> bool {
    read_record(repo_root, f).is_some_and(|r| {
        r.sha256 == f.sha256
            && !r.files.is_empty()
            && r.files.iter().all(|x| file_ok(&repo_root.join(&x.path), x.size))
    })
}

/// A program in one file (engine, CLI): builds with other flags can match in size, only sha256 tells them apart.
fn is_program(f: &FileSpec) -> bool {
    f.extract == Extract::None && (f.dest_rel.starts_with("tools/") || [".dll", ".so", ".dylib", ".exe"].iter().any(|ext| f.dest_rel.ends_with(ext)))
}

/// A program counts when it has the pinned size and its record names the pinned sha256. A file without a record
/// (laid down before records or taken from a neighbouring program) is hashed once and the record keeps its real
/// sha256, so a foreign build stays not installed and is not hashed again.
fn program_installed(repo_root: &Path, f: &FileSpec) -> bool {
    let dest = repo_root.join(f.dest_rel);
    if !file_ok(&dest, f.size) {
        return false;
    }
    if let Some(r) = read_record(repo_root, f) {
        return r.sha256 == f.sha256;
    }
    match sha256_file(&dest, &|| false) {
        Ok(Some(h)) => {
            if h != f.sha256 {
                tracing::warn!("{} is not the pinned build (sha256 {h}, pinned {}); it counts as not installed", f.dest_rel, f.sha256);
            }
            if let Err(e) = write_record_of(repo_root, f, &h, &[dest]) {
                tracing::error!("[ERROR] the install record of {}: {e}", f.dest_rel);
                return false;
            }
            h == f.sha256
        }
        Ok(None) => false,
        Err(e) => {
            tracing::error!("[ERROR] {}: {e}", f.dest_rel);
            false
        }
    }
}

pub(crate) fn file_installed(repo_root: &Path, f: &FileSpec) -> bool {
    match f.extract {
        Extract::None if is_program(f) => program_installed(repo_root, f),
        Extract::None => file_ok(&repo_root.join(f.dest_rel), f.size),
        _ => archive_installed(repo_root, f),
    }
}

fn write_record(repo_root: &Path, f: &FileSpec, written: &[PathBuf]) -> Result<(), DlError> {
    write_record_of(repo_root, f, f.sha256, written)
}

/// The record with the sha256 the file on disk has, which is the pinned one unless the file is another build.
fn write_record_of(repo_root: &Path, f: &FileSpec, sha256: &str, written: &[PathBuf]) -> Result<(), DlError> {
    let files = written
        .iter()
        .map(|p| RecordedFile {
            path: p.strip_prefix(repo_root).unwrap_or(p).to_string_lossy().replace('\\', "/"),
            size: file_len(p),
        })
        .collect();
    let rec = ArchiveRecord { sha256: sha256.to_string(), url: f.url.to_string(), files,
    };
    let path = record_path(repo_root, f);
    let tmp = with_suffix(&path, ".tmp");
    let body = serde_json::to_vec_pretty(&rec).map_err(|e| DlError::new("io", t!("setup-install-record", error = e.to_string())))?;
    std::fs::write(&tmp, body).map_err(|e| DlError::new("io", t!("common-write", what = tmp.display().to_string(), error = e.to_string())))?;
    std::fs::rename(&tmp, &path).map_err(|e| DlError::new("io", t!("common-write", what = path.display().to_string(), error = e.to_string())))
}

/// ffmpeg и ffprobe доступны в системном PATH и ffmpeg умеет то, что зовёт студия? ffmpeg без ffprobe рядом
/// валит анализ на первом же замере длительности, а старый (до 4.4) — сведение: его amix не знает `normalize`.
pub(crate) fn ffmpeg_on_path() -> bool {
    static ON_PATH: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON_PATH.get_or_init(|| {
        let names: &[&str] = if cfg!(windows) { &["ffmpeg.exe", "ffprobe.exe"] } else { &["ffmpeg", "ffprobe"] };
        let Some(paths) = std::env::var_os("PATH") else { return false };
        let dirs: Vec<PathBuf> = std::env::split_paths(&paths).collect();
        let found: Vec<PathBuf> = names.iter().filter_map(|name| dirs.iter().map(|dir| dir.join(name)).find(|path| path.is_file())).collect();
        if found.len() != names.len() {
            return false;
        }
        let help = dub_core::proc::output(dub_core::proc::host_command(&found[0]).args(["-hide_banner", "-h", "filter=amix"]));
        let mixes = help.is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("normalize"));
        if !mixes {
            tracing::warn!("{} is too old for the studio (its amix has no normalize): the studio's own ffmpeg is needed", found[0].display());
        }
        mixes
    })
}

/// Место под докачку одного файла: чего ещё не занял его .part (под докачку по Range он сразу занимает полный
/// размер) плюс, для архива, двойной его размер под распаковку.
fn file_space_needed(repo_root: &Path, f: &FileSpec) -> u64 {
    if file_installed(repo_root, f) {
        return 0;
    }
    let rest = f.size - file_len(&part_path(repo_root, f)).min(f.size);
    if f.extract == Extract::None { rest } else { rest + 2 * f.size }
}

/// Статус компонента на диске.
pub fn component_status(repo_root: &Path, c: &Component) -> ComponentStatus {
    let system = if FOUND_IN_SYSTEM_DIR.contains(&c.id) { system_dir() } else { None };
    status_with_system_dir(repo_root, c, system.as_deref())
}

/// `system` — системный каталог, где засчитываются маркеры компонентов FOUND_IN_SYSTEM_DIR.
fn status_with_system_dir(repo_root: &Path, c: &Component, system: Option<&Path>,
) -> ComponentStatus {
    let mut missing: Vec<String> = Vec::new();
    let mut bytes_on_disk = 0u64;
    let mut space_needed = 0u64;
    for f in c.files {
        if file_installed(repo_root, f) {
            bytes_on_disk += f.size;
        } else {
            missing.push(f.dest_rel.to_string());
            bytes_on_disk += resumed_bytes(&part_path(repo_root, f), f.size);
            space_needed += file_space_needed(repo_root, f);
        }
    }
    let mut from_system = false;
    for m in c.markers {
        if marker_ok(repo_root, m) || missing.iter().any(|x| x == m.rel) {
            continue;
        }
        if system.is_some_and(|dir| in_system_dir(dir, m)) {
            from_system = true;
        } else {
            missing.push(m.rel.to_string());
        }
    }
    if c.delivery == Delivery::Bundled {
        bytes_on_disk = c.markers.iter().map(|m| file_len(&repo_root.join(m.rel))).sum();
    }
    let (installed, detail) = match c.delivery {
        Delivery::External => {
            let gpu = crate::hw::gpu_report();
            (gpu.cuda13_ok, gpu.summary())
        }
        _ if c.id == "ffmpeg" && !missing.is_empty() && ffmpeg_on_path() => {
            // ffmpeg из PATH годится пайплайну (Command::new("ffmpeg") его найдёт) — закачку не навязываем.
            missing.clear();
            space_needed = 0;
            (true, Some("PATH".to_string()))
        }
        _ => (missing.is_empty(), from_system.then(|| "system".to_string()),
        ),
    };
    ComponentStatus {
        id: c.id.to_string(),
        name: c.name.to_string(),
        purpose: c.purpose.to_string(),
        requirement: c.requirement,
        delivery: c.delivery,
        size: c.size,
        installed,
        bytes_on_disk,
        space_needed,
        missing,
        detail,
        external_url: c.external_url.map(|s| s.to_string()),
        vram: vram_estimate(c.id),
        fits_vram: None,
        taken_from: if installed { lock(taken()).get(c.id).cloned() } else { None },
    }
}

// ── Импорт готовых моделей из выбранной папки ────────────────────────────────

/// Рекурсивно собрать карту basename(lower) -> [(path, size)] под dir (лимит глубины/файлов, чтоб не уйти в
/// бесконечность на большом диске). Скрытые каталоги (.git, .download, .cache) пропускаем.
fn index_dir(dir: &Path, map: &mut std::collections::HashMap<String, Vec<(PathBuf, u64)>>, depth: usize, budget: &mut usize,
) {
    if depth > 8 || *budget == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return;
    };
    for e in rd.flatten() {
        if *budget == 0 {
            return;
        }
        let p = e.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()).map(str::to_string) else { continue;
        };
        if p.is_dir() {
            if !name.starts_with('.') {
                index_dir(&p, map, depth + 1, budget);
            }
        } else {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            map.entry(name.to_lowercase()).or_default().push((p, size));
            *budget -= 1;
        }
    }
}

/// Итог импорта: компоненты, ставшие установленными, число положенных файлов и ошибки по файлам.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub files: usize,
    pub errors: Vec<String>,
}

/// Положить src на место dest: жёсткая ссылка (тот же том — мгновенно и без второй копии гигабайт), иначе
/// копия через .part + rename (частичной копии под финальным именем не бывает). Без `copy` — только ссылка:
/// автоподхват у соседних программ не тратит гигабайты места молча. The file already at dest is replaced only by
/// a finished link or copy: a link that fails (another drive) leaves it as it was, and a folder made for it goes.
pub(crate) fn adopt_file(src: &Path, dest: &Path, copy: bool) -> std::io::Result<()> {
    let made = match dest.parent() {
        Some(parent) if !parent.exists() => {
            std::fs::create_dir_all(parent)?;
            Some(parent)
        }
        _ => None,
    };
    let adopted = (|| {
        let tmp = with_suffix(dest, ".import");
        if tmp.exists() {
            std::fs::remove_file(&tmp)?;
        }
        if let Err(e) = std::fs::hard_link(src, &tmp) {
            if !copy {
                return Err(e);
            }
            std::fs::copy(src, &tmp)?;
        }
        std::fs::rename(&tmp, dest).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    })();
    if adopted.is_err() {
        if let Some(parent) = made {
            if let Err(e) = std::fs::remove_dir(parent) {
                tracing::warn!("{} stays after a failed import: {e}", parent.display());
            }
        }
    }
    adopted
}

/// Импорт готовых весов из папки без закачки: для каждого недостающего ПРЯМОГО файла компонента ищем в src_dir
/// файл того же имени и ТОЧНО того же размера. Архивные компоненты (движки, CUDA) не импортируются: по россыпи
/// файлов не проверить, что это закреплённая версия, — их докачивает «Первый запуск».
pub fn import_from_dir(repo_root: &Path, src_dir: &Path, only: Option<&str>, copy: bool, same_folder: bool) -> ImportReport {
    let mut map = std::collections::HashMap::new();
    let mut budget = 200_000usize;
    index_dir(src_dir, &mut map, 0, &mut budget);
    let mut report = ImportReport::default();
    for c in manifest() {
        if c.delivery != Delivery::Download || only.is_some_and(|id| c.id != id) {
            continue;
        }
        let mut any = false;
        for f in c.files.iter().filter(|f| f.extract == Extract::None) {
            let dest = repo_root.join(f.dest_rel);
            if file_ok(&dest, f.size) {
                continue;
            }
            // у соседней программы — ещё и та же папка модели: config.json и словари совпадают по имени и размеру у
            // разных моделей, а контрольной суммы, которая отличила бы их, здесь нет
            let folder = dest.parent().and_then(Path::file_name);
            let in_place = |p: &Path| !same_folder || p.parent().and_then(Path::file_name) == folder;
            let src = match c.markers.iter().find(|m| m.rel == f.dest_rel) {
                Some(m) => pick_import_source(&c, m, &map).filter(|p| file_len(p) == f.size && in_place(p)),
                None => Path::new(f.dest_rel)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase())
                    .and_then(|base| map.get(&base))
                    .and_then(|cands| cands.iter().find(|(p, sz)| *sz == f.size && in_place(p)))
                    .map(|(p, _)| p),
            };
            let Some(src) = src else { continue };
            match adopt_file(src, &dest, copy) {
                Ok(()) => {
                    // the record of the build the file replaced would keep it uninstalled; without one the file is
                    // hashed once and recorded as what it is
                    if is_program(f) {
                        if let Err(e) = std::fs::remove_file(record_path(repo_root, f)) {
                            if e.kind() != std::io::ErrorKind::NotFound {
                                report.errors.push(format!("{}: {e}", record_path(repo_root, f).display()));
                            }
                        }
                    }
                    let part = part_path(repo_root, f);
                    let _ = std::fs::remove_file(done_manifest_path(&part));
                    let _ = std::fs::remove_file(&part);
                    report.files += 1;
                    any = true;
                }
                Err(e) => {
                    report.errors.push(format!("{} -> {}: {e}", src.display(), dest.display()))
                }
            }
        }
        if any && component_status(repo_root, &c).installed {
            report.imported.push(c.id.to_string());
        }
    }
    report
}

/// Компоненты, взятые в этом запуске у соседних программ: id → программа, для пометки в «Первом запуске».
fn taken() -> &'static Mutex<std::collections::BTreeMap<String, String>> {
    static TAKEN: OnceLock<Mutex<std::collections::BTreeMap<String, String>>> = OnceLock::new();
    TAKEN.get_or_init(Default::default)
}

/// A program's name by its id (CONTRACT.md, "Program ids").
pub fn program_name(id: &str) -> &str {
    match id {
        "yue2" => "YuE2 Studio",
        "minimax" => "MiniMax Music3 Studio",
        "ace" => "ACE-Step Studio",
        "dub" => "Dub Studio",
        "speech" => "Speech Studio",
        "dungeon" => "Dungeon Ultimate",
        "bulka" => "Bulka",
        "videosos" => "VideoSOS",
        other => other,
    }
}

/// Недостающие веса из папок соседних программ (YuE2, MiniMax, Dungeon...) тем же импортом, что и из папки
/// пользователя: имя файла и точный размер. При старте — только жёсткие ссылки (место не тратится); перед закачкой
/// и копией, если программа на другом диске, — это всё равно быстрее сети, но только для того, что человек качает
/// (`only`). Отдаёт (компонент, программа).
pub fn import_from_neighbors(repo_root: &Path, neighbors: &[(String, PathBuf)], copy: bool, only: Option<&[String]>) -> Vec<(String, String)> {
    let mut took = Vec::new();
    for (app, dir) in neighbors {
        let reports: Vec<ImportReport> = match only {
            None => vec![import_from_dir(repo_root, dir, None, copy, true)],
            Some(ids) => ids.iter().map(|id| import_from_dir(repo_root, dir, Some(id), copy, true)).collect(),
        };
        for id in reports.into_iter().flat_map(|report| report.imported) {
            lock(taken()).insert(id.clone(), app.clone());
            took.push((id, app.clone()));
        }
    }
    took
}

// ── Место на диске ───────────────────────────────────────────────────────────

/// Свободное место на томе пути (для пользователя, с учётом квот), байт. Путь может ещё не существовать —
/// берём ближайший существующий каталог. None — ОС не ответила: место тогда не проверяем.
#[cfg(windows)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let dir = path.ancestors().find(|p| p.is_dir())?;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut avail = 0u64;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, std::ptr::null_mut(), std::ptr::null_mut(),
        ) };
    (ok != 0).then_some(avail)
}

#[cfg(windows)]
extern "system" {
    fn GetDiskFreeSpaceExW(dir: *const u16, free_to_caller: *mut u64, total: *mut u64, total_free: *mut u64,
    ) -> i32;
    fn GetSystemDirectoryW(buf: *mut u16, size: u32) -> u32;
}

#[cfg(not(windows))]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let dir = path.ancestors().find(|p| p.is_dir())?;
    let c_path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } == 0;
    ok.then(|| stat.f_bavail as u64 * stat.f_frsize as u64)
}

/// Отказ, если на томе моделей меньше места, чем нужно под докачку `need` байт.
fn ensure_space(repo_root: &Path, need: u64) -> Result<(), DlError> {
    let models = repo_root.join("models");
    match free_bytes(&models) {
        Some(free) if free < need => Err(DlError::new(
            "disk_space",
            t!("setup-disk-space", need = format!("{:.1}", need as f64 / 1e9), free = format!("{:.1}", free as f64 / 1e9), path = models.display().to_string()),
        )),
        _ => Ok(()),
    }
}

/// Проверка места под докачку выбранных компонентов до старта (без хэширования — по размерам и записям).
pub fn check_space(repo_root: &Path, ids: &[String]) -> Result<(), DlError> {
    let need: u64 = manifest()
        .iter()
        .filter(|c| c.delivery == Delivery::Download && ids.iter().any(|x| x == c.id))
        .flat_map(|c| c.files.iter())
        .map(|f| file_space_needed(repo_root, f))
        .sum();
    ensure_space(repo_root, need)
}

/// Размер `sz` годится для маркера: неизвестный expect — любой, иначе в пределах ±3% от expect.
fn import_size_fits(m: &Marker, sz: u64) -> bool {
    m.expect == 0 || sz.abs_diff(m.expect).saturating_mul(100) <= m.expect.saturating_mul(3)
}

/// Исходный файл для маркера `m` компонента `c` из индекса `map` (basename в нижнем регистре -> файлы).
/// Кандидат — файл с тем же именем и подходящим размером (точный в приоритете). Каталог-источник
/// отбрасывается, если в нём лежит одноимённый файл ДРУГОГО маркера той же папки назначения с неподходящим
/// размером: это файлы другой модели. Так Parakeet fp32 и Parakeet Ultra (одинаковые имена, у
/// encoder-model.onnx.data ещё и одинаковый размер) не смешиваются в одной папке.
fn pick_import_source<'a>(
    c: &Component,
    m: &Marker,
    map: &'a std::collections::HashMap<String, Vec<(PathBuf, u64)>>,
) -> Option<&'a PathBuf> {
    let base_of = |rel: &str| {
        Path::new(rel).file_name().and_then(|s| s.to_str()).map(|s| s.to_lowercase())
    };
    let cands = map.get(&base_of(m.rel)?)?;
    let others: Vec<Marker> = manifest().into_iter().filter(|o| o.id != c.id).flat_map(|o| o.markers.iter().copied()).collect();
    let fits = |marker: &Marker, sz: u64| {
        import_size_fits(marker, sz)
            && !others.iter().any(|o| o.expect == sz && o.expect != marker.expect && base_of(o.rel) == base_of(marker.rel))
    };
    let dest_dir = Path::new(m.rel).parent();
    let siblings: Vec<&Marker> = c
        .markers
        .iter()
        .filter(|s| s.rel != m.rel && Path::new(s.rel).parent() == dest_dir)
        .collect();
    let dir_conflicts = |dir: Option<&Path>| {
        siblings.iter().any(|s| {
            base_of(s.rel)
                .and_then(|b| map.get(&b))
                .is_some_and(|files| {
                    files.iter().any(|(p, sz)| p.parent() == dir && !fits(s, *sz))
                })
        })
    };
    let ok: Vec<&(PathBuf, u64)> = cands
        .iter()
        .filter(|(p, sz)| fits(m, *sz) && !dir_conflicts(p.parent()))
        .collect();
    ok.iter()
        .find(|(_, sz)| m.expect != 0 && *sz == m.expect)
        .or_else(|| ok.first())
        .map(|(p, _)| p)
}

// ── Полный статус (для GET /setup/status) ────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub components: Vec<ComponentStatus>,
    /// Всё ли обязательное на месте (тогда фронт показывает обычный hero, а не «первый запуск»).
    pub ready: bool,
    /// Совокупный размер того, что ещё надо скачать (обязательное+рекомендованное, что missing и Download).
    pub download_pending: u64,
    /// Драйвер NVIDIA найден (nvcuda.dll грузится); годится ли он под CUDA 13 — в gpu.
    pub driver_ok: bool,
    /// build-строки для диагностики (llama-билд и т.п.).
    pub llama_build: String,
    /// Why Gemma runs another build than the card's (a CUDA build the system cannot start); None when it does not.
    pub llama_note: Option<String>,
    /// Папка моделей (куда кладёт «Первый запуск»).
    pub models_dir: String,
    /// Свободно на томе моделей, байт; None — ОС не ответила, место не проверяется.
    pub free_bytes: Option<u64>,
    /// Видеокарта и драйвер против требований CUDA 13.
    pub gpu: crate::hw::GpuReport,
    /// Фоновая закачка (идёт, на паузе, прервана перезапуском, упала или завершилась). Заполняет обработчик.
    pub active: Option<crate::downloads::DownloadJob>,
}

/// Запас видеопамяти под рабочий стол и окна поверх оценки модели.
const VRAM_HEADROOM: u64 = 512 * 1024 * 1024;

/// Модели по видеопамяти карты (`total`, байт; 0 — неизвестна): каждой отмечено, влезет ли она, а в группе
/// квантов одной модели (`higgs`, `higgs-q6_k`, ...) обязательным становится самый большой влезающий вариант.
/// Группа, где не влезает ничего, готовность не держит: эта стадия идёт на сервере или в облаке. Группу, где
/// вариант уже скачан, не трогает: выбор сделан.
fn fit_to_vram(comps: &mut [ComponentStatus], total: u64) {
    if total == 0 {
        return;
    }
    let group = |id: &str| id.split('-').next().unwrap_or(id).to_string();
    for c in comps.iter_mut().filter(|c| c.vram > 0) {
        c.fits_vram = Some(c.vram + VRAM_HEADROOM <= total);
    }
    let groups: std::collections::BTreeSet<String> = comps
        .iter()
        .filter(|c| c.vram > 0 && c.requirement == Requirement::Required && c.fits_vram == Some(false))
        .map(|c| group(&c.id))
        .filter(|g| !comps.iter().any(|c| c.vram > 0 && c.installed && group(&c.id) == *g))
        .collect();
    for g in groups {
        let best = comps
            .iter()
            .filter(|c| c.vram > 0 && group(&c.id) == g && c.fits_vram == Some(true))
            .max_by_key(|c| c.vram)
            .map(|c| c.id.clone());
        for c in comps.iter_mut().filter(|c| c.vram > 0 && group(&c.id) == g) {
            if Some(&c.id) == best.as_ref() {
                c.requirement = Requirement::Required;
            } else if c.requirement == Requirement::Required {
                c.requirement = Requirement::Optional;
            }
        }
    }
}

/// Варианты одной модели (`parakeet`, `parakeet-ultra`, ...) взаимозаменяемы: скачанный вариант закрывает
/// группу, и обязательный по умолчанию больше не держит готовность — иначе выбор юзера на первом запуске
/// упирается в докачку варианта, который он не выбирал.
fn installed_variant_covers_group(comps: &mut [ComponentStatus]) {
    let group = |id: &str| id.split('-').next().unwrap_or(id).to_string();
    let covered: std::collections::BTreeSet<String> =
        comps.iter().filter(|c| c.vram > 0 && c.installed).map(|c| group(&c.id)).collect();
    for c in comps.iter_mut() {
        if c.vram > 0 && !c.installed && c.requirement == Requirement::Required && covered.contains(&group(&c.id)) {
            c.requirement = Requirement::Optional;
        }
    }
}

pub fn setup_status(repo_root: &Path) -> SetupStatus {
    let mut comps: Vec<ComponentStatus> = manifest()
        .iter()
        .map(|c| component_status(repo_root, c))
        .collect();
    // Облачный пресет (OpenRouter) снимает ОБЯЗАТЕЛЬНОСТЬ тяжёлых локальных движков: если стадия перевода/
    // TTS идёт через облако (флаг + ключ), её локальную модель качать НЕ обязательно — не гейтит ready и не
    // преселектится на первом запуске (юзер выбрал облако -> не тянет ненужные гигабайты Gemma/Higgs).
    let mroot = repo_root.join("models");
    let cloud_llm = !crate::models::local_gemma_needed(&mroot);
    let cloud_tts = crate::models::cloud_tts_on(&mroot);
    let cloud_asr = crate::models::openrouter_asr_on(&mroot);
    for c in comps.iter_mut() {
        if c.requirement != Requirement::Required {
            continue;
        }
        let is_gemma = c.id.starts_with("gemma") || c.id == "llama";
        let is_higgs = c.id.starts_with("higgs");
        let is_asr = c.id.starts_with("parakeet") || c.id.starts_with("crisperwhisper") || c.id == "langid" || c.id.starts_with("transcribe-engine");
        if (cloud_llm && is_gemma) || (cloud_tts && is_higgs) || (cloud_asr && is_asr) {
            c.requirement = Requirement::Optional;
        }
    }
    // Backend локальных стадий: в CPU-режиме (без NVIDIA) CUDA-компоненты не нужны — сепарация идёт
    // CPU-сборкой, тяжёлое в облаке. CPU-движок сепарации становится рекомендованным (преселект на
    // первом запуске), а CUDA-движок + CUDA-рантайм — необязательными (не тянем лишние гигабайты).
    // В GPU-режиме наоборот: CPU-движок не нужен.
    let backend = crate::models::local_backend(&mroot);
    let asr_engine = crate::models::tcpp_build(&mroot).dir();
    let sep_engine = crate::models::sep_build(&mroot).component();
    for c in comps.iter_mut() {
        if !cloud_asr && ["transcribe-engine", "transcribe-engine-cuda12", "transcribe-engine-cpu"].contains(&c.id.as_str()) {
            c.requirement = if c.id == asr_engine { Requirement::Required } else { Requirement::Optional };
        }
        // the separation engine of the separation device is the recommended one, its other builds are optional
        if c.id.starts_with("bsroformer-engine") {
            c.requirement = if c.id == sep_engine { Requirement::Recommended } else { Requirement::Optional };
        } else if backend == "cpu" && (c.id == "cuda-runtime" || c.id == "onnxruntime-gpu" || c.id == "cudnn") {
            c.requirement = Requirement::Optional;
        }
    }
    if backend == "gpu" {
        fit_to_vram(&mut comps, crate::hw::total_vram());
    }
    installed_variant_covers_group(&mut comps);
    // ready = всё СКАЧИВАЕМОЕ/бандл-обязательное на месте. External (драйвер NVIDIA) НЕ гейтит: без NVIDIA
    // приложение работает на CPU/в облаке, а старый драйвер — предупреждение на экране, а не запертый вход.
    let ready = comps
        .iter()
        .filter(|c| c.requirement == Requirement::Required && c.delivery != Delivery::External)
        .all(|c| c.installed);
    let download_pending = comps
        .iter()
        .filter(|c| c.delivery == Delivery::Download && !c.installed)
        .map(|c| c.size.saturating_sub(c.bytes_on_disk))
        .sum();
    let gpu = crate::hw::gpu_report();
    SetupStatus {
        components: comps,
        ready,
        download_pending,
        driver_ok: gpu.nvidia,
        llama_build: GH_LLAMA_BUILD.to_string(),
        llama_note: llama_note(),
        models_dir: mroot.to_string_lossy().to_string(),
        free_bytes: free_bytes(&mroot),
        gpu,
        active: None,
    }
}

// ── Удаление компонентов ─────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalReport {
    pub removed: Vec<String>,
    pub freed_bytes: u64,
    /// Файлы, которые не удалось снять (например, заняты работающим движком).
    pub errors: Vec<String>,
}

/// Удалить скачанные компоненты и освободить место: прямые файлы, всё, что положили их архивы (по записи об
/// установке — в общих каталогах вроде models/higgs-engine снимается ровно своё), недокачанные .part/.done и
/// маркеры старой установки без записи. Выбор варианта в active.json, указывающий на удалённый квант,
/// снимается — резолв возьмёт установленный. Идущую закачку этих компонентов не трогаем (отказ «busy»).
pub fn remove_components(repo_root: &Path, ids: &[String]) -> Result<RemovalReport, DlError> {
    let all = manifest();
    let mut targets: Vec<&Component> = Vec::new();
    for id in ids {
        let c = all
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| DlError::new("unknown_component", t!("setup-unknown-component", id = id.to_string())))?;
        if c.delivery != Delivery::Download {
            return Err(DlError::new("not_removable", t!("setup-not-removable", id = id.to_string()),
            ));
        }
        targets.push(c);
    }
    let claim_ids: Vec<String> = targets.iter().map(|c| c.id.to_string()).collect();
    let Some(_claim) = try_claim(&claim_ids) else {
        return Err(DlError::new("busy", t!("setup-component-busy"),
        ));
    };
    let mut report = RemovalReport::default();
    let mroot = crate::models_root(repo_root);
    for c in targets {
        let before = report.freed_bytes;
        let mut touched: Vec<PathBuf> = Vec::new();
        let mut drop_file = |p: &Path, report: &mut RemovalReport| {
            let Ok(meta) = std::fs::metadata(p) else { return;
            };
            if !meta.is_file() {
                return;
            }
            match std::fs::remove_file(p) {
                Ok(()) => {
                    report.freed_bytes += meta.len();
                    touched.push(p.to_path_buf());
                }
                Err(e) => report.errors.push(format!("{}: {e}", p.display())),
            }
        };
        for f in c.files {
            let part = part_path(repo_root, f);
            drop_file(&part, &mut report);
            drop_file(&done_manifest_path(&part), &mut report);
            if f.extract == Extract::None {
                drop_file(&repo_root.join(f.dest_rel), &mut report);
                drop_file(&record_path(repo_root, f), &mut report);
            } else {
                if let Some(rec) = read_record(repo_root, f) {
                    for x in &rec.files {
                        drop_file(&repo_root.join(&x.path), &mut report);
                    }
                }
                drop_file(&record_path(repo_root, f), &mut report);
            }
        }
        for m in c.markers {
            drop_file(&repo_root.join(m.rel), &mut report);
        }
        prune_empty_dirs(repo_root, &touched);
        if report.freed_bytes > before {
            report.removed.push(c.id.to_string());
        }
        for (engine, variant) in crate::models::component_selection(c.id) {
            if engine == "asr_engine" {
                continue;
            }
            if let Err(e) = crate::models::clear_selection_if(&mroot, engine, &variant) {
                report.errors.push(format!("active.json ({engine}): {e}"));
            }
        }
    }
    Ok(report)
}

/// Снять опустевшие каталоги после удаления (вверх до models/ или tools/, сами они остаются).
fn prune_empty_dirs(repo_root: &Path, removed: &[PathBuf]) {
    let stops = [repo_root.join("models"), repo_root.join("tools"), repo_root.to_path_buf(),
    ];
    for p in removed {
        let mut dir = p.parent();
        while let Some(d) = dir {
            if stops.iter().any(|s| s == d) || !d.starts_with(repo_root) {
                break;
            }
            if std::fs::remove_dir(d).is_err() {
                break; // не пуст (или занят) — выше тем более не пуст
            }
            dir = d.parent();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  ЗАКАЧКА (фоновый менеджер downloads или on-demand догрузка внутри джобы)
// ═══════════════════════════════════════════════════════════════════════════

/// Колбэк прогресса скачивания: менеджер пишет его в своё состояние, джоба — в SSE.
pub type ProgressCb<'a> = dyn Fn(Value) + 'a;

/// Ошибка закачки: код для UI/агента и подробность для журнала.
#[derive(Clone, Debug)]
pub struct DlError {
    pub code: &'static str,
    pub detail: String,
}

impl DlError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        DlError { code, detail: detail.into(),
        }
    }
}

impl std::fmt::Display for DlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

/// Код «остановлено пользователем»: закачка на паузе, .part и манифесты чанков остаются для докачки.
pub const CANCELLED: &str = "cancelled";

fn cancelled() -> DlError {
    DlError::new(CANCELLED, t!("setup-paused"))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Занятые компоненты: один писатель на компонент ───────────────────────────
//
// Фоновая закачка и on-demand догрузка внутри джобы могут идти одновременно; два писателя в один .part
// испортили бы файл. Компонент «занят» на всё время закачки — вторая закачка того же компонента ждёт первую
// (и затем видит файл готовым), закачки разных компонентов идут параллельно в общем бюджете соединений.

struct Claims {
    ids: Mutex<std::collections::HashSet<String>>,
    freed: Condvar,
}

fn claims() -> &'static Claims {
    static C: OnceLock<Claims> = OnceLock::new();
    C.get_or_init(|| Claims { ids: Mutex::new(Default::default()), freed: Condvar::new(),
    })
}

struct ClaimGuard {
    ids: Vec<String>,
}

impl Drop for ClaimGuard {
    fn drop(&mut self) {
        let c = claims();
        let mut held = lock(&c.ids);
        for id in &self.ids {
            held.remove(id);
        }
        c.freed.notify_all();
    }
}

fn try_claim(ids: &[String]) -> Option<ClaimGuard> {
    let mut held = lock(&claims().ids);
    if ids.iter().any(|id| held.contains(id)) {
        return None;
    }
    held.extend(ids.iter().cloned());
    Some(ClaimGuard { ids: ids.to_vec() })
}

fn claim(ids: &[String], stop: &dyn Fn() -> bool, on_wait: &dyn Fn(),
) -> Result<ClaimGuard, DlError> {
    let mut told = false;
    loop {
        if let Some(g) = try_claim(ids) {
            return Ok(g);
        }
        if stop() {
            return Err(cancelled());
        }
        if !told {
            on_wait();
            told = true;
        }
        let c = claims();
        let held = lock(&c.ids);
        let _ = c.freed.wait_timeout(held, Duration::from_millis(500));
    }
}

// ── Бюджет соединений и ожидание сервера ─────────────────────────────────────

// Общий пул соединений на ВЕСЬ процесс: сколько бы закачек ни шло, одновременно открыто не больше POOL_SLOTS
// соединений. 4, не 16: HF Xet-CAS (cas-bridge.xethub.hf.co — туда уехали все альт-кванты) роняет соединения
// при высокой параллели, файл собирается с дырами. Слот держится всё время передачи тела, а не только запроса.
const POOL_SLOTS: usize = 4;
const CHUNK: u64 = 16 * 1024 * 1024; // 16МБ на задачу — балансирует очередь между большими и мелкими файлами
/// Попыток на один кусок/пробу при сетевом сбое (экспоненциальная пауза 0.5, 1, 2… с). Ожидание по 429/5xx
/// попытки не тратит.
const RETRIES: u32 = 8;
/// Сколько всего можно ждать сервер, отвечающий 429/5xx, прежде чем сдаться: лимиты HF сбрасываются минутами.
const RATE_LIMIT_BUDGET_S: u64 = 20 * 60;
/// Ниже стольких оставшихся запросов в окне RateLimit закачка сама ждёт сброса окна, а не упирается в 429.
const KEEP_IN_RESERVE: u64 = 25;

struct Slots {
    used: Mutex<usize>,
    freed: Condvar,
}

fn slots() -> &'static Slots {
    static S: OnceLock<Slots> = OnceLock::new();
    S.get_or_init(|| Slots { used: Mutex::new(0), freed: Condvar::new(),
    })
}

struct SlotGuard;

impl Drop for SlotGuard {
    fn drop(&mut self) {
        let s = slots();
        *lock(&s.used) -= 1;
        s.freed.notify_one();
    }
}

fn acquire_slot(stop: &dyn Fn() -> bool) -> Result<SlotGuard, DlError> {
    let s = slots();
    let mut used = lock(&s.used);
    loop {
        if *used < POOL_SLOTS {
            *used += 1;
            return Ok(SlotGuard);
        }
        if stop() {
            return Err(cancelled());
        }
        used = s.freed.wait_timeout(used, Duration::from_millis(200)).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// До какого момента все закачки процесса не шлют запросов (сервер попросил подождать).
fn hold_until() -> &'static Mutex<Option<Instant>> {
    static H: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(None))
}

/// Секунд, сколько закачка сейчас ждёт сервер (0 — не ждёт). Идёт в прогресс: очередь, а не зависание.
static WAITING: AtomicU64 = AtomicU64::new(0);

pub fn waiting_for_server() -> u64 {
    WAITING.load(Ordering::Relaxed)
}

fn hold_off(secs: u64) {
    let until = Instant::now() + Duration::from_secs(secs);
    let mut h = lock(hold_until());
    if h.is_none_or(|t| t < until) {
        *h = Some(until);
    }
}

fn wait_for_server(stop: &dyn Fn() -> bool) -> Result<(), DlError> {
    loop {
        let until = *lock(hold_until());
        match until {
            Some(t) if t > Instant::now() => {
                WAITING.store((t - Instant::now()).as_secs().max(1), Ordering::Relaxed);
                if stop() {
                    return Err(cancelled());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            _ => {
                WAITING.store(0, Ordering::Relaxed);
                return Ok(());
            }
        }
    }
}

fn sleep_or_stop(d: Duration, stop: &dyn Fn() -> bool) -> Result<(), DlError> {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if stop() {
            return Err(cancelled());
        }
        std::thread::sleep((end - Instant::now()).min(Duration::from_millis(100)));
    }
    Ok(())
}

/// Окно лимита из заголовка RateLimit (черновик IETF, так его шлёт HF): `"resolvers";r=2871;t=143` —
/// (запросов осталось, секунд до сброса).
fn parse_rate_limit(v: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        v.split(';')
            .filter_map(|p| p.trim().split_once('='))
            .find(|(k, _)| k.trim() == name)
            .and_then(|(_, x)| x.trim().parse::<u64>().ok())
    };
    Some((field("r")?, field("t").unwrap_or(300)))
}

fn header_str<'a>(h: &'a ureq::http::HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

/// Сколько ждать по ответу 429/5xx: Retry-After (секунды), иначе сброс окна RateLimit, иначе 30 с.
fn asked_to_wait(h: &ureq::http::HeaderMap) -> u64 {
    header_str(h, "retry-after")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .or_else(|| {
            header_str(h, "ratelimit").and_then(parse_rate_limit).map(|w| w.1)
        })
        .unwrap_or(30)
        .clamp(1, 310)
}

type Resp = ureq::http::Response<ureq::Body>;

/// Один GET со слотом из общего бюджета. 429/5xx — не ошибка, а очередь: ждём, сколько попросил сервер, не
/// тратя попыток; слот на время ожидания отпускается. Вернувшийся слот держать до конца чтения тела.
fn send(agent: &ureq::Agent, url: &str, range: Option<(u64, u64)>, stop: &dyn Fn() -> bool,
) -> Result<(Resp, SlotGuard), DlError> {
    let mut waited = 0u64;
    loop {
        wait_for_server(stop)?;
        let slot = acquire_slot(stop)?;
        let mut req = agent.get(url);
        if let Some((a, b)) = range {
            // Заглохшее соединение (Xet под нагрузкой) иначе висит вечно: кусок — не дольше 10 минут, потом повтор.
            req = req
                .header("Range", &format!("bytes={a}-{b}"))
                .config()
                .timeout_recv_body(Some(Duration::from_secs(600)))
                .build();
        }
        let resp = req.call().map_err(|e| DlError::new("network", format!("{url}: {e}")))?;
        let status = resp.status().as_u16();
        if status == 429 || (500..600).contains(&status) {
            let pause = asked_to_wait(resp.headers());
            drop(resp);
            drop(slot);
            if waited + pause > RATE_LIMIT_BUDGET_S {
                return Err(DlError::new(
                    "rate_limited",
                    t!("setup-rate-limited", url = url.to_string(), status = status, minutes = waited / 60),
                ));
            }
            waited += pause;
            hold_off(pause);
            continue;
        }
        if let Some((left, resets)) = header_str(resp.headers(), "ratelimit").and_then(parse_rate_limit) {
            if left < KEEP_IN_RESERVE {
                hold_off(resets.clamp(1, 310));
            }
        }
        return Ok((resp, slot));
    }
}

static DOWNLOADS_FORBIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// No component is downloaded in this process from now on: a download that would start fails with the code
/// `downloads_forbidden`. For tests, whose roots have none of the studio's models and must not fetch them.
pub fn forbid_downloads() {
    DOWNLOADS_FORBIDDEN.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Построить ureq-агента для закачки. Статусы 4xx/5xx читаем сами (429 и заголовки ожидания). Прокси из
/// active.json — все запросы через него; иначе Config::default берёт HTTP(S)_PROXY из env.
fn dl_agent() -> Result<ureq::Agent, DlError> {
    if DOWNLOADS_FORBIDDEN.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(DlError::new("downloads_forbidden", "downloads are forbidden in this process"));
    }
    let mut cfg = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        // certificates by the system's store, as reqwest does: a proxy or antivirus that re-signs HTTPS with its own
        // root installed in Windows is trusted there, not in the bundled webpki roots
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build());
    // Маршрут прокси для Hugging Face, откуда идут модели (свой / как в Windows / напрямую). ureq не спрашивает
    // прокси на каждый запрос, поэтому агент строится на каждую закачку — смена прокси действует со следующей.
    if let Some(url) = dub_llm::net::proxy_url_for("https://huggingface.co/") {
        cfg = cfg.proxy(Some(dl_proxy(&url).map_err(|e| DlError::new("proxy", e))?));
    }
    Ok(cfg.build().into())
}

/// ureq-прокси из адреса маршрута. ureq шлёт логин и пароль прокси ровно как они записаны в адресе, без
/// раскодирования %XX, поэтому они передаются раскодированными. Держит он их внутри своего адреса и делит по
/// последним `@` и `:`: пароль с `/ ? #`, пробелом или не-ASCII (любой прокси) или с `:` (SOCKS5) дошёл бы до
/// прокси другим — такой прокси для закачки ошибка с причиной, а не неверный пароль.
fn dl_proxy(url: &reqwest::Url) -> Result<ureq::Proxy, String> {
    use ureq::ProxyProtocol;
    let shown = dub_llm::net::masked(url.as_str());
    let protocol = match url.scheme() {
        "http" => ProxyProtocol::Http,
        "https" => ProxyProtocol::Https,
        "socks4" => ProxyProtocol::Socks4,
        "socks4a" => ProxyProtocol::Socks4A,
        "socks5" => ProxyProtocol::Socks5,
        "socks5h" => ProxyProtocol::Socks5h,
        other => return Err(t!("setup-proxy-scheme", proxy = shown.to_string(), scheme = other.to_string())),
    };
    let host = url.host_str().filter(|host| !host.is_empty()).ok_or_else(|| t!("setup-proxy-no-host", proxy = shown.to_string()))?;
    let port = url.port_or_known_default().ok_or_else(|| t!("setup-proxy-no-port", proxy = shown.to_string()))?;
    let user = dub_llm::net::decode_userinfo(url.username());
    let password = url.password().map(dub_llm::net::decode_userinfo);
    let mut builder = ureq::Proxy::builder(protocol).host(host).port(port);
    if !user.is_empty() || password.is_some() {
        builder = builder.username(&user);
    }
    if let Some(password) = &password {
        builder = builder.password(password);
    }
    let unfit = || t!("setup-proxy-credentials", proxy = shown.to_string());
    let proxy = builder.build().map_err(|e| format!("{} ({e})", unfit()))?;
    let sent_user = proxy.username().unwrap_or_default();
    let sent_password = proxy.password();
    let carried = match protocol {
        // CONNECT шлёт base64 от «логин:пароль»: прокси делит его по первому двоеточию.
        ProxyProtocol::Http | ProxyProtocol::Https => {
            format!("{sent_user}:{}", sent_password.unwrap_or_default()) == format!("{user}:{}", password.as_deref().unwrap_or_default())
        }
        _ => sent_user == user && sent_password == password.as_deref(),
    };
    if !carried || proxy.host() != host || proxy.port() != port {
        return Err(unfit());
    }
    Ok(proxy)
}

/// Размер файла + поддержка byte-range: 1-байтовый ranged-пробник. HF CDN (в т.ч. Xet-CAS) отдаёт 206 +
/// content-range на ureq-запрос (reqwest/curl-UA CAS душит 403). Сетевой сбой — повтор с паузой.
fn probe(agent: &ureq::Agent, url: &str, stop: &dyn Fn() -> bool) -> Result<(u64, bool), DlError> {
    let mut last = String::new();
    for attempt in 0..RETRIES {
        match probe_once(agent, url, stop) {
            Ok(v) => return Ok(v),
            Err(e) if e.code != "network" => return Err(e),
            Err(e) => last = e.detail,
        }
        sleep_or_stop(Duration::from_millis(500 << attempt.min(7)), stop)?;
    }
    Err(DlError::new("network", t!("setup-start-failed", url = url.to_string(), retries = RETRIES, error = last.clone()),
    ))
}

fn probe_once(agent: &ureq::Agent, url: &str, stop: &dyn Fn() -> bool,
) -> Result<(u64, bool), DlError> {
    let (resp, _slot) = send(agent, url, Some((0, 0)), stop)?;
    let status = resp.status().as_u16();
    let header_num = |name: &str, last_part: bool| {
        header_str(resp.headers(), name)
            .map(|s| {
                if last_part { s.rsplit('/').next().unwrap_or("") } else { s }
            })
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    if status == 206 {
        let total = header_num("content-range", true);
        if total > 0 {
            return Ok((total, true));
        }
    }
    if !(200..300).contains(&status) {
        return Err(DlError::new("http_status", t!("setup-http-status", url = url.to_string(), status = status),
        ));
    }
    Ok((header_num("content-length", false), false))
}

#[cfg(windows)]
fn write_at(f: &File, buf: &[u8], off: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    f.seek_write(buf, off)
}
#[cfg(not(windows))]
fn write_at(f: &File, buf: &[u8], off: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    f.write_at(buf, off)
}

/// Скачать ОДИН диапазон [start,end] в общий файл по офсету (write_at, без seek-гонок). HF Xet-CAS роняет
/// соединения под параллелью или отдаёт неполный range — это транзиентно, повтор спасает. downloaded —
/// счётчик прогресса; вклад неудачной попытки откатываем, чтобы повтор не задвоил прогресс. Принимается только
/// 206 + РОВНО (end-start+1) байт, иначе дыра в файле.
#[allow(clippy::too_many_arguments)]
fn download_range(
    agent: &ureq::Agent,
    url: &str,
    file: &Arc<File>,
    start: u64,
    end: u64,
    downloaded: &Arc<AtomicU64>,
    abort: &Arc<AtomicBool>,
    done: &Arc<Mutex<File>>,
) -> Result<(), DlError> {
    let stop = || abort.load(Ordering::Relaxed);
    let want = end - start + 1;
    let mut last = String::new();
    for attempt in 0..RETRIES {
        if stop() {
            return Err(cancelled());
        }
        let mut got = 0u64;
        let res = download_range_once(agent, url, file, start, end, downloaded, &stop, &mut got);
        match res {
            Ok(()) if got == want => {
                // Сначала данные чанка на диск, потом отметка в манифесте: иначе при потере питания манифест
                // опередил бы данные, и докачка пропустила бы дыру.
                let _ = file.sync_data();
                let mut m = lock(done);
                use std::io::Write;
                m.write_all(&start.to_le_bytes())
                    .and_then(|_| m.sync_data())
                    .map_err(|e| DlError::new("io", t!("setup-chunk-manifest", error = e.to_string())))?;
                return Ok(());
            }
            Ok(()) => last = t!("setup-range-incomplete", got = got, want = want),
            Err(e) if e.code != "network" => {
                downloaded.fetch_sub(got.min(downloaded.load(Ordering::Relaxed)), Ordering::Relaxed,
                );
                return Err(e);
            }
            Err(e) => last = e.detail,
        }
        downloaded.fetch_sub(got.min(downloaded.load(Ordering::Relaxed)), Ordering::Relaxed,
        );
        sleep_or_stop(Duration::from_millis(500 << attempt.min(7)), &stop)?;
    }
    Err(DlError::new("network", t!("setup-range-failed", start = start, end = end, retries = RETRIES, error = last.clone()),
    ))
}

/// Одна попытка скачать диапазон. Пишет got = сколько байт реально записано (для отката прогресса).
#[allow(clippy::too_many_arguments)]
fn download_range_once(
    agent: &ureq::Agent,
    url: &str,
    file: &Arc<File>,
    start: u64,
    end: u64,
    downloaded: &Arc<AtomicU64>,
    stop: &dyn Fn() -> bool,
    got: &mut u64,
) -> Result<(), DlError> {
    let (resp, _slot) = send(agent, url, Some((start, end)), stop)?;
    let status = resp.status().as_u16();
    if status != 206 {
        return Err(DlError::new("http_status", t!("setup-range-status", start = start, end = end, status = status),
        ));
    }
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 262_144];
    let mut offset = start;
    loop {
        if stop() {
            return Err(cancelled());
        }
        let n = reader.read(&mut buf).map_err(|e| DlError::new("network", t!("setup-range-read", error = e.to_string())))?;
        if n == 0 {
            break;
        }
        let mut w = 0;
        while w < n {
            let k = write_at(file, &buf[w..n], offset + w as u64).map_err(|e| DlError::new("io", t!("setup-write", error = e.to_string())))?;
            if k == 0 {
                return Err(DlError::new("io", "short write"));
            }
            w += k;
        }
        offset += n as u64;
        *got += n as u64;
        downloaded.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(())
}

/// Скачать файл целиком одним потоком (сервер без Range): каждая попытка начинает файл заново.
fn download_whole(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    downloaded: &Arc<AtomicU64>,
    abort: &Arc<AtomicBool>,
) -> Result<(), DlError> {
    use std::io::Write;
    let stop = || abort.load(Ordering::Relaxed);
    let mut last = String::new();
    for attempt in 0..RETRIES {
        let mut got = 0u64;
        let res = (|| -> Result<(), DlError> {
            let (resp, _slot) = send(agent, url, None, &stop)?;
            let status = resp.status().as_u16();
            if !(200..300).contains(&status) {
                return Err(DlError::new("http_status", t!("setup-http-status", url = url.to_string(), status = status),
                ));
            }
            let mut reader = resp.into_body().into_reader();
            let mut file = File::create(dest).map_err(|e| DlError::new("io", t!("setup-create", path = dest.display().to_string(), error = e.to_string())))?;
            let mut buf = vec![0u8; 262_144];
            loop {
                if stop() {
                    return Err(cancelled());
                }
                let n = reader.read(&mut buf).map_err(|e| DlError::new("network", t!("setup-read", error = e.to_string())))?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n]).map_err(|e| DlError::new("io", t!("setup-write", error = e.to_string())))?;
                got += n as u64;
                downloaded.fetch_add(n as u64, Ordering::Relaxed);
            }
            file.flush().map_err(|e| DlError::new("io", format!("flush: {e}")))
        })();
        match res {
            Ok(()) => return Ok(()),
            Err(e) => {
                downloaded.fetch_sub(got.min(downloaded.load(Ordering::Relaxed)), Ordering::Relaxed,
                );
                if e.code != "network" {
                    return Err(e);
                }
                last = e.detail;
            }
        }
        sleep_or_stop(Duration::from_millis(500 << attempt.min(7)), &stop)?;
    }
    Err(DlError::new("network", t!("setup-download-failed", url = url.to_string(), retries = RETRIES, error = last.clone()),
    ))
}

/// Потоковый SHA-256 файла блоками по 1 МиБ. None — остановлено (stop): пауза не ждёт конца хэширования 12 ГБ.
pub(crate) fn sha256_file(path: &Path, stop: &dyn Fn() -> bool) -> std::io::Result<Option<String>> {
    sha256_file_with(path, stop, &|_| {})
}

/// [`sha256_file`] that tells `hashed` how many bytes it has read, every 64 MiB and at the end.
fn sha256_file_with(path: &Path, stop: &dyn Fn() -> bool, hashed: &dyn Fn(u64)) -> std::io::Result<Option<String>> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut blocks = 0u32;
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
        read += n as u64;
        blocks += 1;
        if blocks.is_multiple_of(64) {
            hashed(read);
            if stop() {
                return Ok(None);
            }
        }
    }
    hashed(read);
    Ok(Some(digest.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    ))
}

/// The bar of a download while its files are checked: `done` of `total` bytes hashed.
fn verify_progress(progress: &ProgressCb, f: &FileSpec, done: u64, total: u64) {
    let pct = if total > 0 { (done as f64 / total as f64 * 100.0).min(100.0) } else { 0.0 };
    progress(json!({
        "stage": "download",
        "phase": "verify",
        "file": f.dest_rel,
        "msg": t!("setup-verifying", file = f.dest_rel),
        "downloaded": done,
        "total": total,
        "speed_bps": 0,
        "pct": pct,
    }));
}

fn discard_part(part: &Path) {
    let _ = std::fs::remove_file(part);
    let _ = std::fs::remove_file(done_manifest_path(part));
}

/// Проверить скачанный .part (размер и SHA-256 против закреплённых) и опубликовать: прямой файл — rename в
/// финал, архив — распаковка + запись об установке. Несовпадение — .part удаляется, следующая попытка с нуля.
/// `checked` — сколько байт из `check_total` уже проверено до этого файла: полоса идёт по проверке, а не стоит на 100 %.
fn publish(repo_root: &Path, f: &FileSpec, part: &Path, cancel: &dyn Fn() -> bool, progress: &ProgressCb, checked: u64, check_total: u64,
) -> Result<(), DlError> {
    let size = file_len(part);
    if size != f.size {
        discard_part(part);
        return Err(DlError::new(
            "size_mismatch",
            t!("setup-size-mismatch", file = f.dest_rel, got = size, want = f.size),
        ));
    }
    let hashed = |n: u64| verify_progress(progress, f, checked + n, check_total);
    let hash = sha256_file_with(part, cancel, &hashed).map_err(|e| DlError::new("io", t!("common-read", path = part.display().to_string(), error = e.to_string())))?;
    let Some(hash) = hash else { return Err(cancelled());
    };
    if hash != f.sha256 {
        discard_part(part);
        return Err(DlError::new(
            "hash_mismatch",
            t!("setup-hash-mismatch", file = f.dest_rel, got = hash.clone(), want = f.sha256),
        ));
    }
    let dest = repo_root.join(f.dest_rel);
    let dir = dest.parent().unwrap_or(repo_root);
    if f.extract != Extract::None {
        progress(json!({ "stage": "download", "phase": "extract", "file": f.dest_rel, "msg": t!("setup-unpacking", file = f.dest_rel) }),
        );
    }
    let written = match f.extract {
        Extract::None => {
            std::fs::rename(part, &dest).map_err(|e| {
                DlError::new("io", t!("setup-rename", path = dest.display().to_string(), error = e.to_string()))
            })?;
            // Прямой файл под tools/ — программа (yt-dlp): на Linux ей нужен бит запуска.
            #[cfg(unix)]
            if f.dest_rel.starts_with("tools/") {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| {
                    DlError::new("io", t!("setup-finalize", path = dest.display().to_string(), error = e.to_string()))
                })?;
            }
            let _ = std::fs::remove_file(done_manifest_path(part));
            if is_program(f) {
                write_record(repo_root, f, &[dest])?;
            }
            return Ok(());
        }
        extract => extract_archive(part, f.dest_rel, extract, dir),
    };
    let written = written.map_err(|e| DlError::new("extract", e))?;
    write_record(repo_root, f, &written)?;
    discard_part(part);
    Ok(())
}

/// Итог закачки: статус каждого компонента; скачанный вариант модели становится активным (models/active.json),
/// иначе резолв взял бы дефолт и альт-квант бы не применился.
fn finish(repo_root: &Path, selected: &[&Component]) -> Value {
    #[cfg(unix)]
    make_programs_executable(repo_root, selected);
    let mroot = crate::models_root(repo_root);
    let mut results = Vec::new();
    for c in selected {
        let st = component_status(repo_root, c);
        if st.installed {
            for (engine, variant) in crate::models::component_selection(c.id) {
                if variant == crate::models::DRAMA_TTS && crate::models::drama_unavailable(&mroot).is_some() {
                    continue;
                }
                if let Err(e) = crate::models::set_selection(&mroot, engine, &variant) {
                    tracing::warn!("active.json: the choice {engine}={variant} was not written: {e}");
                }
            }
        }
        results.push(json!({ "id": c.id, "installed": st.installed, "missing": st.missing }));
    }
    let overall = setup_status(repo_root);
    json!({ "components": results, "ready": overall.ready })
}

/// A marker naming a program: a file under tools/ without an extension, as Linux and macOS name programs.
#[cfg(any(unix, test))]
fn is_program_marker(rel: &str) -> bool {
    rel.starts_with("tools/") && Path::new(rel).extension().is_none()
}

/// The programs of the components (their markers) are made executable: an archive packed on Windows carries no
/// execute bit, and the program would answer «Permission denied».
#[cfg(unix)]
fn make_programs_executable(repo_root: &Path, selected: &[&Component]) {
    use std::os::unix::fs::PermissionsExt;
    for m in selected.iter().flat_map(|c| c.markers).filter(|m| is_program_marker(m.rel)) {
        let path = repo_root.join(m.rel);
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let mode = meta.permissions().mode();
        if mode & 0o111 != 0o111 {
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode | 0o755)) {
                tracing::error!("[ERROR] {} was not made executable: {e}", path.display());
            }
        }
    }
}

/// Скачать набор компонентов по id (идемпотентно: целые файлы закреплённой версии пропускаются, уже лежащий
/// файл перед пропуском сверяется по SHA-256). Тело синхронное: зовут фоновый поток менеджера или джоба.
/// cancel — пауза: скачанное остаётся в .part с манифестом чанков и докачивается следующим запуском.
/// Скачанный onnxruntime сразу становится библиотекой ort процесса.
pub fn download_components(
    repo_root: &Path,
    ids: &[String],
    cancel: &dyn Fn() -> bool,
    progress: &ProgressCb,
) -> Result<Value, DlError> {
    let fetched = fetch_components(repo_root, ids, cancel, progress)?;
    if ids.iter().any(|id| id.starts_with("onnxruntime")) {
        crate::point_ort(repo_root);
    }
    Ok(fetched)
}

fn fetch_components(
    repo_root: &Path,
    ids: &[String],
    cancel: &dyn Fn() -> bool,
    progress: &ProgressCb,
) -> Result<Value, DlError> {
    let all = manifest();
    let selected: Vec<&Component> = all
        .iter()
        .filter(|c| ids.iter().any(|x| x == c.id) && c.delivery == Delivery::Download)
        .collect();
    if selected.is_empty() {
        return Err(DlError::new("nothing_to_download", t!("downloads-nothing-to-download"),
        ));
    }
    fetch_selected(repo_root, selected, cancel, progress)
}

/// Downloads a component that is not on the setup screen (the voice pack) with the same chunks, retries, resume and
/// install record as the screen's components.
pub fn download_extra(repo_root: &Path, c: &Component, cancel: &dyn Fn() -> bool, progress: &ProgressCb) -> Result<Value, DlError> {
    fetch_selected(repo_root, vec![c], cancel, progress)
}

fn fetch_selected(
    repo_root: &Path,
    selected: Vec<&Component>,
    cancel: &dyn Fn() -> bool,
    progress: &ProgressCb,
) -> Result<Value, DlError> {
    let selected_ids: Vec<String> = selected.iter().map(|c| c.id.to_string()).collect();
    let _claim = claim(&selected_ids, cancel, &|| {
        progress(json!({ "stage": "download", "phase": "waiting", "msg": t!("setup-waiting-other") }),
        )
    })?;
    let agent = dl_agent()?;

    // Что качать: прямые файлы не той версии/размера и архивы без записи об установке закреплённой версии.
    struct Planned<'a> {
        ci: usize,
        f: &'a FileSpec,
        part: PathBuf,
    }
    let mut planned: Vec<Planned> = Vec::new();
    for (ci, c) in selected.iter().enumerate() {
        for f in c.files {
            if cancel() {
                return Err(cancelled());
            }
            if f.extract == Extract::None {
                let dest = repo_root.join(f.dest_rel);
                if file_ok(&dest, f.size) {
                    progress(json!({ "stage": "download", "phase": "verify", "file": f.dest_rel, "msg": t!("setup-verifying", file = f.dest_rel) }),
                    );
                    let hashed = |n: u64| verify_progress(progress, f, n, f.size);
                    let hash = sha256_file_with(&dest, cancel, &hashed).map_err(|e| {
                        DlError::new("io", t!("common-read", path = dest.display().to_string(), error = e.to_string()))
                    })?;
                    match hash {
                        None => return Err(cancelled()),
                        Some(h) if h == f.sha256 => continue,
                        Some(h) => {
                            tracing::warn!("{}: SHA-256 {h} does not match the pinned one; downloading again", dest.display());
                            std::fs::remove_file(&dest).map_err(|e| {
                                DlError::new("io", t!("setup-delete", path = dest.display().to_string(), error = e.to_string()))
                            })?;
                        }
                    }
                }
            } else if archive_installed(repo_root, f) {
                continue;
            }
            planned.push(Planned { ci, f, part: part_path(repo_root, f),
            });
        }
    }
    if planned.is_empty() {
        return Ok(finish(repo_root, &selected));
    }
    let need: u64 = planned.iter().map(|p| file_space_needed(repo_root, p.f)).sum();
    ensure_space(repo_root, need)?;
    std::fs::create_dir_all(download_dir(repo_root))
        .map_err(|e| {
        DlError::new("io", t!("setup-create", path = download_dir(repo_root).display().to_string(), error = e.to_string()),
        )
    })?;

    progress(json!({ "msg": t!("setup-downloading"), "stage": "download", "phase": "download" }));

    // Чанки всех файлов в ОДНУ очередь; счётчик прогресса — на КАЖДЫЙ компонент (comp_done[ci]).
    enum Task {
        // done — манифест завершённых чанков (дозапись offset при успехе) для РЕЗЮМА при следующем запуске.
        Range { file: Arc<File>, url: &'static str, start: u64, end: u64, ci: usize, done: Arc<Mutex<File>>,
        },
        Whole { url: &'static str, dest: PathBuf, ci: usize,
        },
    }
    let ncomp = selected.len();
    let comp_done: Vec<Arc<AtomicU64>> = (0..ncomp).map(|_| Arc::new(AtomicU64::new(0))).collect();
    let mut comp_total = vec![0u64; ncomp];
    let mut tasks: Vec<Task> = Vec::new();
    let mut open_files: Vec<Arc<File>> = Vec::new(); // держим хендлы живыми до конца пула
    for p in &planned {
        if cancel() {
            return Err(cancelled());
        }
        if let Some(parent) = p.part.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DlError::new("io", t!("setup-create", path = parent.display().to_string(), error = e.to_string())))?;
        }
        let (total, ranged) = probe(&agent, p.f.url, cancel)?;
        if total != 0 && total != p.f.size {
            return Err(DlError::new(
                "size_mismatch",
                t!("setup-source-changed", url = p.f.url, got = total, want = p.f.size),
            ));
        }
        comp_total[p.ci] += p.f.size;
        if !(ranged && total > 0) {
            tasks.push(Task::Whole { url: p.f.url, dest: p.part.clone(), ci: p.ci,
            });
            continue;
        }
        // Докачка: .part нужного размера и манифест рядом -> дочитываем только недостающие чанки.
        let done_path = done_manifest_path(&p.part);
        let resuming = file_len(&p.part) == total && done_path.is_file();
        let completed = if resuming {
            completed_offsets(&p.part, total)
        } else {
            let _ = std::fs::remove_file(&done_path);
            Default::default()
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(!resuming)
            .open(&p.part)
            .map_err(|e| DlError::new("io", t!("setup-create", path = p.part.display().to_string(), error = e.to_string())))?;
        if !resuming {
            file.set_len(total).map_err(|e| DlError::new("io", format!("set_len {}: {e}", p.part.display())))?;
        }
        let file = Arc::new(file);
        open_files.push(file.clone());
        let done = Arc::new(Mutex::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&done_path)
                .map_err(|e| {
                    DlError::new("io", t!("setup-manifest-write", path = done_path.display().to_string(), error = e.to_string()))
                })?,
        ));
        let mut start = 0u64;
        while start < total {
            let end = (start + CHUNK - 1).min(total - 1);
            if completed.contains(&start) {
                comp_done[p.ci].fetch_add(end - start + 1, Ordering::Relaxed);
            } else {
                tasks.push(Task::Range { file: file.clone(), url: p.f.url, start, end, ci: p.ci, done: done.clone(),
                });
            }
            start += CHUNK;
        }
    }
    let grand_total: u64 = comp_total.iter().sum();

    // Воркеры разбирают ОДНУ очередь чанков всех файлов; соединений не больше общего бюджета процесса.
    let n = POOL_SLOTS.min(tasks.len()).max(1);
    let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(tasks)));
    let finished = Arc::new(AtomicUsize::new(0));
    let abort = Arc::new(AtomicBool::new(false));
    let error: Arc<Mutex<Option<DlError>>> = Arc::new(Mutex::new(None));

    std::thread::scope(|sc| {
        for _ in 0..n {
            let (queue, comp_done, finished, abort, error, agent) =
                (queue.clone(), comp_done.clone(), finished.clone(), abort.clone(), error.clone(), agent.clone(),
            );
            sc.spawn(move || {
                loop {
                    if abort.load(Ordering::Relaxed) {
                        break;
                    }
                    let Some(task) = lock(&queue).pop_front() else { break;
                    };
                    let res = match &task {
                        Task::Range { file, url, start, end, ci, done,
                        } => download_range(&agent, url, file, *start, *end, &comp_done[*ci], &abort, done,
                        ),
                        Task::Whole { url, dest, ci } => {
                            download_whole(&agent, url, dest, &comp_done[*ci], &abort)
                        }
                    };
                    if let Err(e) = res {
                        if e.code != CANCELLED {
                            abort.store(true, Ordering::Relaxed);
                            let mut slot = lock(&error);
                            if slot.is_none() {
                                *slot = Some(e);
                            }
                        }
                        break;
                    }
                }
                finished.fetch_add(1, Ordering::SeqCst);
            });
        }
        // Главный поток: агрегатный + ПОКОМПОНЕНТНЫЙ прогресс + скорость по окну последних секунд + отмена.
        let mut window: std::collections::VecDeque<(Instant, u64)> = std::collections::VecDeque::new();
        loop {
            if cancel() {
                abort.store(true, Ordering::Relaxed);
            }
            let got: u64 = comp_done.iter().map(|a| a.load(Ordering::Relaxed)).sum();
            let now = Instant::now();
            window.push_back((now, got));
            while window.len() > 2 && now.duration_since(window[0].0) > Duration::from_secs(4) {
                window.pop_front();
            }
            let (t0, g0) = window[0];
            let dt = now.duration_since(t0).as_secs_f64();
            let bps = if dt > 0.5 { (got.saturating_sub(g0) as f64 / dt) as u64 } else { 0 };
            let overall = if grand_total > 0 { (got as f64 / grand_total as f64) * 100.0 } else { 0.0 };
            let parts: Vec<Value> = (0..ncomp)
                .filter(|&i| comp_total[i] > 0)
                .map(|i| {
                    let d = comp_done[i].load(Ordering::Relaxed).min(comp_total[i]);
                    let p = (d as f64 / comp_total[i] as f64 * 100.0).min(100.0);
                    json!({ "component": selected[i].id, "pct": p, "done": d, "total": comp_total[i] })
                })
                .collect();
            progress(json!({
                "stage": "download",
                "phase": "download",
                "msg": t!("setup-downloading"),
                "downloaded": got,
                "total": grand_total,
                "speed_bps": bps,
                "speed_mbps": bps as f64 / 1_000_000.0,
                "waiting_s": waiting_for_server(),
                "pct": overall.min(100.0),
                "parts": parts,
            }));
            if finished.load(Ordering::SeqCst) >= n {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });

    drop(open_files); // закрыть хендлы до проверки и распаковки

    // Сбой и пауза: .part и манифесты чанков остаются — следующий запуск докачает недостающее.
    if let Some(e) = lock(&error).take() {
        return Err(e);
    }
    if cancel() {
        return Err(cancelled());
    }

    // Проверка и публикация каждого файла; удачные публикуются, даже если соседний не сошёлся.
    let mut first_err: Option<DlError> = None;
    let check_total: u64 = planned.iter().map(|p| p.f.size).sum();
    let mut checked = 0u64;
    for p in &planned {
        let result = publish(repo_root, p.f, &p.part, cancel, progress, checked, check_total);
        checked += p.f.size;
        match result {
            Ok(()) => {}
            Err(e) if e.code == CANCELLED => return Err(e),
            Err(e) => {
                tracing::warn!("download {}: {}", p.f.dest_rel, e.detail);
                first_err.get_or_insert(e);
            }
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    Ok(finish(repo_root, &selected))
}

// ── Распаковка архивов (возвращают список положенных файлов для записи об установке) ──

/// Формат архива — по имени, под которым он закреплён: zip/wheel, tar.gz или tar.xz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveFormat {
    Zip,
    TarGz,
    TarXz,
}

fn archive_format(name: &str) -> ArchiveFormat {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
        ArchiveFormat::TarGz
    } else if n.ends_with(".tar.xz") {
        ArchiveFormat::TarXz
    } else {
        ArchiveFormat::Zip
    }
}

/// Программы, которые Extract::Pick берёт из сборки ffmpeg.
#[cfg(windows)]
const FFMPEG_PICK: &[&str] = &["ffmpeg.exe", "ffprobe.exe"];
#[cfg(not(windows))]
const FFMPEG_PICK: &[&str] = &["ffmpeg", "ffprobe"];

/// Динамическая библиотека платформы: *.dll на Windows, *.so и *.so.N на Linux.
fn is_shared_library(leaf: &str) -> bool {
    let n = leaf.to_ascii_lowercase();
    if cfg!(windows) {
        n.ends_with(".dll")
    } else {
        n.ends_with(".so") || n.contains(".so.")
    }
}

fn leaf_of(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// Путь записи внутри архива без `..`, `.` и корня (защита от zip-slip).
fn safe_relative(name: &str) -> Option<PathBuf> {
    let mut rel = PathBuf::new();
    for comp in name.split('/') {
        if comp.is_empty() || comp == "." || comp == ".." {
            continue;
        }
        rel.push(comp);
    }
    (!rel.as_os_str().is_empty()).then_some(rel)
}

/// Разложить архив по правилу `extract` в каталог `dir`. Символьные ссылки tar (libfoo.so -> libfoo.so.1)
/// ведут на соседний файл: на Linux они ссылками и остаются, на Windows становятся копией цели.
fn extract_archive(archive: &Path, name: &str, extract: Extract, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| t!("common-create-dir", path = dir.display().to_string(), error = e.to_string()))?;
    let place = |entry: &str| -> Option<PathBuf> {
        let leaf = leaf_of(entry);
        if leaf.is_empty() {
            return None;
        }
        match extract {
            Extract::None => None,
            Extract::Flat => Some(dir.join(leaf)),
            Extract::Pick => FFMPEG_PICK.iter().any(|w| w.eq_ignore_ascii_case(leaf)).then(|| dir.join(leaf)),
            Extract::Tree => safe_relative(entry).map(|rel| dir.join(rel)),
            // stubs/ redist NVIDIA — заглушки для линковки с теми же именами, что настоящие библиотеки.
            Extract::Libs => (is_shared_library(leaf) && !entry.contains("/stubs/")).then(|| dir.join(leaf)),
        }
    };
    let mut written = Vec::new();
    let mut links: Vec<(PathBuf, String)> = Vec::new();
    let format = archive_format(name);
    let file = std::fs::File::open(archive).map_err(|e| t!("setup-open", path = archive.display().to_string(), error = e.to_string()))?;
    match format {
        ArchiveFormat::Zip => {
            let mut zip = zip::ZipArchive::new(file).map_err(|e| t!("setup-not-zip", error = e.to_string()))?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| t!("setup-zip-entry", error = e.to_string()))?;
                if entry.is_dir() {
                    continue;
                }
                let entry_name = entry.name().replace('\\', "/");
                let Some(out) = place(&entry_name) else { continue };
                let mode = entry.unix_mode();
                write_entry(&mut entry, &out, mode)?;
                written.push(out);
            }
        }
        ArchiveFormat::TarGz | ArchiveFormat::TarXz => {
            let reader: Box<dyn Read> = if format == ArchiveFormat::TarGz {
                Box::new(flate2::read::GzDecoder::new(std::io::BufReader::new(file)))
            } else {
                Box::new(lzma_rust2::XzReader::new(std::io::BufReader::new(file), true))
            };
            let mut tar = tar::Archive::new(reader);
            let entries = tar.entries().map_err(|e| t!("setup-zip-entry", error = e.to_string()))?;
            for entry in entries {
                let mut entry = entry.map_err(|e| t!("setup-zip-entry", error = e.to_string()))?;
                let kind = entry.header().entry_type();
                let entry_name = entry.path().map_err(|e| t!("setup-zip-entry", error = e.to_string()))?.to_string_lossy().replace('\\', "/");
                if !(kind.is_file() || kind.is_symlink() || kind.is_hard_link()) {
                    continue;
                }
                let Some(out) = place(&entry_name) else { continue };
                if kind.is_symlink() || kind.is_hard_link() {
                    let target = entry.link_name().ok().flatten().map(|t| t.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                    links.push((out, leaf_of(&target).to_string()));
                    continue;
                }
                let mode = entry.header().mode().ok();
                write_entry(&mut entry, &out, mode)?;
                written.push(out);
            }
        }
    }
    // Ссылка может указывать на другую ссылку (libcudnn.so -> libcudnn.so.8 -> libcudnn.so.8.9.7) в любом
    // порядке записей: проходы, пока каждая не встанет.
    while !links.is_empty() {
        let before = links.len();
        let mut rest = Vec::new();
        for (out, target) in links {
            let source = out.parent().unwrap_or(dir).join(&target);
            if target.is_empty() || !source.is_file() {
                rest.push((out, target));
                continue;
            }
            let _ = std::fs::remove_file(&out);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &out).map_err(|e| t!("setup-finalize", path = out.display().to_string(), error = e.to_string()))?;
            #[cfg(not(unix))]
            std::fs::copy(&source, &out).map_err(|e| t!("setup-finalize", path = out.display().to_string(), error = e.to_string()))?;
            written.push(out);
        }
        if rest.len() == before {
            return Err(t!("setup-archive-no-files", path = archive.display().to_string()));
        }
        links = rest;
    }
    #[cfg(unix)]
    if matches!(extract, Extract::Flat | Extract::Libs) {
        add_soname_links(&mut written)?;
    }
    if written.is_empty() {
        let path = archive.display().to_string();
        return Err(if extract == Extract::Libs { t!("setup-archive-no-dll", path = path) } else { t!("setup-archive-no-files", path = path) });
    }
    Ok(written)
}

/// Как `ldconfig -n`: библиотеке, пришедшей одним полным именем (`libX.so.0.15.1`, `libX.0.15.1.dylib`), без
/// соседней ссылки по имени, которое просит загрузчик, поставить эту ссылку (CPU-сборка BSRoformer для Linux, его
/// сборка для macOS).
#[cfg(unix)]
fn add_soname_links(written: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut added = Vec::new();
    for path in written.iter() {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Some(soname) = soname_of(name) else { continue };
        let link = path.with_file_name(&soname);
        if link.exists() || written.contains(&link) || added.contains(&link) {
            continue;
        }
        std::os::unix::fs::symlink(name, &link).map_err(|e| t!("setup-finalize", path = link.display().to_string(), error = e.to_string()))?;
        added.push(link);
    }
    written.extend(added);
    Ok(())
}

/// The name the loader asks for of a library named by its full version: `libX.so.0.15.1` → `libX.so.0`,
/// `libX.0.15.1.dylib` → `libX.0.dylib`; None for a name without one.
#[cfg(any(unix, test))]
fn soname_of(name: &str) -> Option<String> {
    let numeric = |parts: &[&str]| parts.len() >= 2 && parts.iter().all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
    if let Some(at) = name.find(".so.") {
        let version: Vec<&str> = name[at + 4..].split('.').collect();
        return numeric(&version).then(|| format!("{}.so.{}", &name[..at], version[0]));
    }
    let stem = name.strip_suffix(".dylib")?;
    let mut parts: Vec<&str> = stem.split('.').collect();
    let base = parts.remove(0);
    let first = parts.iter().position(|p| p.bytes().all(|b| b.is_ascii_digit()))?;
    let (named, version) = parts.split_at(first);
    numeric(version).then(|| format!("{}.{}.dylib", std::iter::once(base).chain(named.iter().copied()).collect::<Vec<_>>().join("."), version[0]))
}

/// Записать элемент архива в файл через .part+rename (атомарно); на Linux — с правами из архива (бит запуска).
fn write_entry(entry: &mut impl Read, out: &Path, mode: Option<u32>) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| t!("common-create-dir", path = parent.display().to_string(), error = e.to_string()))?;
    }
    let tmp = with_suffix(out, ".part");
    {
        let mut fout = std::fs::File::create(&tmp).map_err(|e| t!("setup-create", path = tmp.display().to_string(), error = e.to_string()))?;
        std::io::copy(entry, &mut fout).map_err(|e| t!("setup-unpack", path = out.display().to_string(), error = e.to_string()))?;
    }
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode & 0o777));
    }
    #[cfg(not(unix))]
    let _ = mode;
    std::fs::rename(&tmp, out).map_err(|e| t!("setup-finalize", path = out.display().to_string(), error = e.to_string()))?;
    Ok(())
}


// ═══════════════════════════════════════════════════════════════════════════
/// Models of a former version that nothing reads any more, under the models folder: Parakeet on ONNX Runtime (Dub
/// Studio 4.3 and earlier) and the Faster-Whisper models (4.4 and earlier).
const RETIRED: &[&str] = &["tdt", "tdt-fp32", "tdt-ultra", "tdt-ultra-int8", "whisper"];

/// Programs of a former version that nothing runs any more, under the program folder: the OpenRouter helper (Dub Studio
/// 4.2 and earlier), the Higgs library (Higgs runs in audiocpp_server now) and ONNX Runtime 1.24.2 (before 1.28.2).
const RETIRED_TOOLS: &[&str] = &[
    "tools/openrouter-helper",
    "models/higgs-engine/audiocpp_engine.dll",
    "models/higgs-engine/libaudiocpp_engine.so",
    "models/runtime/onnxruntime-win-x64-1.24.2",
    "models/runtime/onnxruntime-win-x64-gpu-1.24.2",
];

/// The studio's own downloads it no longer runs are deleted for good: hundreds of megabytes of runtimes have no place
/// in the person's recycle bin, which keeps their projects.
fn delete(path: &Path) -> Result<(), String> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }.map_err(|e| e.to_string())
}

/// Removes what former versions left (`leftovers`): the last step of the update screen, after the downloads that
/// take their place. A link (another program's folder linked in) loses only the link; a hard-linked file stays with
/// the program that shares it.
pub fn remove_leftovers(repo_root: &Path, models_root: &Path) {
    retire(models_root, RETIRED, &delete);
    retire_whisper(repo_root, &delete);
    retire(repo_root, RETIRED_TOOLS, &delete);
}

/// What former versions left on disk, each under the name the update screen's table of changes knows it by: a
/// retired model folder (RETIRED, under the models folder), a retired program (RETIRED_TOOLS) and the former
/// Whisper's files ("tools/whisper").
pub(crate) fn leftovers(repo_root: &Path, models_root: &Path) -> Vec<(&'static str, Vec<PathBuf>)> {
    let mut found = Vec::new();
    for (root, retired) in [(models_root, RETIRED), (repo_root, RETIRED_TOOLS)] {
        for rel in retired {
            let path = root.join(rel);
            if std::fs::symlink_metadata(&path).is_ok() {
                found.push((*rel, vec![path]));
            }
        }
    }
    let whisper = whisper_leftovers(repo_root);
    if !whisper.is_empty() {
        found.push(("tools/whisper", whisper));
    }
    found
}

/// The bytes removing `path` frees: a link frees nothing, a folder what lies in it.
pub(crate) fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if meta.file_type().is_symlink() {
        return 0;
    }
    if !meta.is_dir() {
        return meta.len();
    }
    match std::fs::read_dir(path) {
        Ok(entries) => entries.flatten().map(|entry| tree_size(&entry.path())).sum(),
        Err(e) => {
            tracing::warn!("{} is not readable, its size is not counted: {e}", path.display());
            0
        }
    }
}

fn retire(root: &Path, retired: &[&str], discard: &dyn Fn(&Path) -> Result<(), String>) {
    for rel in retired {
        let path = root.join(rel);
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        let removed = if meta.file_type().is_symlink() {
            std::fs::remove_dir(&path).or_else(|_| std::fs::remove_file(&path)).map_err(|e| e.to_string())
        } else {
            discard(&path)
        };
        match removed {
            Ok(()) => tracing::info!("removed the retired {}", path.display()),
            Err(e) => tracing::error!("[ERROR] could not remove the retired {}: {e}", path.display()),
        }
    }
}

/// Install records of the Whisper-Faster r192.3/r189.1 program and its CUDA 11 cuBLAS and cuDNN 8 (Dub Studio 4.4 and
/// earlier), which lay flat in tools/whisper; Whisper is CrisperWhisper in audiocpp_server now.
const RETIRED_WHISPER_RECORDS: &[&str] = &[
    "tools/whisper/_whisper.zip.json",
    "tools/whisper/_wcublas.zip.json",
    "tools/whisper/_wcudnn.zip.json",
    "tools/whisper/_wcublas.tar.xz.json",
    "tools/whisper/_wcudnn.tar.xz.json",
];

/// The former Whisper program and the CUDA 11 cuBLAS and cuDNN 8 it brought, as an install without records left them
/// in tools/whisper.
const RETIRED_WHISPER_FILES: &[&str] = &[
    "whisper-faster.exe",
    "whisper-faster",
    "cublas64_11.dll",
    "cublasLt64_11.dll",
    "nvblas64_11.dll",
    "cudnn64_8.dll",
    "cudnn_adv_infer64_8.dll",
    "cudnn_adv_train64_8.dll",
    "cudnn_cnn_infer64_8.dll",
    "cudnn_cnn_train64_8.dll",
    "cudnn_ops_infer64_8.dll",
    "cudnn_ops_train64_8.dll",
    "license.txt",
    "_models",
];

/// The former Whisper program and its CUDA 11 libraries on disk: what their install records list, and an older
/// install without records (the program, the cuBLAS and cuDNN libraries, its licence and the models it downloaded
/// for itself lying directly in tools/whisper).
fn whisper_leftovers(repo_root: &Path) -> Vec<PathBuf> {
    let dir = repo_root.join("tools").join("whisper");
    let mut doomed: Vec<PathBuf> = Vec::new();
    for rel in RETIRED_WHISPER_RECORDS {
        let record = repo_root.join(rel);
        let Ok(text) = std::fs::read_to_string(&record) else { continue };
        match serde_json::from_str::<ArchiveRecord>(&text) {
            Ok(r) => doomed.extend(r.files.iter().filter(|x| x.path.starts_with("tools/whisper/")).map(|x| repo_root.join(&x.path))),
            Err(e) => tracing::error!("[ERROR] the install record {} is unreadable: {e}", record.display()),
        }
        doomed.push(record);
    }
    doomed.extend(RETIRED_WHISPER_FILES.iter().map(|name| dir.join(name)));
    doomed.sort();
    doomed.dedup();
    doomed.retain(|p| p.is_file() || p.is_dir());
    doomed
}

fn retire_whisper(repo_root: &Path, discard: &dyn Fn(&Path) -> Result<(), String>) {
    for path in whisper_leftovers(repo_root) {
        match discard(&path) {
            Ok(()) => tracing::info!("removed the retired Whisper file {}", path.display()),
            Err(e) => tracing::error!("[ERROR] could not remove the retired Whisper file {}: {e}", path.display()),
        }
    }
    // the folder was the former program's alone: it goes once nothing is left in it
    let dir = repo_root.join("tools").join("whisper");
    if std::fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_none()) {
        if let Err(e) = std::fs::remove_dir(&dir) {
            tracing::error!("[ERROR] could not remove the empty {}: {e}", dir.display());
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn retired_models_go_and_current_ones_stay() {
        let root = tempfile::tempdir().unwrap();
        for rel in RETIRED {
            let path = root.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            if rel.ends_with(".onnx") {
                std::fs::write(&path, b"x").unwrap();
            } else {
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(path.join("encoder-model.int8.onnx"), b"x").unwrap();
            }
        }
        let kept = root.path().join("asr-gguf-parakeet-v3").join("parakeet-tdt-0.6b-v3-Q8_0.gguf");
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, b"x").unwrap();
        let gone = |p: &Path| if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) }.map_err(|e| e.to_string());
        retire(root.path(), RETIRED, &gone);
        assert!(RETIRED.iter().all(|rel| !root.path().join(rel).exists()));
        assert!(kept.is_file());
        retire(root.path(), RETIRED, &gone);
        for rel in RETIRED_TOOLS {
            std::fs::create_dir_all(root.path().join(rel)).unwrap();
            std::fs::write(root.path().join(rel).join("x.dll"), b"x").unwrap();
        }
        let current = root.path().join("models/runtime/onnxruntime-win-x64-1.28.2/lib/onnxruntime.dll");
        std::fs::create_dir_all(current.parent().unwrap()).unwrap();
        std::fs::write(&current, b"x").unwrap();
        retire(root.path(), RETIRED_TOOLS, &gone);
        assert!(RETIRED_TOOLS.iter().all(|rel| !root.path().join(rel).exists()));
        assert!(current.is_file());
    }

    /// An installed studio (DUB_STUDIO_CHECK_ROOT, read only): every component with a file on disk that is not the
    /// pinned one, which the next job or the first-run screen downloads again.
    #[test]
    #[ignore]
    fn an_installed_studio_names_the_files_an_update_replaces() {
        let Ok(root) = std::env::var("DUB_STUDIO_CHECK_ROOT") else { return };
        let root = PathBuf::from(root);
        for c in manifest() {
            let st = component_status(&root, &c);
            let present: Vec<&str> = c.markers.iter().map(|m| m.rel).filter(|rel| root.join(rel).is_file()).collect();
            if !st.installed && !present.is_empty() {
                let found: Vec<String> = present.iter().map(|rel| format!("{rel} {} bytes", file_len(&root.join(rel)))).collect();
                println!("[REPLACE] {}: on disk {} | missing {}", c.id, found.join(", "), st.missing.join(", "));
            } else if st.installed {
                println!("[OK] {}", c.id);
            }
        }
    }

    /// A program folder (DUB_STUDIO_SWAP_ROOT) holding an engine of a former release: the component counts as missing,
    /// and its download puts the pinned file in its place.
    #[test]
    #[ignore]
    fn a_former_engine_is_replaced_by_the_pinned_one() {
        let Ok(root) = std::env::var("DUB_STUDIO_SWAP_ROOT") else { return };
        let (root, id) = (PathBuf::from(root), std::env::var("DUB_STUDIO_SWAP_COMPONENT").unwrap_or_else(|_| "transcribe-engine".into()));
        let c = manifest().into_iter().find(|c| c.id == id).unwrap();
        let file = root.join(c.files[0].dest_rel);
        let before = file_len(&file);
        assert!(!component_status(&root, &c).installed, "{} of {before} bytes counts as installed", file.display());
        let progress = |_: Value| {};
        download_components(&root, &[id.clone()], &|| false, &progress).map_err(|e| e.detail).unwrap();
        assert!(component_status(&root, &c).installed);
        if c.files[0].extract == Extract::None {
            assert_eq!(file_len(&file), c.files[0].size);
        }
        println!("[OK] {id}: {before} bytes -> {} bytes, {}", file_len(&file), file.display());
    }
    use super::*;

    fn download_proxy(address: &str, kind: dub_llm::net::ProxyKind) -> Result<ureq::Proxy, String> {
        dl_proxy(&dub_llm::net::normalize(address, kind).map_err(|e| format!("{e:#}"))?)
    }

    fn model(id: &str, gb: f64, requirement: Requirement) -> ComponentStatus {
        ComponentStatus {
            id: id.into(),
            name: id.into(),
            purpose: String::new(),
            requirement,
            delivery: Delivery::Download,
            size: 0,
            installed: false,
            bytes_on_disk: 0,
            space_needed: 0,
            missing: vec![],
            detail: None,
            external_url: None,
            vram: (gb * 1024.0 * 1024.0 * 1024.0) as u64,
            fits_vram: None,
            taken_from: None,
        }
    }

    fn models() -> Vec<ComponentStatus> {
        vec![
            model("higgs", 5.6, Requirement::Required),
            model("higgs-q6_k", 5.1, Requirement::Optional),
            model("higgs-q4_k_m", 4.2, Requirement::Optional),
            model("gemma", 8.5, Requirement::Required),
            model("gemma-q8_0", 14.0, Requirement::Optional),
            model("higgs-engine", 0.0, Requirement::Required),
        ]
    }

    fn required(comps: &[ComponentStatus]) -> Vec<&str> {
        comps.iter().filter(|c| c.requirement == Requirement::Required).map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn a_downloaded_variant_stands_for_its_model_on_the_first_run() {
        let mut comps = vec![
            model("parakeet", 1.1, Requirement::Required),
            model("parakeet-ultra", 1.1, Requirement::Optional),
            model("gemma", 8.5, Requirement::Required),
            model("higgs-engine", 0.0, Requirement::Required),
        ];
        comps[1].installed = true;
        installed_variant_covers_group(&mut comps);
        assert_eq!(required(&comps), ["gemma", "higgs-engine"], "ultra was picked, the int8 default is no longer owed");

        let mut comps = models();
        installed_variant_covers_group(&mut comps);
        assert_eq!(required(&comps), ["higgs", "gemma", "higgs-engine"], "nothing downloaded keeps every default");
    }

    #[test]
    fn drama_can_be_downloaded_from_a_6_gb_card_and_not_from_a_4_gb_one() {
        let drama = || vec![ComponentStatus { vram: vram_estimate("drama3"), ..model("drama3", 0.0, Requirement::Optional) }];
        let mut comps = drama();
        fit_to_vram(&mut comps, 6 << 30);
        assert_eq!(comps[0].fits_vram, Some(true), "6 GB card: Drama runs with its experts in RAM");
        let mut comps = drama();
        fit_to_vram(&mut comps, 4 << 30);
        assert_eq!(comps[0].fits_vram, Some(false), "4 GB card: no room for the expert cache");
    }

    #[test]
    fn a_small_card_gets_the_largest_quant_that_fits_and_no_model_it_cannot_hold() {
        let mut comps = models();
        fit_to_vram(&mut comps, 6 * 1024 * 1024 * 1024);
        assert_eq!(required(&comps), ["higgs-q6_k", "higgs-engine"]);
        assert_eq!(comps[0].fits_vram, Some(false));
        assert_eq!(comps[1].fits_vram, Some(true));
        assert_eq!(comps[3].fits_vram, Some(false));
        assert_eq!(comps[5].fits_vram, None);

        let mut comps = models();
        fit_to_vram(&mut comps, 24 * 1024 * 1024 * 1024);
        assert_eq!(required(&comps), ["higgs", "gemma", "higgs-engine"]);
        assert!(comps.iter().filter(|c| c.vram > 0).all(|c| c.fits_vram == Some(true)));

        let mut comps = models();
        comps[0].installed = true;
        fit_to_vram(&mut comps, 6 * 1024 * 1024 * 1024);
        assert_eq!(required(&comps), ["higgs", "higgs-engine"]);

        let mut comps = models();
        fit_to_vram(&mut comps, 0);
        assert_eq!(required(&comps), ["higgs", "gemma", "higgs-engine"]);
        assert!(comps.iter().all(|c| c.fits_vram.is_none()));
    }

    #[test]
    fn a_download_hands_the_proxy_its_password_as_is() {
        use dub_llm::net::ProxyKind;
        use ureq::ProxyProtocol;
        for (address, kind, protocol) in [
            ("1.2.3.4:8000:bob:p@ss", ProxyKind::Socks5, ProxyProtocol::Socks5h,
            ),
            ("1.2.3.4:8000:bob:p@ss", ProxyKind::Http, ProxyProtocol::Http,
            ),
            ("socks5://bob:p%40ss@1.2.3.4:8000", ProxyKind::Http, ProxyProtocol::Socks5,
            ),
            ("http://bob:p@ss@1.2.3.4:8000", ProxyKind::Http, ProxyProtocol::Http,
            ),
        ] {
            let proxy = download_proxy(address, kind).unwrap();
            assert_eq!((proxy.protocol(), proxy.username(), proxy.password()), (protocol, Some("bob"), Some("p@ss")), "{address}");
            assert_eq!((proxy.host(), proxy.port()), ("1.2.3.4", 8000), "{address}");
        }
        let seller = download_proxy("1.2.3.4:8000:b@b:p;=!$&'()*+,ss", ProxyKind::Socks5).unwrap();
        assert_eq!((seller.username(), seller.password()), (Some("b@b"), Some("p;=!$&'()*+,ss")));

        let colon = download_proxy(&format!("http://bob:{}@1.2.3.4:8000", dub_llm::net::encode_userinfo("p@ss:1")), ProxyKind::Http,
        ).unwrap();
        assert_eq!(format!("{}:{}", colon.username().unwrap(), colon.password().unwrap()), "bob:p@ss:1", "CONNECT sends user:password whole");

        let plain = download_proxy("proxy.example:3128", ProxyKind::Http).unwrap();
        assert_eq!((plain.username(), plain.password(), plain.port()), (None, None, 3128));
        let v6 = download_proxy("[::1]:1080", ProxyKind::Socks5).unwrap();
        assert_eq!((v6.protocol(), v6.port()), (ProxyProtocol::Socks5h, 1080));

        for (password, kind) in [("p@ss:1/x", ProxyKind::Http), ("p@ss:1/x", ProxyKind::Socks5), ("pa:ss", ProxyKind::Socks5), ("pa ss", ProxyKind::Http), ("p\u{e4}ss", ProxyKind::Http),
        ] {
            let address = format!("bob:{}@1.2.3.4:8000", dub_llm::net::encode_userinfo(password));
            let refused = download_proxy(&address, kind).expect_err(password);
            assert!(refused.contains("ureq") && !refused.contains(password) && !refused.contains(&dub_llm::net::encode_userinfo(password)), "{refused}");
        }
    }

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dub-setup-{tag}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Файл нужного размера без записи содержимого (для проверок по размеру, не по хэшу).
    fn sized(path: &Path, size: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap().set_len(size).unwrap();
    }

    #[test]
    fn a_linux_older_than_glibc_2_38_runs_gemma_on_vulkan_instead_of_cuda() {
        use crate::acpp::Build;
        assert_eq!(parse_glibc("2.35"), Some((2, 35)));
        assert_eq!(parse_glibc("2.39\n"), Some((2, 39)));
        assert_eq!(parse_glibc("2.38-ubuntu"), Some((2, 38)));
        assert_eq!(parse_glibc("x"), None);
        for card in [Build::Cuda13, Build::Cuda12] {
            assert_eq!(llama_build_for(card, Some((2, 35))), Build::Vulkan);
            assert_eq!(llama_build_for(card, Some((2, 38))), card);
            assert_eq!(llama_build_for(card, None), card);
        }
        assert_eq!(llama_build_for(Build::Cpu, Some((2, 35))), Build::Cpu);
        assert_eq!(llama_build_for(Build::Vulkan, Some((2, 35))), Build::Vulkan);
    }

    #[test]
    fn manifest_ids_unique_and_nonempty() {
        let m = manifest();
        assert!(!m.is_empty());
        let mut ids: Vec<&str> = m.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "id компонентов должны быть уникальны");
    }

    #[test]
    fn download_components_have_files_and_markers() {
        for c in manifest() {
            match c.delivery {
                Delivery::Download => {
                    assert!(!c.files.is_empty(), "{}: Download без files", c.id);
                    assert!(!c.markers.is_empty(), "{}: Download без markers", c.id);
                    for f in c.files {
                        assert!(f.url.starts_with("https://"), "{}: не https URL", c.id);
                    }
                }
                Delivery::Bundled => {
                    assert!(c.files.is_empty(), "{}: Bundled не качается", c.id);
                    assert!(!c.markers.is_empty(), "{}: Bundled без markers", c.id);
                }
                Delivery::External => {
                    assert!(c.external_url.is_some(), "{}: External без url", c.id);
                }
            }
        }
    }

    fn comp(id: &str) -> Component {
        manifest().into_iter().find(|c| c.id == id).unwrap_or_else(|| panic!("нет компонента {id}"))
    }

    #[test]
    fn diarization_component_is_nemotron_at_dub_asr_path() {
        let c = comp("sortformer");
        let want = format!("models/{}/{}", dub_asr::DIAR_MODEL_DIR, dub_asr::DIAR_MODEL_FILE);
        assert_eq!(c.markers.len(), 1);
        assert_eq!(c.markers[0].rel, want);
        assert!(c.files.iter().any(|f| f.dest_rel == want));
        assert!(c.files.iter().any(|f| f.dest_rel == format!("models/{}/LICENSE", dub_asr::DIAR_MODEL_DIR)));
        for x in manifest() {
            assert!(x.markers.iter().all(|m| !m.rel.contains("4spk-v2")), "{}: маркер на Sortformer v2", x.id);
        }
    }

    #[test]
    fn new_model_components_pinned_and_sized() {
        for id in ["sortformer", "parakeet-ultra"] {
            let c = comp(id);
            let sum: u64 = c.files.iter().map(|f| f.size).sum();
            assert_eq!(c.size, sum, "{id}: size != сумме файлов");
            for f in c.files {
                assert!(f.size > 0, "{id}: {} без размера", f.dest_rel);
                assert!(!f.url.contains("/resolve/main/"), "{id}: {} не закреплён ревизией", f.url);
            }
            for m in c.markers {
                let f = c.files.iter().find(|f| f.dest_rel == m.rel).expect("маркер без файла");
                assert_eq!(f.size, m.expect, "{id}: {} маркер != размеру файла", m.rel);
            }
        }
    }

    /// Всё, что качает «Первый запуск», закреплено: HF — коммитом в URL, остальное — версией в пути; у каждого
    /// файла точный размер и SHA-256. Движущаяся цель (resolve/main, releases/latest) ломает установку у
    /// пользователей, как только апстрим перезальёт файл.
    #[test]
    fn every_downloaded_file_is_pinned() {
        let mut problems = Vec::new();
        for c in manifest() {
            for f in c.files {
                if f.size == 0 {
                    problems.push(format!("{}: {} без размера", c.id, f.dest_rel));
                }
                if f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                    problems.push(format!("{}: {} — sha256 не 64 hex в нижнем регистре", c.id, f.dest_rel));
                }
                if let Some(rest) = f.url.strip_prefix("https://huggingface.co/") {
                    let rev = rest.split('/').nth(3).unwrap_or("");
                    let pinned = rest.split('/').nth(2) == Some("resolve")
                        && rev.len() == 40
                        && rev.bytes().all(|b| b.is_ascii_hexdigit());
                    if !pinned {
                        problems.push(format!("{}: {} не закреплён коммитом", c.id, f.url));
                    }
                }
                if f.url.contains("/latest/") || f.url.contains("/resolve/main/") {
                    problems.push(format!("{}: {} — движущаяся цель", c.id, f.url));
                }
            }
        }
        assert!(problems.is_empty(), "не закреплено: {problems:#?}");
    }

    #[test]
    fn component_size_is_the_sum_of_its_files() {
        for c in manifest().iter().filter(|c| c.delivery == Delivery::Download) {
            let sum: u64 = c.files.iter().map(|f| f.size).sum();
            assert_eq!(c.size, sum, "{}: size компонента не равен сумме files", c.id);
        }
    }

    /// Маркер прямого файла и сам файл говорят об одном размере — иначе компонент не станет установленным никогда.
    #[test]
    fn markers_agree_with_files() {
        for c in manifest() {
            for m in c.markers {
                if let Some(f) = c.files.iter().find(|f| f.extract == Extract::None && f.dest_rel == m.rel) {
                    assert!(m.expect == 0 || m.expect == f.size, "{}: маркер {} {} != файл {}", c.id, m.rel, m.expect, f.size);
                }
            }
        }
    }


    /// The recognition models and their engine download the same files as dub_asr::tcpp names, so every program
    /// that keeps them takes them from its neighbour.
    #[test]
    fn transcribe_cpp_components_are_dub_asrs() {
        for (id, model) in [("parakeet", "parakeet-v3"), ("parakeet-ultra", "parakeet-ultra")] {
            let m = dub_asr::tcpp::model(model).unwrap();
            assert_eq!(m.component, id);
            let c = comp(id);
            let [f] = c.files else { panic!("{id}: one file") };
            assert_eq!((f.url, f.size, f.sha256), (m.download.url, m.download.size, m.download.sha256), "{id}");
            assert_eq!(std::path::Path::new(f.dest_rel), std::path::Path::new("models").join(m.dir).join(m.file), "{id}");
        }
        for b in dub_sep::asset::Build::ALL {
            let Some(c) = manifest().into_iter().find(|c| c.id == b.component()) else { continue };
            let a = b.engine().expect("a component only for a build this platform has");
            let [f] = c.files else { panic!("{b:?}: one archive") };
            assert_eq!((f.url, f.dest_rel, f.size, f.sha256), (a.url, a.archive, a.size, a.sha256), "{b:?}");
            let cli = dub_sep::engine_cli_of(Path::new(""), b.name());
            assert!(c.markers.iter().any(|m| Path::new(m.rel) == cli), "{b:?}: the marker is the program dub_sep runs");
        }
        // a Mac has the processor build alone (Metal in it)
        let builds: &[dub_asr::tcpp::Build] = if cfg!(target_os = "macos") {
            &[dub_asr::tcpp::Build::Cpu]
        } else {
            &[dub_asr::tcpp::Build::Cuda, dub_asr::tcpp::Build::Cuda12, dub_asr::tcpp::Build::Cpu]
        };
        for &b in builds {
            let d = b.download();
            let [f] = comp(b.dir()).files else { panic!("{b:?}: one file") };
            assert_eq!((f.url, f.size, f.sha256), (d.url, d.size, d.sha256), "{b:?}");
            assert_eq!(std::path::Path::new(f.dest_rel), std::path::Path::new("models").join(b.dir()).join(dub_asr::tcpp::LIBRARY), "{b:?}");
        }
    }

    /// Имена архивов в каталоге закачек не пересекаются (два _engine.zip разных движков).
    #[test]
    fn download_parts_do_not_collide() {
        let root = Path::new("R");
        let mut seen = std::collections::HashSet::new();
        for c in manifest() {
            for f in c.files {
                assert!(seen.insert(part_path(root, f)), "{}: .part совпадает с другим файлом", f.dest_rel);
            }
        }
    }

    /// Архив установлен, только если его запись называет закреплённый sha256 и файлы на месте: у обновившихся
    /// пользователей CUDA-DLL прошлой версии (того же имени и даже размера) дают «недостающий компонент».
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn an_archive_counts_only_with_the_pinned_version_recorded() {
        let root = temp_root("record");
        let all = manifest();
        let cuda = all.iter().find(|c| c.id == "cuda-runtime").unwrap();
        let wheel = &cuda.files[0];
        let dll = root.join("models/higgs-engine/cudart64_13.dll");
        sized(&dll, 551_024);
        assert!(!archive_installed(&root, wheel), "DLL без записи — старая установка");

        write_record(&root, wheel, std::slice::from_ref(&dll)).unwrap();
        assert!(archive_installed(&root, wheel));

        let mut rec = read_record(&root, wheel).unwrap();
        rec.sha256 = "0".repeat(64);
        std::fs::write(record_path(&root, wheel), serde_json::to_vec(&rec).unwrap()).unwrap();
        assert!(!archive_installed(&root, wheel), "запись о другой версии архива");

        write_record(&root, wheel, std::slice::from_ref(&dll)).unwrap();
        sized(&dll, 1);
        assert!(!archive_installed(&root, wheel), "файл из записи изменился");
        std::fs::remove_dir_all(&root).ok();
    }

    #[cfg(windows)]
    #[test]
    fn the_vc_runtime_counts_where_the_loader_finds_it() {
        let root = temp_root("vcrt");
        let system = temp_root("vcrt-system");
        let vc = manifest().into_iter().find(|c| c.id == "vcruntime").unwrap();
        let name = |m: &Marker| Path::new(m.rel).file_name().unwrap().to_owned();

        let st = status_with_system_dir(&root, &vc, Some(&system));
        assert!(!st.installed);
        assert_eq!(st.missing.len(), vc.markers.len());

        let (engine, rest) = vc.markers.split_at(2);
        for m in engine {
            sized(&root.join(m.rel), 10);
        }
        for m in rest {
            sized(&system.join(name(m)), 10);
        }
        let st = status_with_system_dir(&root, &vc, Some(&system));
        assert!(st.installed, "{:?}", st.missing);
        assert_eq!(st.detail.as_deref(), Some("system"));
        assert!(!status_with_system_dir(&root, &vc, None).installed, "без системного каталога — только комплект");

        std::fs::remove_file(system.join(name(&rest[0]))).unwrap();
        let st = status_with_system_dir(&root, &vc, Some(&system));
        assert!(!st.installed);
        assert_eq!(st.missing, vec![rest[0].rel.to_string()]);

        for m in rest {
            sized(&root.join(m.rel), 10);
        }
        let st = status_with_system_dir(&root, &vc, Some(&system));
        assert!(st.installed);
        assert_eq!(st.detail, None, "весь комплект рядом с движком");
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&system).ok();
    }

    #[test]
    #[cfg(windows)]
    fn the_system_dir_is_the_one_windows_loads_from() {
        let dir = system_dir().expect("GetSystemDirectoryW");
        assert!(dir.join("kernel32.dll").is_file(), "{}", dir.display());
        assert!(FOUND_IN_SYSTEM_DIR.iter().all(|id| manifest().iter().any(|c| c.id == *id && c.delivery == Delivery::Bundled)));
    }

    /// Комплект релиза: каждый файл Bundled-компонента лежит в staging установщика (или в распакованном
    /// портативе) по тому же пути, что после установки. Шаг сборки — desktop/src-tauri/STAGING.md.
    #[test]
    #[ignore]
    fn the_release_staging_carries_every_bundled_file() {
        let stage = PathBuf::from(std::env::var("DUB_RELEASE_STAGING").expect("DUB_RELEASE_STAGING = каталог staging или портатива"),
        );
        let absent: Vec<String> = manifest()
            .iter()
            .filter(|c| c.delivery == Delivery::Bundled)
            .flat_map(|c| c.markers.iter().map(move |m| (c.id, m)))
            .filter(|(_, m)| !marker_ok(&stage, m))
            .map(|(id, m)| format!("{id}: {}", m.rel))
            .collect();
        assert!(absent.is_empty(), "нет в {}: {absent:#?}", stage.display());
    }

    #[test]
    fn an_empty_root_reports_every_download_missing_with_its_size() {
        let root = temp_root("empty");
        let st = setup_status(&root);
        for c in st.components.iter().filter(|c| c.delivery == Delivery::Download && c.id != "ffmpeg") {
            assert!(!c.installed, "{} установлен в пустом корне", c.id);
            assert_eq!(c.bytes_on_disk, 0, "{}", c.id);
            assert!(c.space_needed >= c.size, "{}: места меньше размера закачки", c.id);
        }
        assert!(!st.ready);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_free_space_of_a_folder_not_yet_made_is_read_from_its_volume() {
        let root = temp_root("free");
        let free = free_bytes(&root.join("models").join("not-yet"));
        let _ = std::fs::remove_dir_all(&root);
        assert!(free.is_some_and(|b| b > 0), "{free:?}");
    }

    #[test]
    fn a_library_by_its_full_version_gets_the_name_the_loader_asks_for() {
        assert_eq!(soname_of("libggml-base.so.0.15.1").as_deref(), Some("libggml-base.so.0"));
        assert_eq!(soname_of("libggml-base.0.15.1.dylib").as_deref(), Some("libggml-base.0.dylib"));
        assert_eq!(soname_of("libllama-common.0.7.0.dylib").as_deref(), Some("libllama-common.0.dylib"));
        for name in ["libggml.0.dylib", "libqwen.dylib", "libggml.so.0", "libfoo.so", "bs_roformer-cli", "libonnxruntime.dylib"] {
            assert_eq!(soname_of(name), None, "{name}");
        }
    }

    #[test]
    fn the_check_of_a_download_moves_the_bar() {
        let root = temp_root("verify-bar");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("big.bin");
        std::fs::write(&file, vec![7u8; 70 << 20]).unwrap();
        let seen = std::sync::Mutex::new(Vec::new());
        let hash = sha256_file_with(&file, &|| false, &|n| seen.lock().unwrap().push(n)).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(hash.is_some());
        assert_eq!(*seen.lock().unwrap(), vec![64 << 20, 70 << 20], "every 64 MiB and at the end");
    }

    #[test]
    fn a_program_counts_only_as_the_pinned_build() {
        const ENGINE: FileSpec = FileSpec { url: "https://example.invalid/e.dll", dest_rel: "models/eng/e.dll", size: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", extract: Extract::None };
        assert!(is_program(&ENGINE));
        let root = temp_root("program");
        let dll = root.join(ENGINE.dest_rel);
        std::fs::create_dir_all(dll.parent().unwrap()).unwrap();

        std::fs::write(&dll, b"abd").unwrap();
        assert!(!file_installed(&root, &ENGINE), "другая сборка того же размера");
        assert_eq!(read_record(&root, &ENGINE).unwrap().sha256, "a52d159f262b2c6ddb724a61840befc36eb30c88877a4030b65cbe86298449c9");
        assert!(!file_installed(&root, &ENGINE), "запись о другой сборке, без повторного хэша");

        std::fs::write(&dll, b"abc").unwrap();
        std::fs::remove_file(record_path(&root, &ENGINE)).unwrap();
        assert!(file_installed(&root, &ENGINE), "файл без записи хэшируется один раз");
        assert_eq!(read_record(&root, &ENGINE).unwrap().sha256, ENGINE.sha256);

        std::fs::write(&dll, b"ab").unwrap();
        assert!(!file_installed(&root, &ENGINE), "не тот размер");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_program_marker_is_a_file_under_tools_without_an_extension() {
        assert!(is_program_marker("tools/audiocpp/audiocpp_server"));
        assert!(is_program_marker("tools/bsroformer/bs_roformer-cli"));
        assert!(!is_program_marker("tools/audiocpp/audiocpp_server.exe"));
        assert!(!is_program_marker("tools/llama/libggml.so"));
        assert!(!is_program_marker("models/higgs-engine/libcudart.so.13"));
        assert!(!is_program_marker("models/eng/engine"));
    }

    /// A Linux archive packed on Windows (no execute bit on its program) still installs a program that runs.
    #[cfg(unix)]
    #[test]
    fn a_program_unpacked_without_its_execute_bit_is_made_executable() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_root("exec-bit");
        let dir = root.join("tools/srv");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("_server.tar.gz");
        {
            let gz = flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::fast());
            let mut tar = tar::Builder::new(gz);
            let mut h = tar::Header::new_gnu();
            h.set_size(4);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, "srv/prog", &b"#!/x"[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let written = extract_archive(&archive, "tools/srv/_server.tar.gz", Extract::Flat, &dir).unwrap();
        let prog = dir.join("prog");
        assert!(written.contains(&prog), "{written:?}");
        assert_eq!(std::fs::metadata(&prog).unwrap().permissions().mode() & 0o111, 0, "the archive's own mode");
        let c = Component {
            id: "srv",
            name: "srv".into(),
            purpose: String::new(),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: 0,
            files: &[],
            markers: &[Marker { rel: "tools/srv/prog", expect: 0 }],
            external_url: None,
        };
        make_programs_executable(&root, &[&c]);
        assert_eq!(std::fs::metadata(&prog).unwrap().permissions().mode() & 0o777, 0o755);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resumed_bytes_come_from_the_chunk_manifest() {
        let root = temp_root("resume");
        let part = root.join("x.part");
        let total = CHUNK * 2 + 10;
        sized(&part, total);
        assert_eq!(resumed_bytes(&part, total), 0, "без манифеста ничего не готово");
        let offs: Vec<u8> = [0u64, CHUNK * 2, CHUNK * 2].iter().flat_map(|o| o.to_le_bytes()).collect();
        std::fs::write(done_manifest_path(&part), offs).unwrap();
        assert_eq!(resumed_bytes(&part, total), CHUNK + 10, "повтор офсета не считается дважды, хвост — по размеру");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removal_frees_the_files_and_the_partial_download() {
        let root = temp_root("remove");
        let all = manifest();
        let tiny = all.iter().find(|c| c.id == "langid").unwrap();
        for f in tiny.files {
            sized(&root.join(f.dest_rel), f.size);
        }
        let leftover = part_path(&root, &all.iter().find(|c| c.id == "roformer-q4").unwrap().files[0],
        );
        sized(&leftover, 1000);
        assert!(component_status(&root, tiny).installed);

        let r = remove_components(&root, &["langid".to_string(), "roformer-q4".to_string()],
        ).unwrap();
        assert_eq!(r.removed, vec!["langid".to_string(), "roformer-q4".to_string()]);
        assert_eq!(r.freed_bytes, tiny.size + 1000);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(!component_status(&root, tiny).installed);
        assert!(!root.join("models/langid-voxlingua107").exists(), "пустой каталог модели снят");
        assert!(root.join("models").is_dir(), "models/ остаётся");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removal_refuses_what_the_app_does_not_install() {
        let root = temp_root("refuse");
        assert_eq!(remove_components(&root, &["ocr".to_string()]).unwrap_err().code, "not_removable");
        assert_eq!(remove_components(&root, &["nvidia-driver".to_string()]).unwrap_err().code, "not_removable");
        assert_eq!(remove_components(&root, &["nope".to_string()]).unwrap_err().code, "unknown_component");
        let _busy = try_claim(&["roformer-q5".to_string()]).unwrap();
        assert_eq!(remove_components(&root, &["roformer-q5".to_string()]).unwrap_err().code, "busy");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn import_takes_only_files_of_the_exact_size() {
        let root = temp_root("import-dst");
        let src = temp_root("import-src");
        let all = manifest();
        let tiny = all.iter().find(|c| c.id == "langid").unwrap();
        for f in tiny.files {
            let name = Path::new(f.dest_rel).file_name().unwrap();
            sized(&src.join("nested").join(name), f.size);
        }
        sized(&src.join(".hidden").join("model.bin"), 1);
        sized(&src.join("other").join("config.json"), 7);
        let r = import_from_dir(&root, &src, Some("langid"), true, false);
        assert_eq!(r.imported, vec!["langid".to_string()]);
        assert_eq!(r.files, tiny.files.len());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(component_status(&root, tiny).installed);
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&src).ok();
    }

    #[test]
    fn claims_keep_one_writer_per_component() {
        let a = try_claim(&["roformer-q4".to_string()]).unwrap();
        assert!(try_claim(&["roformer-q4".to_string(), "roformer-q5".to_string()]).is_none());
        assert!(try_claim(&["roformer-q5".to_string()]).is_some(), "другой компонент качается параллельно");
        drop(a);
        assert!(try_claim(&["roformer-q4".to_string()]).is_some());
    }

    #[test]
    fn a_model_another_program_has_is_linked_in_and_says_where_from() {
        let root = temp_root("neighbor-dst");
        let other = temp_root("neighbor-src");
        let tiny = manifest().into_iter().find(|c| c.id == "langid").unwrap();
        // the neighbour keeps the model in a folder of the same name, under its own layout
        let in_neighbor = |rel: &str| {
            let rel = Path::new(rel);
            other.join("karaoke").join(rel.parent().unwrap().file_name().unwrap()).join(rel.file_name().unwrap())
        };
        for f in tiny.files {
            sized(&in_neighbor(f.dest_rel), f.size);
        }
        // the same names and sizes in another model's folder are not taken: nothing proves they are this model
        let decoy = temp_root("neighbor-decoy");
        for f in tiny.files {
            sized(&decoy.join("other-model").join(Path::new(f.dest_rel).file_name().unwrap()), f.size);
        }
        assert!(import_from_neighbors(&root, &[("ace".to_string(), decoy.clone())], false, None).is_empty());

        let took = import_from_neighbors(&root, &[("yue2".to_string(), other.clone())], false, None);
        assert!(took.contains(&("langid".to_string(), "yue2".to_string())));
        let status = component_status(&root, &tiny);
        assert!(status.installed);
        assert_eq!(status.taken_from.as_deref(), Some("yue2"));
        let dest = root.join(tiny.files[0].dest_rel);
        std::fs::write(in_neighbor(tiny.files[0].dest_rel), vec![7u8; tiny.files[0].size as usize]).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap()[0], 7, "a hard link: one file on disk for both programs");
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&other).ok();
        std::fs::remove_dir_all(&decoy).ok();
    }

    /// Заголовок в том виде, в каком его шлёт Hugging Face.
    #[test]
    fn the_rate_limit_header_is_read_the_way_the_hub_writes_it() {
        assert_eq!(parse_rate_limit("\"resolvers\";r=2819;t=216"), Some((2819, 216)));
        assert_eq!(parse_rate_limit("\"api\";r=0"), Some((0, 300)));
        assert_eq!(parse_rate_limit("garbage"), None);
    }

    #[test]
    fn a_busy_server_is_waited_for_as_long_as_it_asks() {
        let mut h = ureq::http::HeaderMap::new();
        assert_eq!(asked_to_wait(&h), 30, "без заголовков — полминуты");
        h.insert("ratelimit", "\"resolvers\";r=0;t=143".parse().unwrap());
        assert_eq!(asked_to_wait(&h), 143);
        h.insert("retry-after", "7".parse().unwrap());
        assert_eq!(asked_to_wait(&h), 7, "Retry-After важнее окна");
        h.insert("retry-after", "100000".parse().unwrap());
        assert_eq!(asked_to_wait(&h), 310, "не дольше пяти минут за раз");
    }

    #[test]
    fn sha256_is_streamed_and_can_be_stopped() {
        let root = temp_root("sha");
        let p = root.join("f.bin");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p, &|| false).unwrap().unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn all_installed_on_this_machine() {
        // Манифест против реального диска. repo_root — из DUB_STUDIO_ROOT (задаётся в CI/приёмке), иначе
        // CARGO_MANIFEST_DIR/../..; без models/ тест пропускается.
        let repo_root = std::env::var("DUB_STUDIO_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
            });
        if !repo_root.join("models").is_dir() {
            eprintln!("skip: нет {}/models (не приёмочная машина)", repo_root.display());
            return;
        }
        let st = setup_status(&repo_root);
        let mut broken = Vec::new();
        for c in &st.components {
            // Драйвер зависит от железа; опциональные кванты — взаимозаменяемые альтернативы дефолту.
            if c.delivery == Delivery::External || c.requirement == Requirement::Optional {
                continue;
            }
            if !c.installed {
                broken.push(format!("{} (missing: {:?})", c.id, c.missing));
            }
        }
        assert!(broken.is_empty(), "не установлены компоненты: {broken:?}");
    }

    /// tar.gz как у redist NVIDIA: ссылки раньше своих целей и цепочкой, заглушки stubs/ с теми же именами,
    /// заголовки рядом. Libs кладёт плоско одни библиотеки, и каждая ссылка читается как настоящий файл.
    #[test]
    fn a_tar_archive_unpacks_its_libraries_and_links() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = if cfg!(windows) { ("foo.dll", "foo64.dll") } else { ("libfoo.so.1.2.3", "libfoo.so.1") };
        let archive = tmp.path().join("pkg.tar.gz");
        {
            let gz = flate2::write::GzEncoder::new(File::create(&archive).unwrap(), flate2::Compression::fast());
            let mut tar = tar::Builder::new(gz);
            let link = |tar: &mut tar::Builder<_>, name: &str, target: &str| {
                let mut h = tar::Header::new_gnu();
                h.set_entry_type(tar::EntryType::Symlink);
                h.set_size(0);
                h.set_mode(0o777);
                tar.append_link(&mut h, name, target).unwrap();
            };
            let file = |tar: &mut tar::Builder<_>, name: &str, body: &[u8]| {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o755);
                tar.append_data(&mut h, name, body).unwrap();
            };
            link(&mut tar, &format!("pkg/lib/{}-alias", lib.1), lib.1);
            link(&mut tar, &format!("pkg/lib/{}", lib.1), &format!("./{}", lib.0));
            file(&mut tar, &format!("pkg/lib/stubs/{}", lib.1), b"stub");
            file(&mut tar, &format!("pkg/lib/{}", lib.0), b"real library");
            file(&mut tar, "pkg/include/foo.h", b"header");
            tar.into_inner().unwrap().finish().unwrap();
        }
        assert_eq!(archive_format("x/_engine.tar.xz"), ArchiveFormat::TarXz);
        assert_eq!(archive_format("models/runtime/_ort.tgz"), ArchiveFormat::TarGz);
        assert_eq!(archive_format("tools/whisper/_whisper.zip"), ArchiveFormat::Zip);
        let out = tmp.path().join("out");
        let written = extract_archive(&archive, "pkg.tar.gz", Extract::Libs, &out).unwrap();
        assert_eq!(std::fs::read(out.join(lib.0)).unwrap(), b"real library");
        assert_eq!(std::fs::read(out.join(lib.1)).unwrap(), b"real library", "the link, not the stub");
        assert!(!out.join("foo.h").exists() && !out.join("include").exists());
        assert!(written.iter().all(|p| p.parent() == Some(out.as_path())), "{written:?}");
        let tree = tmp.path().join("tree");
        extract_archive(&archive, "pkg.tar.gz", Extract::Tree, &tree).unwrap();
        assert_eq!(std::fs::read(tree.join("pkg/include/foo.h")).unwrap(), b"header");
    }

    /// Прежний Whisper уходит: всё, что назвала его запись, и россыпь CUDA 11 без записи; чужое остаётся.
    #[test]
    fn the_former_whisper_goes_and_other_files_stay() {
        let root = tempfile::tempdir().unwrap();
        let w = root.path().join("tools/whisper");
        std::fs::create_dir_all(&w).unwrap();
        for f in ["whisper-faster.exe", "cublas64_11.dll", "cudnn_ops_infer64_8.dll", "notes.txt"] {
            std::fs::write(w.join(f), b"x").unwrap();
        }
        std::fs::write(w.join("old-extra.dll"), b"x").unwrap();
        std::fs::write(
            w.join("_whisper.zip.json"),
            r#"{"sha256":"s","url":"u","files":[{"path":"tools/whisper/old-extra.dll","size":1},{"path":"models/elsewhere.bin","size":1}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join("models")).unwrap();
        std::fs::write(root.path().join("models/elsewhere.bin"), b"x").unwrap();
        std::fs::write(w.join("license.txt"), b"x").unwrap();
        std::fs::create_dir_all(w.join("_models/faster-whisper-small")).unwrap();
        std::fs::write(w.join("_models/faster-whisper-small/model.bin"), b"x").unwrap();
        retire_whisper(root.path(), &delete);
        for gone in ["whisper-faster.exe", "cublas64_11.dll", "cudnn_ops_infer64_8.dll", "old-extra.dll", "_whisper.zip.json", "license.txt", "_models"] {
            assert!(!w.join(gone).exists(), "{gone}");
        }
        for kept in ["notes.txt"] {
            assert!(w.join(kept).is_file(), "{kept}");
        }
        assert!(root.path().join("models/elsewhere.bin").is_file());
        std::fs::remove_file(w.join("notes.txt")).unwrap();
        std::fs::write(w.join("license.txt"), b"x").unwrap();
        retire_whisper(root.path(), &delete);
        assert!(!w.exists(), "the emptied folder goes");
    }

    /// A neighbour's file that cannot be linked (another drive) leaves the program's own file in place.
    #[test]
    fn a_link_that_fails_keeps_the_file_it_was_to_replace() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("tools/yt-dlp/yt-dlp.exe");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"older build").unwrap();
        assert!(adopt_file(&root.path().join("nowhere/yt-dlp.exe"), &dest, false).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"older build");
        let src = root.path().join("neighbour.exe");
        std::fs::write(&src, b"pinned build").unwrap();
        adopt_file(&src, &dest, false).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"pinned build");
        assert!(!with_suffix(&dest, ".import").exists());
    }

    /// A link that fails leaves no folder made for it (a neighbour's engine on another drive), and a folder that was
    /// there stays.
    #[test]
    fn a_link_that_fails_leaves_no_empty_folder() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("models/transcribe-engine-vulkan/transcribe_engine.dll");
        assert!(adopt_file(&root.path().join("nowhere/transcribe_engine.dll"), &dest, false).is_err());
        assert!(!dest.parent().unwrap().exists());
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        assert!(adopt_file(&root.path().join("nowhere/transcribe_engine.dll"), &dest, false).is_err());
        assert!(dest.parent().unwrap().is_dir());
    }

    /// A program taken from another folder replaces one of another build: the record of that build goes with it, so
    /// the new file is hashed and recorded as what it is instead of counting as the old build for good.
    #[test]
    fn a_program_taken_from_a_neighbour_drops_the_record_of_the_build_it_replaced() {
        let c = manifest().into_iter().find(|c| c.id == "transcribe-engine-cpu").unwrap();
        let f = &c.files[0];
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join(f.dest_rel);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::File::create(&dest).unwrap().set_len(f.size - 2048).unwrap();
        write_record_of(root.path(), f, "stale", &[dest.clone()]).unwrap();
        let neighbour = tempfile::tempdir().unwrap();
        let src = neighbour.path().join(Path::new(f.dest_rel).file_name().unwrap());
        std::fs::write(&src, vec![7u8; f.size as usize]).unwrap();
        let report = import_from_dir(root.path(), neighbour.path(), Some(c.id), true, false);
        assert_eq!(report.files, 1, "{:?}", report.errors);
        assert!(!component_status(root.path(), &c).installed, "not the pinned build");
        let record = read_record(root.path(), f).expect("recorded again");
        assert_ne!(record.sha256, "stale", "the record names the file now in place");
    }

    /// Vendor libraries the studio downloads whose AVX-512 the vendor picks at run time (they run on every x86-64
    /// processor their vendor supports) and that carry no symbols: NVIDIA's CUDA libraries, Microsoft's VC++ runtime and
    /// ONNX Runtime (MLAS picks its kernels by the processor), denoland's deno (V8 and simdutf do the same).
    const VENDOR_DISPATCHED: &[&str] = &[
        "cublas64_", "cublaslt64_", "cufft64_", "cufftw64_", "cudnn", "cudart64_", "nvblas64_", "vcruntime140", "msvcp140", "vcomp140",
        "onnxruntime", "libonnxruntime", "deno",
    ];
    /// What a package brings that the studio never loads: ONNX Runtime's TensorRT provider (the studio takes its CUDA
    /// one), which imports TensorRT.
    const NEVER_LOADED: &[&str] = &["onnxruntime_providers_tensorrt.dll", "libonnxruntime_providers_tensorrt.so"];

    /// Every binary of a program folder (DUB_COMPAT_ROOT: a dev root or an installed or unpacked release, read only)
    /// against the release checks of dub_core::compat: AVX-512 outside CPUID dispatchers, the Ubuntu 22.04 limits and
    /// RUNPATH of Linux libraries, imports Windows cannot resolve (the folders the studio puts on PATH count), and the
    /// execute bit of the components' programs. DUB_COMPAT_MAPS: link maps of stripped binaries, `name=path` joined by
    /// `;` (`Dub-Studio.exe=D:/…/Dub-Studio.map`).
    #[test]
    #[ignore]
    fn a_program_folder_passes_the_release_checks() {
        let Ok(root) = std::env::var("DUB_COMPAT_ROOT") else { return };
        let root = PathBuf::from(root);
        let mut maps = std::collections::BTreeMap::new();
        for pair in std::env::var("DUB_COMPAT_MAPS").unwrap_or_default().split(';').filter(|p| !p.is_empty()) {
            let (name, path) = pair.split_once('=').expect("name=path");
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
            maps.insert(name.to_string(), dub_core::compat::Functions::from_link_map(&text));
        }
        // the VC++ runtime the studio takes from the system folder when it is there instead of downloading it
        let system = system_dir();
        let from_system: Vec<String> = manifest()
            .into_iter()
            .filter(|c| FOUND_IN_SYSTEM_DIR.contains(&c.id))
            .flat_map(|c| c.markers)
            .filter(|m| !root.join(m.rel).is_file())
            .filter_map(|m| Path::new(m.rel).file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|name| system.as_ref().is_some_and(|dir| dir.join(name).is_file()))
            .collect();
        let provided: Vec<&str> = from_system.iter().map(String::as_str).collect();
        let how = dub_core::compat::Tree {
            path_folders: &["tools/ffmpeg", "tools/llama", "models/cuda12-runtime", "models/higgs-engine"],
            provided: &provided,
            maps,
            vendor_dispatched: VENDOR_DISPATCHED,
            unused: NEVER_LOADED,
        };
        let found = dub_core::compat::check_tree(&root, &how).unwrap();
        let mut lines: Vec<String> = Vec::new();
        for (path, problems) in &found {
            let rel = path.strip_prefix(&root).unwrap_or(path).display().to_string();
            lines.extend(problems.iter().map(|p| format!("{rel}: {p}")));
        }
        for c in manifest() {
            for m in c.markers.iter().filter(|m| is_program_marker(m.rel)) {
                let path = root.join(m.rel);
                if path.is_file() && !dub_core::compat::executable(&path).unwrap() {
                    lines.push(format!("{}: {}", m.rel, dub_core::compat::Problem::NotExecutable));
                }
            }
        }
        for line in &lines {
            println!("[ERROR] {line}");
        }
        assert!(lines.is_empty(), "{} problems in {}", lines.len(), root.display());
        println!("[OK] {}", root.display());
    }
}
