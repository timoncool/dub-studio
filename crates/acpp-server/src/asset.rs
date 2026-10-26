//! The published `audiocpp_server` of the studios (fork timoncool/audio.cpp, branch studio, 6d6cb6a5; model specs inside, AVX2
//! floor, no AVX-512) and what it needs to start: one build per CUDA major, chosen by the card, a Vulkan build for
//! other cards and a processor build, and the CUDA libraries a CUDA build imports. A studio downloads the assets into its own folders and checks the libraries before
//! it starts the server (`missing_libraries`); without them Windows refuses the program (0xC0000135).

use std::path::Path;

/// One file to download: an archive of the server or a wheel of CUDA libraries, pinned by size and sha256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asset {
    pub url: &'static str,
    /// The file name of the download.
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// A build of the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Build {
    /// SASS 7.5-12.0a without PTX, cudart linked in; cuBLAS 13 and cuFFT 12 beside it or on PATH. Driver 580+.
    Cuda13,
    /// SASS 5.2-12.0a without PTX; cudart 12, cuBLAS 12 and cuFFT 11 from [`CUDA12_RUNTIME`]. Driver 525+.
    Cuda12,
    /// Any card with a Vulkan driver (AMD, Intel; NVIDIA only when chosen, not in «auto»).
    Vulkan,
    /// The processor alone.
    Cpu,
}

/// The oldest card of each build and the CUDA version its driver has to report (cuDriverGetVersion: 1000 * major + 10
/// * minor); minor version compatibility runs 13.x and 12.9 on the first driver of their major.
pub const CUDA13_OLDEST: (u32, u32) = (7, 5);
pub const CUDA13_DRIVER_API: u32 = 13_000;
pub const CUDA12_OLDEST: (u32, u32) = (5, 2);
pub const CUDA12_DRIVER_API: u32 = 12_000;

impl Build {
    /// The build for a card of this compute capability on a driver of this CUDA version: CUDA 13 where it runs, else
    /// CUDA 12.9; None — neither.
    pub fn for_card(compute: (u32, u32), cuda_driver: u32) -> Option<Build> {
        if compute >= CUDA13_OLDEST && cuda_driver >= CUDA13_DRIVER_API {
            Some(Build::Cuda13)
        } else if compute >= CUDA12_OLDEST && cuda_driver >= CUDA12_DRIVER_API {
            Some(Build::Cuda12)
        } else {
            None
        }
    }

    /// The backend the server of this build is started with.
    pub const fn backend(self) -> crate::Backend {
        match self {
            Build::Cuda13 | Build::Cuda12 => crate::Backend::Cuda,
            Build::Vulkan => crate::Backend::Vulkan,
            Build::Cpu => crate::Backend::Cpu,
        }
    }

    /// The folder of the build under the program's tools folder; the archive unpacks flat into it.
    pub const fn folder(self) -> &'static str {
        match self {
            Build::Cuda13 => "audiocpp",
            Build::Cuda12 => "audiocpp-cuda12",
            Build::Vulkan => "audiocpp-vulkan",
            Build::Cpu => "audiocpp-cpu",
        }
    }

    /// The archive of the build: on Windows the program and the VC++ 14.51 libraries it needs (msvcp140, msvcp140_1,
    /// msvcp140_codecvt_ids, vcruntime140, vcruntime140_1, vcomp140), on Linux the program and libgomp. None where it is
    /// not published (macOS).
    pub const fn server(self) -> Option<Asset> {
        if cfg!(windows) {
            Some(match self {
                Build::Cuda13 => SERVER_WIN_CUDA13,
                Build::Cuda12 => SERVER_WIN_CUDA12,
                Build::Vulkan => SERVER_WIN_VULKAN,
                Build::Cpu => SERVER_WIN_CPU,
            })
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(match self {
                Build::Cuda13 => SERVER_LINUX_CUDA13,
                Build::Cuda12 => SERVER_LINUX_CUDA12,
                Build::Vulkan => SERVER_LINUX_VULKAN,
                Build::Cpu => SERVER_LINUX_CPU,
            })
        } else {
            None
        }
    }

    /// The CUDA libraries the program imports and the archive does not carry.
    pub const fn libraries(self) -> &'static [&'static str] {
        if !cfg!(windows) {
            return &[];
        }
        match self {
            Build::Cuda13 => &["cublas64_13.dll", "cublasLt64_13.dll", "cufft64_12.dll"],
            Build::Cuda12 => &["cudart64_12.dll", "cublas64_12.dll", "cublasLt64_12.dll", "cufft64_11.dll"],
            Build::Vulkan | Build::Cpu => &[],
        }
    }
}

