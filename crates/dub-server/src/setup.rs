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
//!   • сайдкары/движки — zip-релизы GitHub (BSRoformer.cpp v0.1.0, llama.cpp b11146 win-cuda-13.4,
//!     onnxruntime 1.28.2, ffmpeg BtbN, deno 2.9.7) + audiocpp_engine.dll (своя сборка с sm_75, GitHub timoncool/Higgs-Ultimate) + yt-dlp.exe 2026.08.19;
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

// Сборка llama.cpp одна на обе платформы (стабильный релиз v0.5.0 = билд b11146).
const GH_LLAMA_BUILD: &str = "b11146";

/// Windows x64: zip-релизы, PyPI-колёса win_amd64, redist NVIDIA windows-x86_64.
#[cfg(windows)]
mod platform {
    use super::{Extract, FileSpec, Marker, Parts};

    // GitHub: BSRoformer.cpp движок win-cuda-13.1.0 zip (chenmozhijin/BSRoformer.cpp v0.1.0).
    const GH_BSROFORMER_ENGINE: &str =
        "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-cuda-13.1.0.zip";
    // GitHub: BSRoformer.cpp CPU-сборка (win-x64-msvc, без CUDA) — сепарация на процессоре: медленнее,
    // но полная функция. Статический exe 671КБ; MSVC-рантайм уже вшит компонентом vcruntime.
    const GH_BSROFORMER_ENGINE_CPU: &str =
        "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip";
    // GitHub: llama.cpp win-cuda-13.4 (ggml-org/llama.cpp; пин на стабильный релиз v0.5.0 = билд b11146).
    // ggml-cuda.dll импортирует cublas64_13.dll (ставит cuda-runtime), cudart слинкован статически.
    const GH_LLAMA: &str =
        "https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-cuda-13.4-x64.zip";
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
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-win64-gpl.zip";
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
    // CUDA-либы для whisper-faster r192.3 (CTranslate2, собран под CUDA 11!): нужны РЯДОМ с exe
    // (cublas64_11 + cudnn64_8), иначе GPU-режим Whisper падает «cublas64_11.dll not found» -> откат на CPU.
    // ВАЖНО: не-XXL сборка = CUDA 11 (cublas64_11), XXL = CUDA 12 (cublas64_12) — РАЗНЫЕ. Версии те, что
    // валидировал Purfview (cuBLAS 11.11.3.6 + cuDNN 8.9.7.29), но архивы у NVIDIA (Libs тянет библиотеки плоско).
    const REDIST_WHISPER_CUBLAS: &str = "https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.11.3.6-archive.zip";
    const REDIST_WHISPER_CUDNN: &str = "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-8.9.7.29_cuda11-archive.zip";
    // GitHub: yt-dlp 2026.08.19 (yt-dlp/yt-dlp, SHA-256 из SHA2-256SUMS релиза) — загрузка видео по ссылке. Это опора:
    // более свежие версии ставит рядом ytdlp::update, при их провале работает эта.
    const GH_YTDLP: &str = "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp.exe";
    // GitHub: deno 2.9.7 (denoland/deno) — JS-рантайм, без которого yt-dlp не решает задачи YouTube (yt-dlp-ejs уже внутри
    // yt-dlp.exe; вики yt-dlp EJS: deno не ниже 2.3.0).
    const GH_DENO: &str = "https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip";
    pub const HIGGS_ENGINE: Parts = Parts {
        size: 129_390_080,
        files: &[
            FileSpec { url: "https://github.com/timoncool/Higgs-Ultimate/releases/download/engine-0.2.3-turing/audiocpp_engine.dll", dest_rel: "models/higgs-engine/audiocpp_engine.dll", size: 129_390_080, sha256: "c3608613bd54bdd41c85cc90412d4add67b928b07b32be24f26d91cb9626f46d", extract: Extract::None },
        ],
        markers: &[Marker { rel: "models/higgs-engine/audiocpp_engine.dll", expect: 129_390_080 }],
    };

