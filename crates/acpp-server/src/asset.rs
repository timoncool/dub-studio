//! The published `audiocpp_server` of the studios (fork timoncool/audio.cpp, branch studio, a8a621a1 on Windows and Linux,
//! 6d6cb6a5 on the Mac; model specs inside, one ggml-cpu module per instruction level chosen by cpuid, so any x86-64 runs it) and what it needs to start: one build per CUDA major, chosen by the card, a Vulkan build for
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
    /// A Mac on Apple silicon: Metal and the processor in one program.
    Metal,
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
            Build::Metal => crate::Backend::Metal,
        }
    }

    /// The build of this Mac: Metal on Apple silicon; None off macOS and on an Intel Mac, for which none is published.
    pub const fn for_this_mac() -> Option<Build> {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Build::Metal)
        } else {
            None
        }
    }

    /// The folder of the build under the program's tools folder; the archive unpacks flat into it.
    pub const fn folder(self) -> &'static str {
        match self {
            Build::Cuda13 => "audiocpp",
            Build::Cuda12 => "audiocpp-cuda12",
            Build::Vulkan => "audiocpp-vulkan",
            Build::Cpu => "audiocpp-cpu",
            Build::Metal => "audiocpp-metal",
        }
    }

    /// The archive of the build: on Windows the program and the VC++ 14.51 libraries it needs (msvcp140, msvcp140_1,
    /// msvcp140_codecvt_ids, vcruntime140, vcruntime140_1, vcomp140), on Linux the program and libgomp, on a Mac with
    /// Apple silicon the Metal program. None where a build is not published (Metal off the Mac, the others on it).
    pub const fn server(self) -> Option<Asset> {
        if cfg!(windows) {
            match self {
                Build::Cuda13 => Some(SERVER_WIN_CUDA13),
                Build::Cuda12 => Some(SERVER_WIN_CUDA12),
                Build::Vulkan => Some(SERVER_WIN_VULKAN),
                Build::Cpu => Some(SERVER_WIN_CPU),
                Build::Metal => None,
            }
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            match self {
                Build::Cuda13 => Some(SERVER_LINUX_CUDA13),
                Build::Cuda12 => Some(SERVER_LINUX_CUDA12),
                Build::Vulkan => Some(SERVER_LINUX_VULKAN),
                Build::Cpu => Some(SERVER_LINUX_CPU),
                Build::Metal => None,
            }
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            match self {
                Build::Metal => Some(SERVER_MACOS),
                Build::Cuda13 | Build::Cuda12 | Build::Vulkan | Build::Cpu => None,
            }
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
            Build::Vulkan | Build::Cpu | Build::Metal => &[],
        }
    }
}

/// The Windows archives of the builds (release server-a8a621a1): the program, its ggml modules (one ggml-cpu-* per
/// instruction level, loaded by cpuid) and the VC++ libraries.
pub const SERVER_WIN_CUDA13: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-win-cuda13-x64.zip",
    file: "audiocpp-server-win-cuda13-x64.zip",
    size: 137_632_076,
    sha256: "ffbbf5ace977c9004a237b3184211fb6752ee9b3947aa6fbc717c5ef97a9d041",
};
pub const SERVER_WIN_CUDA12: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-win-cuda12-x64.zip",
    file: "audiocpp-server-win-cuda12-x64.zip",
    size: 185_474_916,
    sha256: "2338d6b812f61d1da1815e0bedf51cfce6beb139674db4f9c5aa31e732f74488",
};
pub const SERVER_WIN_VULKAN: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-win-vulkan-x64.zip",
    file: "audiocpp-server-win-vulkan-x64.zip",
    size: 37_463_816,
    sha256: "44d4164a18c5364eb488446e6a3bd967c11b0954b0e7ba36ab00b597bf50aa1a",
};
pub const SERVER_WIN_CPU: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-win-cpu-x64.zip",
    file: "audiocpp-server-win-cpu-x64.zip",
    size: 20_170_672,
    sha256: "c7dc6957f87214e00bdf963df8ced30cef47258c148eac22dcd2f4fd4ff7cd8d",
};

/// The Linux x86-64 archives of the builds (release server-a8a621a1; Ubuntu 22.04, glibc 2.35): the program (mode 0755)
/// and libgomp, RUNPATH $ORIGIN, ../../models/higgs-engine (CUDA 13 libraries) and ../../models/cuda12-runtime (CUDA 12.9).
pub const SERVER_LINUX_CUDA13: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-linux-cuda13-x64.tar.gz",
    file: "audiocpp-server-linux-cuda13-x64.tar.gz",
    size: 150_297_050,
    sha256: "a6ab0eb9a61ff68a322dcc9615fcd34972ee5e26aa84a608e5cddca1d97844c9",
};
pub const SERVER_LINUX_CUDA12: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-linux-cuda12-x64.tar.gz",
    file: "audiocpp-server-linux-cuda12-x64.tar.gz",
    size: 197_807_523,
    sha256: "0d9b66aeb742b2635ab7baebc8997a83cfb20192877dd4af1b7859e6cb39f957",
};
pub const SERVER_LINUX_VULKAN: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-linux-vulkan-x64.tar.gz",
    file: "audiocpp-server-linux-vulkan-x64.tar.gz",
    size: 49_183_434,
    sha256: "066b0015c3fe46d17033c7e959d5a57fa275eba762a183551d3fc0f5bf966403",
};
pub const SERVER_LINUX_CPU: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-a8a621a1/audiocpp-server-linux-cpu-x64.tar.gz",
    file: "audiocpp-server-linux-cpu-x64.tar.gz",
    size: 31_629_448,
    sha256: "b3c721e2424c287eb88a66ce6267f4d3af774f1ec4f539cbb3e50ca3fa93bcf4",
};

/// The macOS arm64 archive of the Metal build (release server-6d6cb6a5): the program with its libraries.
pub const SERVER_MACOS: Asset = Asset {
    url: "https://github.com/timoncool/audio.cpp/releases/download/server-6d6cb6a5/audiocpp-server-macos-arm64-metal.tar.gz",
    file: "audiocpp-server-macos-arm64-metal.tar.gz",
    size: 17_535_293,
    sha256: "fa5afc009fbafb09c48bfc48b8e15a77256d52623d72bd855d0f0664ddb1f345",
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
        assert_eq!(Build::Metal.backend(), crate::Backend::Metal);
        let folders = [Build::Cuda13, Build::Cuda12, Build::Vulkan, Build::Cpu, Build::Metal].map(Build::folder);
        assert_eq!(folders.iter().collect::<std::collections::HashSet<_>>().len(), 5, "{folders:?}");
        assert!(Build::Vulkan.libraries().is_empty() && Build::Cpu.libraries().is_empty() && Build::Metal.libraries().is_empty());
    }

    #[test]
    fn a_mac_on_apple_silicon_takes_the_metal_build_and_no_other_platform_does() {
        let on_apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(Build::for_this_mac(), on_apple_silicon.then_some(Build::Metal));
        assert_eq!(Build::Metal.server(), on_apple_silicon.then_some(SERVER_MACOS));
        if on_apple_silicon {
            assert!([Build::Cuda13, Build::Cuda12, Build::Vulkan, Build::Cpu].iter().all(|b| b.server().is_none()));
        }
        assert!(SERVER_MACOS.url.ends_with(SERVER_MACOS.file) && SERVER_MACOS.url.contains("/server-6d6cb6a5/"));
        assert_eq!(serde_json::to_value(crate::Backend::Metal).unwrap(), "metal", "the server's config names it so");
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