/// The Windows archives of the builds (release server-6d6cb6a5).
pub const SERVER_WIN_CUDA13: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-win-cuda13-x64.zip",
    file: "audiocpp-server-win-cuda13-x64.zip",
    size: 131_000_798,
    sha256: "95a1dfa336d53ad67a7d89f1b33a9a251555e3eac2ab1ab81a61b9c4052408c1",
};
pub const SERVER_WIN_CUDA12: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-win-cuda12-x64.zip",
    file: "audiocpp-server-win-cuda12-x64.zip",
    size: 178_064_747,
    sha256: "99c2282399310b9952a3d07ced0f00ce22b429b5352235298359ff39fa9d726a",
};
pub const SERVER_WIN_VULKAN: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-win-vulkan-x64.zip",
    file: "audiocpp-server-win-vulkan-x64.zip",
    size: 33_125_129,
    sha256: "7c7a1d865381b2d6141156a3c7f353862f4965628b993bb50871abd4a5936240",
};
pub const SERVER_WIN_CPU: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-win-cpu-x64.zip",
    file: "audiocpp-server-win-cpu-x64.zip",
    size: 16_194_219,
    sha256: "b85b2f0fccad6d9776fe73a8b94403d32b9709cb3c61c5a43e12439c99c8201a",
};

/// The Linux x86-64 archives of the builds (release server-6d6cb6a5; Ubuntu 22.04, glibc 2.35): the program and libgomp,
/// RUNPATH $ORIGIN, ../../models/higgs-engine (CUDA 13 libraries) and ../../models/cuda12-runtime (CUDA 12.9).
pub const SERVER_LINUX_CUDA13: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-linux-cuda13-x64.tar.gz",
    file: "audiocpp-server-linux-cuda13-x64.tar.gz",
    size: 140_035_250,
    sha256: "9d36a9b8dd655419309c0e6d3905681725ba0fd6668e5ac1d241f080343363f3",
};
pub const SERVER_LINUX_CUDA12: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-linux-cuda12-x64.tar.gz",
    file: "audiocpp-server-linux-cuda12-x64.tar.gz",
    size: 186_686_738,
    sha256: "b29ad83f54f6e99475623cdaa565328bb194705f0d6ae4ae37b8c7562a83e364",
};
pub const SERVER_LINUX_VULKAN: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-linux-vulkan-x64.tar.gz",
    file: "audiocpp-server-linux-vulkan-x64.tar.gz",
    size: 41_266_364,
    sha256: "48ef418399b85a8ca80b56b763fa4196d619cbc487c8dd82a4a43284fa45217b",
};
pub const SERVER_LINUX_CPU: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-linux-cpu-x64.tar.gz",
    file: "audiocpp-server-linux-cpu-x64.tar.gz",
    size: 23_865_331,
    sha256: "9893b970f431d5a9bb2290a9963834cd38afc56c2ef20e13ef8449e6065bc4ff",
};