    pub const WHISPER_ENGINE: Parts = Parts {
        size: 87_654_143,
        files: &[
            FileSpec { url: "https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r192.3_windows.zip", dest_rel: "tools/whisper/_whisper.zip", size: 87_654_143, sha256: "8150ad257fd8e46d817bb7e667260c2ce4c493d9e58973862e6409c592b44ba5", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/whisper/whisper-faster.exe", expect: 0 }],
    };

    pub const WHISPER_CUDA: Parts = Parts {
        size: 1_125_090_089,
        files: &[
            FileSpec { url: REDIST_WHISPER_CUBLAS, dest_rel: "tools/whisper/_wcublas.zip", size: 420_850_025, sha256: "67b0934a6359e4ee26fff823c356021589d392c4fd49ca12624f570edc08e2b9", extract: Extract::Libs },
            FileSpec { url: REDIST_WHISPER_CUDNN, dest_rel: "tools/whisper/_wcudnn.zip", size: 704_240_064, sha256: "5e45478efe71a96329e6c0d2a3a2f79c747c15b2a51fead4b84c89b02cbf1671", extract: Extract::Libs },
        ],
        markers: &[
            Marker { rel: "tools/whisper/cublas64_11.dll", expect: 0 },
            Marker { rel: "tools/whisper/cudnn64_8.dll", expect: 0 },
        ],
    };

    pub const BSROFORMER: Parts = Parts {
        size: 164_990_561,
        files: &[
            FileSpec { url: GH_BSROFORMER_ENGINE, dest_rel: "tools/bsroformer/_engine.zip", size: 164_990_561, sha256: "a7c330774c0a40ec4de09ca0613af48fdc23c28bd0d90212425697daf7b1db74", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/bsroformer/bs_roformer-cli.exe", expect: 0 }],
    };

    pub const BSROFORMER_CPU: Parts = Parts {
        size: 671_031,
        files: &[
            FileSpec { url: GH_BSROFORMER_ENGINE_CPU, dest_rel: "tools/bsroformer-cpu/_engine.zip", size: 671_031, sha256: "e002811d56605bce6a51c275cf8f9ba447a3707771289ea6fbcca7f4d3e9ba1f", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/bsroformer-cpu/bs_roformer-cli.exe", expect: 0 }],
    };

    pub const LLAMA: Parts = Parts {
        size: 149_758_833,
        files: &[
            FileSpec { url: GH_LLAMA, dest_rel: "tools/llama/_llama.zip", size: 149_758_833, sha256: "b1866c0ce76bc7bfb0c24b33e9a37e9669f1be18539b12c74ce361f81c41f047", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/llama/llama-server.exe", expect: 0 }],
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
        size: 170_732_198,
        files: &[
            FileSpec { url: GH_FFMPEG, dest_rel: "tools/ffmpeg/_ffmpeg.zip", size: 170_732_198, sha256: "b4da332540eaebc6939181b59e267f163dd57407ef6596f7f3452845921d1d91", extract: Extract::Pick },
        ],
        markers: &[Marker { rel: "tools/ffmpeg/ffmpeg.exe", expect: 0 }],
    };

    pub const YTDLP: Parts = Parts {
        size: 60_470_620,
        files: &[
            FileSpec { url: GH_YTDLP, dest_rel: "tools/yt-dlp/yt-dlp.exe", size: 17_840_399, sha256: "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a", extract: Extract::None },
            FileSpec { url: GH_DENO, dest_rel: "tools/yt-dlp/_deno.zip", size: 42_630_221, sha256: "a0c3101b4158d1dfb7d6a78a7bf0f3de80c96bb423c152beec8beb22786f2238", extract: Extract::Flat },
        ],
        markers: &[
            Marker { rel: "tools/yt-dlp/yt-dlp.exe", expect: 17_840_399 },
            Marker { rel: "tools/yt-dlp/deno.exe", expect: 97_462_048 },
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
#[cfg(not(windows))]
mod platform {
    use super::{Extract, FileSpec, Marker, Parts};

    // Движок Higgs, собранный под Linux из исходника (timoncool/Higgs-Ultimate, sm 75/80/86/89/90/120a): RUNPATH
    // $ORIGIN, CUDA-библиотеки берёт из своего каталога (их кладёт cuda-runtime).
    pub const HIGGS_ENGINE: Parts = Parts {
        size: 136_774_464,
        files: &[
            FileSpec { url: "https://github.com/timoncool/Higgs-Ultimate/releases/download/engine-0.2.3-turing/libaudiocpp_engine.so", dest_rel: "models/higgs-engine/libaudiocpp_engine.so", size: 136_774_464, sha256: "dae7b29111acf1e564f9afa541b2a4d360ba2cd6fc60eb4d5b8bdb9c04d722f6", extract: Extract::None },
        ],
        markers: &[Marker { rel: "models/higgs-engine/libaudiocpp_engine.so", expect: 136_774_464 }],
    };

    // Purfview faster-whisper r189.1 для Linux (последняя Linux-сборка не-XXL, CTranslate2 под CUDA 11).
    pub const WHISPER_ENGINE: Parts = Parts {
        size: 106_093_648,
        files: &[
            FileSpec { url: "https://github.com/Purfview/whisper-standalone-win/releases/download/faster-whisper/Whisper-Faster_r189.1_linux.zip", dest_rel: "tools/whisper/_whisper.zip", size: 106_093_648, sha256: "f32f5e7abbb53300e569ca5ab3e9dd286b63b5e212209c12d47bdd9c72f923dd", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/whisper/whisper-faster", expect: 0 }],
    };

    // cuBLAS 11.11.3.6 и cuDNN 8.9.7.29 (CUDA 11) — те же версии, что на Windows, из redist NVIDIA.
    pub const WHISPER_CUDA: Parts = Parts {
        size: 1_361_648_788,
        files: &[
            FileSpec { url: "https://developer.download.nvidia.com/compute/cuda/redist/libcublas/linux-x86_64/libcublas-linux-x86_64-11.11.3.6-archive.tar.xz", dest_rel: "tools/whisper/_wcublas.tar.xz", size: 500_681_532, sha256: "045e6455c9f8789b1c7ced19957c7904d23c221f4d1d75bb574a2c856aebae98", extract: Extract::Libs },
            FileSpec { url: "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/linux-x86_64/cudnn-linux-x86_64-8.9.7.29_cuda11-archive.tar.xz", dest_rel: "tools/whisper/_wcudnn.tar.xz", size: 860_967_256, sha256: "a3e2509028cecda0117ce5a0f42106346e82e86d390f4bb9475afc976c77402e", extract: Extract::Libs },
        ],
        markers: &[
            Marker { rel: "tools/whisper/libcublas.so.11", expect: 0 },
            Marker { rel: "tools/whisper/libcudnn.so.8", expect: 0 },
        ],
    };

    // BSRoformer.cpp v0.1.0 linux-cuda-13.1.0: RUNPATH указывает на каталог CI, свои libggml берёт через
    // LD_LIBRARY_PATH запуска (dub-sep).
    pub const BSROFORMER: Parts = Parts {
        size: 239_527_692,
        files: &[
            FileSpec { url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-linux-cuda-13.1.0.tar.xz", dest_rel: "tools/bsroformer/_engine.tar.xz", size: 239_527_692, sha256: "6d7e543f2985b785cfef2baf0b217fc2f891e6b5a03804ce2657b4eb70379126", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/bsroformer/bs_roformer-cli", expect: 0 }],
    };

    pub const BSROFORMER_CPU: Parts = Parts {
        size: 640_204,
        files: &[
            FileSpec { url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-linux-x64-cpu.tar.xz", dest_rel: "tools/bsroformer-cpu/_engine.tar.xz", size: 640_204, sha256: "bc0f20237b9ed263582ebd0844dfc7dbb61309a67c31f4a8d7ba156e21292c77", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/bsroformer-cpu/bs_roformer-cli", expect: 0 }],
    };

    // llama.cpp b11146 ubuntu-cuda-13.4: RUNPATH $ORIGIN для своих библиотек, cudart/cuBLAS 13 — из cuda-runtime,
    // libssl.so.3 — системная.
    pub const LLAMA: Parts = Parts {
        size: 149_265_156,
        files: &[
            FileSpec { url: "https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-ubuntu-cuda-13.4-x64.tar.gz", dest_rel: "tools/llama/_llama.tar.gz", size: 149_265_156, sha256: "1603d9c00a4b6eac8298c5c7868cdb080a3ac31948ab1e457441d71ce274dd7e", extract: Extract::Flat },
        ],
        markers: &[Marker { rel: "tools/llama/llama-server", expect: 0 }],
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
        size: 128_065_756,
        files: &[
            FileSpec { url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-N-126342-gf88b741dbf-linux64-gpl.tar.xz", dest_rel: "tools/ffmpeg/_ffmpeg.tar.xz", size: 128_065_756, sha256: "d1cf19f669510448f18a4cffcdbd8fa9592ee7c15c92feb5b96ad7e9ccc30114", extract: Extract::Pick },
        ],
        markers: &[Marker { rel: "tools/ffmpeg/ffmpeg", expect: 0 }],
    };

    pub const YTDLP: Parts = Parts {
        size: 82_043_018,
        files: &[
            FileSpec { url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_linux", dest_rel: "tools/yt-dlp/yt-dlp", size: 40_446_224, sha256: "58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a", extract: Extract::None },
            FileSpec { url: "https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-unknown-linux-gnu.zip", dest_rel: "tools/yt-dlp/_deno.zip", size: 41_596_794, sha256: "c6527f24f4b16031d3ae4fa9f658d5f11534c8d84ce7dc8502420280919c3490", extract: Extract::Flat },
        ],
        markers: &[
            Marker { rel: "tools/yt-dlp/yt-dlp", expect: 40_446_224 },
            Marker { rel: "tools/yt-dlp/deno", expect: 95_830_104 },
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

// Страница драйверов NVIDIA (кнопка «Открыть сайт» — драйвер DLL-кой не ставится).
pub const NVIDIA_DRIVER_URL: &str = "https://www.nvidia.com/Download/index.aspx";

// ═══════════════════════════════════════════════════════════════════════════
//  МАНИФЕСТ
// ═══════════════════════════════════════════════════════════════════════════

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
            size: platform::HIGGS_ENGINE.size,
            files: platform::HIGGS_ENGINE.files,
            markers: platform::HIGGS_ENGINE.markers,
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
            name: "Parakeet-TDT 0.6B v3 (int8)".into(),
            purpose: t!("setup-comp-parakeet-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            size: 670_619_803,
            files: &[
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx", dest_rel: "models/tdt/encoder-model.int8.onnx", size: 652_183_999, sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx", dest_rel: "models/tdt/decoder_joint-model.int8.onnx", size: 18_202_004, sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx", dest_rel: "models/tdt/nemo128.onnx", size: 139_764, sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt", dest_rel: "models/tdt/vocab.txt", size: 93_939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json", dest_rel: "models/tdt/config.json", size: 97, sha256: "666903c76b9798caf2c210afd4f6cd60b08a8dbf9800ec8d7a3bc0d2148ac466", extract: Extract::None },
            ],
            markers: &[
                Marker { rel: "models/tdt/encoder-model.int8.onnx", expect: 652_183_999 },
                Marker { rel: "models/tdt/decoder_joint-model.int8.onnx", expect: 18_202_004 },
                Marker { rel: "models/tdt/vocab.txt", expect: 93_939 },
            ],
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
        // Альтернативный квант ASR: fp32 (точнее, тяжелее int8). Отдельная папка (fp32 приоритетнее int8).
        Component {
            id: "parakeet-fp32",
            name: "Parakeet-TDT 0.6B v3 (fp32)".into(),
            purpose: t!("setup-comp-parakeet-fp32-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 2_549_945_719,
            files: &[
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx", dest_rel: "models/tdt-fp32/encoder-model.onnx", size: 41_770_866, sha256: "98a74b21b4cc0017c1e7030319a4a96f4a9506e50f0708f3a516d02a77c96bb1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.onnx.data", dest_rel: "models/tdt-fp32/encoder-model.onnx.data", size: 2_435_420_160, sha256: "9a22d372c51455c34f13405da2520baefb7125bd16981397561423ed32d24f36", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.onnx", dest_rel: "models/tdt-fp32/decoder_joint-model.onnx", size: 72_520_893, sha256: "e978ddf6688527182c10fde2eb4b83068421648985ef23f7a86be732be8706c1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx", dest_rel: "models/tdt-fp32/nemo128.onnx", size: 139_764, sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt", dest_rel: "models/tdt-fp32/vocab.txt", size: 93_939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json", dest_rel: "models/tdt-fp32/config.json", size: 97, sha256: "666903c76b9798caf2c210afd4f6cd60b08a8dbf9800ec8d7a3bc0d2148ac466", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/tdt-fp32/encoder-model.onnx", expect: 41_770_866 }, Marker { rel: "models/tdt-fp32/encoder-model.onnx.data", expect: 2_435_420_160 }, Marker { rel: "models/tdt-fp32/vocab.txt", expect: 93_939 }],
            external_url: None,
        },
        // Parakeet Ultra (Moondream): дообученный parakeet-tdt-0.6b-v3 той же архитектуры, словаря и 25 языков,
        // fp32. ONNX — altunenes/parakeet-rs/parakeet-ultra; nemo128.onnx и config.json дополняют папку до
        // раскладки tdt-fp32 (файлы байт-в-байт те же, что у базовой модели). Ревизии закреплены sha коммитов.
        Component {
            id: "parakeet-ultra",
            name: "Parakeet Ultra 0.6B (fp32)".into(),
            purpose: t!("setup-comp-parakeet-ultra-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 2_596_031_917,
            files: &[
                FileSpec { url: "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx", dest_rel: "models/tdt-ultra/encoder-model.onnx", size: 87_857_063, sha256: "76f835e57d62d82f1485c7a84706782e44a123a69f4efa86ed3b4ad56e236051", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/encoder-model.onnx.data", dest_rel: "models/tdt-ultra/encoder-model.onnx.data", size: 2_435_420_160, sha256: "6aeb9438f1f45dafc17d27c61a12bc406c0c2ccb8c17219aeeb3f898c283a8e6", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/decoder_joint-model.onnx", dest_rel: "models/tdt-ultra/decoder_joint-model.onnx", size: 72_520_894, sha256: "a5911fe202e8fba44251fce252a6c9c7c0a7c724c882a13f81d96611fa2d7ccb", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/parakeet-ultra/vocab.txt", dest_rel: "models/tdt-ultra/vocab.txt", size: 93_939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/tdt/nemo128.onnx", dest_rel: "models/tdt-ultra/nemo128.onnx", size: 139_764, sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/config.json", dest_rel: "models/tdt-ultra/config.json", size: 97, sha256: "666903c76b9798caf2c210afd4f6cd60b08a8dbf9800ec8d7a3bc0d2148ac466", extract: Extract::None },
            ],
            markers: &[
                Marker { rel: "models/tdt-ultra/encoder-model.onnx", expect: 87_857_063 },
                Marker { rel: "models/tdt-ultra/encoder-model.onnx.data", expect: 2_435_420_160 },
                Marker { rel: "models/tdt-ultra/decoder_joint-model.onnx", expect: 72_520_894 },
                Marker { rel: "models/tdt-ultra/vocab.txt", expect: 93_939 },
            ],
            external_url: None,
        },
        // Parakeet Ultra int8 (Masterx): та же дообученная Moondream модель, квантованная тем же рецептом, что
        // istupakov int8 базовой. Своя папка: энкодер в пределах допуска по размеру от базового int8, а декодер
        // и словарь совпадают с ним по размеру при других байтах.
        Component {
            id: "parakeet-ultra-int8",
            name: "Parakeet Ultra 0.6B (int8)".into(),
            purpose: t!("setup-comp-parakeet-ultra-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 670_619_018,
            files: &[
                FileSpec { url: "https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/encoder-model.int8.onnx", dest_rel: "models/tdt-ultra-int8/encoder-model.int8.onnx", size: 652_183_214, sha256: "46e78f85f1ae43b43bd359889c966da5c7ca401df60a42eeffc2e7699b357ba6", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/decoder_joint-model.int8.onnx", dest_rel: "models/tdt-ultra-int8/decoder_joint-model.int8.onnx", size: 18_202_004, sha256: "2276a335d4c8dc48686e931475a634595954956427d485830e1214c0d1a18d07", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx", dest_rel: "models/tdt-ultra-int8/nemo128.onnx", size: 139_764, sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/vocab.txt", dest_rel: "models/tdt-ultra-int8/vocab.txt", size: 93_939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx/resolve/99b09f030a5a6efeaa13cf2cf54592100ce2c3f1/config.json", dest_rel: "models/tdt-ultra-int8/config.json", size: 97, sha256: "666903c76b9798caf2c210afd4f6cd60b08a8dbf9800ec8d7a3bc0d2148ac466", extract: Extract::None },
            ],
            markers: &[
                Marker { rel: "models/tdt-ultra-int8/encoder-model.int8.onnx", expect: 652_183_214 },
                Marker { rel: "models/tdt-ultra-int8/decoder_joint-model.int8.onnx", expect: 18_202_004 },
                Marker { rel: "models/tdt-ultra-int8/vocab.txt", expect: 93_939 },
            ],
            external_url: None,
        },
        // ── АЛЬТЕРНАТИВНЫЙ ASR-ДВИЖОК: Whisper (Purfview standalone faster-whisper) ──────────
        // Бинарь-onefile (CTranslate2 CPU из коробки; GPU опц. с CUDA11-либами). Выбор в настройках:
        // движок Parakeet/Whisper + РАЗНЫЕ модели (tiny…large-v3-turbo) + РАЗНЫЕ кванты (compute_type).
        Component {
            id: "whisper-engine",
            name: t!("setup-comp-whisper-engine-name"),
            purpose: t!("setup-comp-whisper-engine-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::WHISPER_ENGINE.size,
            files: platform::WHISPER_ENGINE.files,
            markers: platform::WHISPER_ENGINE.markers,
            external_url: None,
        },
        // CUDA-либы для GPU-режима Whisper (cuBLAS 12 + cuDNN 8) РЯДОМ с whisper-faster.exe. Без них
        // whisper-faster (CTranslate2) не видит CUDA и падает на CPU — «GPU выбран, а работает на CPU».
        // Качается по запросу, когда стартует Whisper-джоба на GPU (см. ensure_job_components).
        Component {
            id: "whisper-cuda",
            name: t!("setup-comp-whisper-cuda-name"),
            purpose: t!("setup-comp-whisper-cuda-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: platform::WHISPER_CUDA.size,
            files: platform::WHISPER_CUDA.files,
            markers: platform::WHISPER_CUDA.markers,
            external_url: None,
        },
        Component {
            id: "whisper-tiny",
            name: t!("setup-comp-whisper-tiny-name"),
            purpose: t!("setup-comp-whisper-tiny-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 78_203_619,
            files: &[
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/model.bin", dest_rel: "models/whisper/faster-whisper-tiny/model.bin", size: 75_538_270, sha256: "dcb76c6586fc06cbdac6dd21f14cfd129cc4cdd9dce19bf4ffa62e59cbe6e6d1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/config.json", dest_rel: "models/whisper/faster-whisper-tiny/config.json", size: 2_249, sha256: "a73a28cdfe1c43ccc7202fa333d1f89c202477271407ae9a7f19afa52039cac8", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/tokenizer.json", dest_rel: "models/whisper/faster-whisper-tiny/tokenizer.json", size: 2_203_239, sha256: "fb7b63191e9bb045082c79fd742a3106a12c99513ab30df4a0d47fa6cb6fd0ab", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-tiny/resolve/d90ca5fe260221311c53c58e660288d3deb8d356/vocabulary.txt", dest_rel: "models/whisper/faster-whisper-tiny/vocabulary.txt", size: 459_861, sha256: "34ce3fe1c5041027b3f8d42912270993f986dbc4bb34cf27f951e34a1e453913", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/whisper/faster-whisper-tiny/model.bin", expect: 75_538_270 }, Marker { rel: "models/whisper/faster-whisper-tiny/tokenizer.json", expect: 2_203_239 }],
            external_url: None,
        },
        Component {
            id: "whisper-base",
            name: t!("setup-comp-whisper-base-name"),
            purpose: t!("setup-comp-whisper-base-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 147_882_941,
            files: &[
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/model.bin", dest_rel: "models/whisper/faster-whisper-base/model.bin", size: 145_217_532, sha256: "d01c3014881c9c6f3133c182f3d2887eb6ca1c789a7538c5c007196857a0a6a9", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/config.json", dest_rel: "models/whisper/faster-whisper-base/config.json", size: 2_309, sha256: "56a6d8110d311f19c8f0471e562832c7527f146b567275bfca59fcf7c184da9a", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/tokenizer.json", dest_rel: "models/whisper/faster-whisper-base/tokenizer.json", size: 2_203_239, sha256: "fb7b63191e9bb045082c79fd742a3106a12c99513ab30df4a0d47fa6cb6fd0ab", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-base/resolve/ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66/vocabulary.txt", dest_rel: "models/whisper/faster-whisper-base/vocabulary.txt", size: 459_861, sha256: "34ce3fe1c5041027b3f8d42912270993f986dbc4bb34cf27f951e34a1e453913", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/whisper/faster-whisper-base/model.bin", expect: 145_217_532 }, Marker { rel: "models/whisper/faster-whisper-base/tokenizer.json", expect: 2_203_239 }],
            external_url: None,
        },
        Component {
            id: "whisper-small",
            name: t!("setup-comp-whisper-small-name"),
            purpose: t!("setup-comp-whisper-small-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 486_212_372,
            files: &[
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/model.bin", dest_rel: "models/whisper/faster-whisper-small/model.bin", size: 483_546_902, sha256: "3e305921506d8872816023e4c273e75d2419fb89b24da97b4fe7bce14170d671", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/config.json", dest_rel: "models/whisper/faster-whisper-small/config.json", size: 2_370, sha256: "b55496ac7940a7ae47d2c01eab40edfd8701feec1229d9cce3b40014383fb828", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/tokenizer.json", dest_rel: "models/whisper/faster-whisper-small/tokenizer.json", size: 2_203_239, sha256: "fb7b63191e9bb045082c79fd742a3106a12c99513ab30df4a0d47fa6cb6fd0ab", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-small/resolve/536b0662742c02347bc0e980a01041f333bce120/vocabulary.txt", dest_rel: "models/whisper/faster-whisper-small/vocabulary.txt", size: 459_861, sha256: "34ce3fe1c5041027b3f8d42912270993f986dbc4bb34cf27f951e34a1e453913", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/whisper/faster-whisper-small/model.bin", expect: 483_546_902 }, Marker { rel: "models/whisper/faster-whisper-small/tokenizer.json", expect: 2_203_239 }],
            external_url: None,
        },
        Component {
            id: "whisper-medium",
            name: t!("setup-comp-whisper-medium-name"),
            purpose: t!("setup-comp-whisper-medium-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 1_530_571_735,
            files: &[
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/model.bin", dest_rel: "models/whisper/faster-whisper-medium/model.bin", size: 1_527_906_378, sha256: "9b45e1009dcc4ab601eff815b61d80e60ce3fd8c74c1a14f4a282258286b51ae", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/config.json", dest_rel: "models/whisper/faster-whisper-medium/config.json", size: 2_257, sha256: "3622a2ddc41ec0e0fd4e68c13c6830f03b90c38d89aaad184de02c8c642cf807", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/tokenizer.json", dest_rel: "models/whisper/faster-whisper-medium/tokenizer.json", size: 2_203_239, sha256: "fb7b63191e9bb045082c79fd742a3106a12c99513ab30df4a0d47fa6cb6fd0ab", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-medium/resolve/08e178d48790749d25932bbc082711ddcfdfbc4f/vocabulary.txt", dest_rel: "models/whisper/faster-whisper-medium/vocabulary.txt", size: 459_861, sha256: "34ce3fe1c5041027b3f8d42912270993f986dbc4bb34cf27f951e34a1e453913", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/whisper/faster-whisper-medium/model.bin", expect: 1_527_906_378 }, Marker { rel: "models/whisper/faster-whisper-medium/tokenizer.json", expect: 2_203_239 }],
            external_url: None,
        },
        Component {
            id: "whisper-large-v3",
            name: t!("setup-comp-whisper-large-v3-name"),
            purpose: t!("setup-comp-whisper-large-v3-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 3_090_835_702,
            files: &[
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/model.bin", dest_rel: "models/whisper/faster-whisper-large-v3/model.bin", size: 3_087_284_237, sha256: "69f74147e3334731bc3a76048724833325d2ec74642fb52620eda87352e3d4f1", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/config.json", dest_rel: "models/whisper/faster-whisper-large-v3/config.json", size: 2_394, sha256: "a9306624f5ec14270a014b647e5c316b6e03a662c369758d1b90697a7b0655b9", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/preprocessor_config.json", dest_rel: "models/whisper/faster-whisper-large-v3/preprocessor_config.json", size: 340, sha256: "7ccc62c6f2765af1f3b46c00c9b5894426835a05021c8b9c01eecb6dfb542711", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/tokenizer.json", dest_rel: "models/whisper/faster-whisper-large-v3/tokenizer.json", size: 2_480_617, sha256: "6d8cbd7cd0d8d5815e478dac67b85a26bbe77c1f5e0c6d76d1ce2abc0e5f21ca", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/Systran/faster-whisper-large-v3/resolve/edaa852ec7e145841d8ffdb056a99866b5f0a478/vocabulary.json", dest_rel: "models/whisper/faster-whisper-large-v3/vocabulary.json", size: 1_068_114, sha256: "c69260f2ab26d659b7c398f9a2b2b48ed0df16c3b47d7326782fd9cba71690c1", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/whisper/faster-whisper-large-v3/model.bin", expect: 3_087_284_237 }, Marker { rel: "models/whisper/faster-whisper-large-v3/tokenizer.json", expect: 2_480_617 }],
            external_url: None,
        },
        Component {
            id: "whisper-large-v3-turbo",
            name: t!("setup-comp-whisper-large-v3-turbo-name"),
            purpose: t!("setup-comp-whisper-large-v3-turbo-purpose"),
            requirement: Requirement::Optional,
            delivery: Delivery::Download,
            size: 1_621_665_983,
            files: &[
                FileSpec { url: "https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/model.bin", dest_rel: "models/whisper/faster-whisper-large-v3-turbo/model.bin", size: 1_617_884_929, sha256: "e76620f83d5f5b69efd3d87e3dc180c1bd21df9fbebacfd4335e5e1efcc018da", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/config.json", dest_rel: "models/whisper/faster-whisper-large-v3-turbo/config.json", size: 2_263, sha256: "b0253ea6c0d3bea6b1e19e91a02acfd3b53f4467362efcb5a3e6b16c9b3a9b7e", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/preprocessor_config.json", dest_rel: "models/whisper/faster-whisper-large-v3-turbo/preprocessor_config.json", size: 340, sha256: "7ccc62c6f2765af1f3b46c00c9b5894426835a05021c8b9c01eecb6dfb542711", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/tokenizer.json", dest_rel: "models/whisper/faster-whisper-large-v3-turbo/tokenizer.json", size: 2_710_337, sha256: "297b13372ac43916285644fb9687add3cc62ee2a1adb60da3dc25cc94c1871fd", extract: Extract::None },
                FileSpec { url: "https://huggingface.co/deepdml/faster-whisper-large-v3-turbo-ct2/resolve/4df90f75321148c3a29a9e2351b7ddf8f5b115a8/vocabulary.json", dest_rel: "models/whisper/faster-whisper-large-v3-turbo/vocabulary.json", size: 1_068_114, sha256: "c69260f2ab26d659b7c398f9a2b2b48ed0df16c3b47d7326782fd9cba71690c1", extract: Extract::None },
            ],
            markers: &[Marker { rel: "models/whisper/faster-whisper-large-v3-turbo/model.bin", expect: 1_617_884_929 }, Marker { rel: "models/whisper/faster-whisper-large-v3-turbo/tokenizer.json", expect: 2_710_337 }],
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
            id: "llama",
            name: t!("setup-comp-llama-name"),
            purpose: t!("setup-comp-llama-purpose"),
            requirement: Requirement::Required,
            delivery: Delivery::Download,
            // Размер сжатого zip (для прогресса закачки); распакованный footprint ~183 МБ.
            size: platform::LLAMA.size,
            files: platform::LLAMA.files,
            markers: platform::LLAMA.markers,
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
}

/// Оценка VRAM загруженной модели по id (движки/рантаймы = 0). Грубо, для показа в UI.
fn vram_estimate(id: &str) -> u64 {
    let gb = |g: f64| (g * 1024.0 * 1024.0 * 1024.0) as u64;
    match id {
        "higgs" => gb(5.6),
        "higgs-q6_k" => gb(5.1),
        "higgs-q4_k_m" => gb(4.2),
        "gemma" => gb(8.5),
        "gemma-q5_0" => gb(9.9),
        "gemma-q6_k" => gb(11.2),
        "gemma-q8_0" => gb(14.0),
        "parakeet" => gb(1.1),
        "parakeet-fp32" => gb(2.7),
        "parakeet-ultra" => gb(2.7),
        "parakeet-ultra-int8" => gb(1.1),
        "sortformer" => gb(0.5),
        "roformer" => gb(0.5),
        "roformer-q5" => gb(0.45),
        "roformer-q4" => gb(0.4),
        _ => 0,
    }
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
struct ArchiveRecord {
    sha256: String,
    url: String,
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

fn read_record(repo_root: &Path, f: &FileSpec) -> Option<ArchiveRecord> {
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

fn file_installed(repo_root: &Path, f: &FileSpec) -> bool {
    match f.extract {
        Extract::None => file_ok(&repo_root.join(f.dest_rel), f.size),
        _ => archive_installed(repo_root, f),
    }
}

fn write_record(repo_root: &Path, f: &FileSpec, written: &[PathBuf]) -> Result<(), DlError> {
    let files = written
        .iter()
        .map(|p| RecordedFile {
            path: p.strip_prefix(repo_root).unwrap_or(p).to_string_lossy().replace('\\', "/"),
            size: file_len(p),
        })
        .collect();
    let rec = ArchiveRecord { sha256: f.sha256.to_string(), url: f.url.to_string(), files,
    };
    let path = record_path(repo_root, f);
    let tmp = with_suffix(&path, ".tmp");
    let body = serde_json::to_vec_pretty(&rec).map_err(|e| DlError::new("io", t!("setup-install-record", error = e.to_string())))?;
    std::fs::write(&tmp, body).map_err(|e| DlError::new("io", t!("common-write", what = tmp.display().to_string(), error = e.to_string())))?;
    std::fs::rename(&tmp, &path).map_err(|e| DlError::new("io", t!("common-write", what = path.display().to_string(), error = e.to_string())))
}

/// ffmpeg доступен в системном PATH? (Command::new("ffmpeg") найдёт его при рендере.)
fn ffmpeg_on_path() -> bool {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
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
/// копия через .part + rename (частичной копии под финальным именем не бывает).
fn adopt_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    if std::fs::hard_link(src, dest).is_ok() {
        return Ok(());
    }
    let tmp = with_suffix(dest, ".import");
    std::fs::copy(src, &tmp)?;
    std::fs::rename(&tmp, dest)
}

/// Импорт готовых весов из папки без закачки: для каждого недостающего ПРЯМОГО файла компонента ищем в src_dir
/// файл того же имени и ТОЧНО того же размера. Архивные компоненты (движки, CUDA) не импортируются: по россыпи
/// файлов не проверить, что это закреплённая версия, — их докачивает «Первый запуск».
pub fn import_from_dir(repo_root: &Path, src_dir: &Path, only: Option<&str>) -> ImportReport {
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
            let src = match c.markers.iter().find(|m| m.rel == f.dest_rel) {
                Some(m) => pick_import_source(&c, m, &map).filter(|p| file_len(p) == f.size),
                None => Path::new(f.dest_rel)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase())
                    .and_then(|base| map.get(&base))
                    .and_then(|cands| cands.iter().find(|(_, sz)| *sz == f.size))
                    .map(|(p, _)| p),
            };
            let Some(src) = src else { continue };
            match adopt_file(src, &dest) {
                Ok(()) => {
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

// ── Место на диске ───────────────────────────────────────────────────────────

/// Свободное место на томе пути (для пользователя, с учётом квот), байт. Путь может ещё не существовать —
/// берём ближайший существующий каталог. None — ОС не ответила (или не Windows): место тогда не проверяем.
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
pub fn free_bytes(_path: &Path) -> Option<u64> {
    None
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
        let is_asr = c.id.starts_with("parakeet") || c.id.starts_with("whisper");
        if (cloud_llm && is_gemma) || (cloud_tts && is_higgs) || (cloud_asr && is_asr) {
            c.requirement = Requirement::Optional;
        }
    }
    // Backend локальных стадий: в CPU-режиме (без NVIDIA) CUDA-компоненты не нужны — сепарация идёт
    // CPU-сборкой, тяжёлое в облаке. CPU-движок сепарации становится рекомендованным (преселект на
    // первом запуске), а CUDA-движок + CUDA-рантайм — необязательными (не тянем лишние гигабайты).
    // В GPU-режиме наоборот: CPU-движок не нужен.
    let backend = crate::models::local_backend(&mroot);
    for c in comps.iter_mut() {
        if backend == "cpu" {
            if c.id == "bsroformer-engine-cpu" {
                c.requirement = Requirement::Recommended;
            } else if c.id == "bsroformer-engine" || c.id == "cuda-runtime" || c.id == "onnxruntime-gpu" || c.id == "cudnn" {
                c.requirement = Requirement::Optional;
            }
        } else if c.id == "bsroformer-engine-cpu" {
            c.requirement = Requirement::Optional;
        }
    }
    if backend == "gpu" {
        fit_to_vram(&mut comps, crate::hw::total_vram());
    }
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

/// Построить ureq-агента для закачки. Статусы 4xx/5xx читаем сами (429 и заголовки ожидания). Прокси из
/// active.json — все запросы через него; иначе Config::default берёт HTTP(S)_PROXY из env.
fn dl_agent() -> Result<ureq::Agent, DlError> {
    let mut cfg = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)));
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
    use sha2::{Digest, Sha256};
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut blocks = 0u32;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
        blocks += 1;
        if blocks.is_multiple_of(64) && stop() {
            return Ok(None);
        }
    }
    Ok(Some(digest.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    ))
}

fn discard_part(part: &Path) {
    let _ = std::fs::remove_file(part);
    let _ = std::fs::remove_file(done_manifest_path(part));
}

/// Проверить скачанный .part (размер и SHA-256 против закреплённых) и опубликовать: прямой файл — rename в
/// финал, архив — распаковка + запись об установке. Несовпадение — .part удаляется, следующая попытка с нуля.
fn publish(repo_root: &Path, f: &FileSpec, part: &Path, cancel: &dyn Fn() -> bool, progress: &ProgressCb,
) -> Result<(), DlError> {
    let size = file_len(part);
    if size != f.size {
        discard_part(part);
        return Err(DlError::new(
            "size_mismatch",
            t!("setup-size-mismatch", file = f.dest_rel, got = size, want = f.size),
        ));
    }
    progress(json!({ "stage": "download", "phase": "verify", "file": f.dest_rel, "msg": t!("setup-verifying", file = f.dest_rel) }),
    );
    let hash = sha256_file(part, cancel).map_err(|e| DlError::new("io", t!("common-read", path = part.display().to_string(), error = e.to_string())))?;
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
    let mroot = crate::models_root(repo_root);
    let mut results = Vec::new();
    for c in selected {
        let st = component_status(repo_root, c);
        if st.installed {
            for (engine, variant) in crate::models::component_selection(c.id) {
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

/// Скачать набор компонентов по id (идемпотентно: целые файлы закреплённой версии пропускаются, уже лежащий
/// файл перед пропуском сверяется по SHA-256). Тело синхронное: зовут фоновый поток менеджера или джоба.
/// cancel — пауза: скачанное остаётся в .part с манифестом чанков и докачивается следующим запуском.
pub fn download_components(
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
                    let hash = sha256_file(&dest, cancel).map_err(|e| {
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
    for p in &planned {
        match publish(repo_root, p.f, &p.part, cancel, progress) {
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

/// Как `ldconfig -n`: библиотеке `libX.so.0.15.1` без соседней `libX.so.0` поставить эту ссылку — по ней
/// её ищет загрузчик (CPU-сборка BSRoformer для Linux приходит одними полными именами).
#[cfg(unix)]
fn add_soname_links(written: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut added = Vec::new();
    for path in written.iter() {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Some(at) = name.find(".so.") else { continue };
        let version: Vec<&str> = name[at + 4..].split('.').collect();
        if version.len() < 2 || !version.iter().all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        let soname = format!("{}.so.{}", &name[..at], version[0]);
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
#[cfg(test)]
mod tests {
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
        for id in ["sortformer", "parakeet-ultra", "parakeet-ultra-int8"] {
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

    fn index(files: &[(&str, u64)]) -> std::collections::HashMap<String, Vec<(PathBuf, u64)>> {
        let mut map: std::collections::HashMap<String, Vec<(PathBuf, u64)>> = Default::default();
        for (p, sz) in files {
            let p = PathBuf::from(p);
            let base = p.file_name().unwrap().to_string_lossy().to_lowercase();
            map.entry(base).or_default().push((p, *sz));
        }
        map
    }

    const FP32_DIR: &[(&str, u64)] = &[
        ("src/fp32/encoder-model.onnx", 41_770_866),
        ("src/fp32/encoder-model.onnx.data", 2_435_420_160),
        ("src/fp32/decoder_joint-model.onnx", 72_520_893),
        ("src/fp32/vocab.txt", 93_939),
    ];
    const ULTRA_DIR: &[(&str, u64)] = &[
        ("src/ultra/encoder-model.onnx", 87_857_063),
        ("src/ultra/encoder-model.onnx.data", 2_435_420_160),
        ("src/ultra/decoder_joint-model.onnx", 72_520_894),
        ("src/ultra/vocab.txt", 93_939),
    ];

    fn picks(id: &str, map: &std::collections::HashMap<String, Vec<(PathBuf, u64)>>,
    ) -> Vec<Option<String>> {
        let c = comp(id);
        c.markers
            .iter()
            .map(|m| pick_import_source(&c, m, map).map(|p| p.to_string_lossy().replace('\\', "/")))
            .collect()
    }

    #[test]
    fn import_never_fills_ultra_from_fp32_folder() {
        let map = index(FP32_DIR);
        assert!(picks("parakeet-ultra", &map).iter().all(Option::is_none), "{:?}", picks("parakeet-ultra", &map));
        assert!(picks("parakeet-fp32", &map).iter().all(Option::is_some));
    }

    #[test]
    fn import_never_fills_fp32_from_ultra_folder() {
        let map = index(ULTRA_DIR);
        assert!(picks("parakeet-fp32", &map).iter().all(Option::is_none), "{:?}", picks("parakeet-fp32", &map));
        assert!(picks("parakeet-ultra", &map).iter().all(Option::is_some));
    }

    const INT8_DIR: &[(&str, u64)] = &[
        ("src/int8/encoder-model.int8.onnx", 652_183_999),
        ("src/int8/decoder_joint-model.int8.onnx", 18_202_004),
        ("src/int8/vocab.txt", 93_939),
    ];
    const ULTRA_INT8_DIR: &[(&str, u64)] = &[
        ("src/ultra-int8/encoder-model.int8.onnx", 652_183_214),
        ("src/ultra-int8/decoder_joint-model.int8.onnx", 18_202_004),
        ("src/ultra-int8/vocab.txt", 93_939),
    ];

    #[test]
    fn import_never_mixes_base_int8_and_ultra_int8() {
        let map = index(INT8_DIR);
        assert!(picks("parakeet-ultra-int8", &map).iter().all(Option::is_none), "{:?}", picks("parakeet-ultra-int8", &map));
        let map = index(ULTRA_INT8_DIR);
        assert!(picks("parakeet", &map).iter().all(Option::is_none), "{:?}", picks("parakeet", &map));
        let both: Vec<(&str, u64)> = INT8_DIR.iter().chain(ULTRA_INT8_DIR).copied().collect();
        let map = index(&both);
        for (id, dir) in [("parakeet", "src/int8/"), ("parakeet-ultra-int8", "src/ultra-int8/")] {
            for p in picks(id, &map) {
                assert!(p.as_deref().is_some_and(|p| p.starts_with(dir)), "{id}: {p:?}");
            }
        }
    }

    #[test]
    fn import_takes_each_variant_from_its_own_folder() {
        let both: Vec<(&str, u64)> = FP32_DIR.iter().chain(ULTRA_DIR).copied().collect();
        let map = index(&both);
        for (id, dir) in [("parakeet-ultra", "src/ultra/"), ("parakeet-fp32", "src/fp32/"),
        ] {
            for p in picks(id, &map) {
                assert!(p.as_deref().is_some_and(|p| p.starts_with(dir)), "{id}: {p:?}");
            }
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
        let tiny = all.iter().find(|c| c.id == "whisper-tiny").unwrap();
        for f in tiny.files {
            sized(&root.join(f.dest_rel), f.size);
        }
        let leftover = part_path(&root, &all.iter().find(|c| c.id == "whisper-base").unwrap().files[0],
        );
        sized(&leftover, 1000);
        assert!(component_status(&root, tiny).installed);

        let r = remove_components(&root, &["whisper-tiny".to_string(), "whisper-base".to_string()],
        ).unwrap();
        assert_eq!(r.removed, vec!["whisper-tiny".to_string(), "whisper-base".to_string()]);
        assert_eq!(r.freed_bytes, tiny.size + 1000);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(!component_status(&root, tiny).installed);
        assert!(!root.join("models/whisper/faster-whisper-tiny").exists(), "пустой каталог модели снят");
        assert!(root.join("models").is_dir(), "models/ остаётся");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removal_refuses_what_the_app_does_not_install() {
        let root = temp_root("refuse");
        assert_eq!(remove_components(&root, &["ocr".to_string()]).unwrap_err().code, "not_removable");
        assert_eq!(remove_components(&root, &["nvidia-driver".to_string()]).unwrap_err().code, "not_removable");
        assert_eq!(remove_components(&root, &["nope".to_string()]).unwrap_err().code, "unknown_component");
        let _busy = try_claim(&["whisper-small".to_string()]).unwrap();
        assert_eq!(remove_components(&root, &["whisper-small".to_string()]).unwrap_err().code, "busy");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn import_takes_only_files_of_the_exact_size() {
        let root = temp_root("import-dst");
        let src = temp_root("import-src");
        let all = manifest();
        let tiny = all.iter().find(|c| c.id == "whisper-tiny").unwrap();
        for f in tiny.files {
            let name = Path::new(f.dest_rel).file_name().unwrap();
            sized(&src.join("nested").join(name), f.size);
        }
        sized(&src.join(".hidden").join("model.bin"), 1);
        sized(&src.join("other").join("config.json"), 7);
        let r = import_from_dir(&root, &src, Some("whisper-tiny"));
        assert_eq!(r.imported, vec!["whisper-tiny".to_string()]);
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
}