/// The CUDA 12.9 libraries of the CUDA 12 build (PyPI wheels; their `.dll` files unpack flat): cudart 12.9.79,
/// cuBLAS 12.9.2.10 (with cuBLASLt), cuFFT 11.4.1.4. The CUDA 13 build takes the studio's own CUDA 13 runtime.
pub const CUDA12_RUNTIME: &[Asset] = &[
    Asset {
        url: "https://files.pythonhosted.org/packages/59/df/e7c3a360be4f7b93cee39271b792669baeb3846c58a4df6dfcf187a7ffab/nvidia_cuda_runtime_cu12-12.9.79-py3-none-win_amd64.whl",
        file: "nvidia_cuda_runtime_cu12-12.9.79-py3-none-win_amd64.whl",
        size: 3_591_604,
        sha256: "8e018af8fa02363876860388bd10ccb89eb9ab8fb0aa749aaf58430a9f7c4891",
    },
    Asset {
        url: "https://files.pythonhosted.org/packages/20/e2/fc9a0e985249d873150276d5afb02e39a66817fedbf1a385724393e505ed/nvidia_cublas_cu12-12.9.2.10-py3-none-win_amd64.whl",
        file: "nvidia_cublas_cu12-12.9.2.10-py3-none-win_amd64.whl",
        size: 553_162_896,
        sha256: "623f43027d40d44ceadf0043f002bd25cf353e8f13ce90b9a87057019f560661",
    },
    Asset {
        url: "https://files.pythonhosted.org/packages/20/ee/29955203338515b940bd4f60ffdbc073428f25ef9bfbce44c9a066aedc5c/nvidia_cufft_cu12-11.4.1.4-py3-none-win_amd64.whl",
        file: "nvidia_cufft_cu12-11.4.1.4-py3-none-win_amd64.whl",
        size: 200_067_309,
        sha256: "8e5bfaac795e93f80611f807d42844e8e27e340e0cde270dcb6c65386d795b80",
    },
];

/// The libraries of `build` found in none of `dirs` (the program's folder first, then where the studio puts its CUDA
/// runtime): the reason the program would not start.
pub fn missing_libraries(build: Build, dirs: &[&Path]) -> Vec<&'static str> {
    build.libraries().iter().copied().filter(|lib| !dirs.iter().any(|d| d.join(lib).is_file())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_picks_the_build() {
        assert_eq!(Build::for_card((8, 9), 13_040), Some(Build::Cuda13));
        assert_eq!(Build::for_card(CUDA13_OLDEST, CUDA13_DRIVER_API), Some(Build::Cuda13));
        assert_eq!(Build::for_card((8, 6), 12_080), Some(Build::Cuda12), "RTX 30 on driver 566");
        assert_eq!(Build::for_card((6, 1), 13_000), Some(Build::Cuda12), "Pascal");
        assert_eq!(Build::for_card((7, 0), 12_020), Some(Build::Cuda12), "V100");
        assert_eq!(Build::for_card(CUDA12_OLDEST, CUDA12_DRIVER_API), Some(Build::Cuda12), "Maxwell 2 on driver 525");
        assert_eq!(Build::for_card((5, 0), 13_000), None, "Maxwell 1");
        assert_eq!(Build::for_card((8, 6), 11_080), None, "a driver of the CUDA 11 branch");
    }

    #[test]
    fn a_build_names_its_backend_and_folder_and_only_cuda_needs_libraries() {
        assert_eq!(Build::Cuda12.backend(), crate::Backend::Cuda);
        assert_eq!(Build::Vulkan.backend(), crate::Backend::Vulkan);
        assert_eq!(Build::Cpu.backend(), crate::Backend::Cpu);
        let folders = [Build::Cuda13, Build::Cuda12, Build::Vulkan, Build::Cpu].map(Build::folder);
        assert_eq!(folders.iter().collect::<std::collections::HashSet<_>>().len(), 4, "{folders:?}");
        assert!(Build::Vulkan.libraries().is_empty() && Build::Cpu.libraries().is_empty());
    }

    #[test]
    fn a_library_counts_in_any_folder_given() {
        let root = std::env::temp_dir().join(format!("acpp-libs-{}", std::process::id()));
        let (beside, runtime) = (root.join("tools"), root.join("runtime"));
        std::fs::create_dir_all(&beside).unwrap();
        std::fs::create_dir_all(&runtime).unwrap();
        let libs = Build::Cuda12.libraries();
        for (i, lib) in libs.iter().enumerate() {
            std::fs::write(if i % 2 == 0 { beside.join(lib) } else { runtime.join(lib) }, b"x").unwrap();
        }
        assert!(missing_libraries(Build::Cuda12, &[&beside, &runtime]).is_empty());
        if let Some(first) = libs.first() {
            std::fs::remove_file(beside.join(first)).unwrap();
            assert_eq!(missing_libraries(Build::Cuda12, &[&beside, &runtime]), vec![*first]);
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
