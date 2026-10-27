//! The pinned downloads of the separation engine BSRoformer.cpp v0.1.0, one per build, the same for every program.
//! The CUDA builds are the studios' (timoncool/BSRoformer.cpp studio-0.1.0: `-real` SASS without PTX, Ubuntu 22.04 on
//! Linux, RUNPATH `$ORIGIN` and the programs' CUDA runtime folders); Vulkan, the processor and the Mac are the author's
//! (chenmozhijin/BSRoformer.cpp v0.1.0).

/// A build of the engine, named as `acpp_server::asset::Build` names the server's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    Cuda13,
    Cuda12,
    Vulkan,
    Cpu,
}

/// One archive: where it comes from, where it lies under the program folder until it is unpacked, its size and SHA-256.
#[derive(Clone, Copy, Debug)]
pub struct Asset {
    pub url: &'static str,
    pub archive: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

impl Build {
    pub const ALL: [Build; 4] = [Build::Cuda13, Build::Cuda12, Build::Vulkan, Build::Cpu];

    /// The build's name, as [`crate::engine_cli_of`] takes it.
    pub const fn name(self) -> &'static str {
        match self {
            Build::Cuda13 => "cuda13",
            Build::Cuda12 => "cuda12",
            Build::Vulkan => "vulkan",
            Build::Cpu => "cpu",
        }
    }

    /// The component that downloads the build (the same id in every program, so one takes another's).
    pub const fn component(self) -> &'static str {
        match self {
            Build::Cuda13 => "bsroformer-engine",
            Build::Cuda12 => "bsroformer-engine-cuda12",
            Build::Vulkan => "bsroformer-engine-vulkan",
            Build::Cpu => "bsroformer-engine-cpu",
        }
    }

    /// The CUDA runtime component the build's program imports from: the CUDA 13 or the CUDA 12.9 one.
    pub const fn runtime_component(self) -> Option<&'static str> {
        match self {
            Build::Cuda13 => Some("cuda-runtime"),
            Build::Cuda12 => Some("cuda12-runtime"),
            Build::Vulkan | Build::Cpu => None,
        }
    }

    /// The build's archive on this platform; None where there is no such build (a Mac has the processor's alone, Metal in
    /// it).
    #[cfg(windows)]
    pub const fn engine(self) -> Option<Asset> {
        Some(match self {
            Build::Cuda13 => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.0/BSRoformer-windows-cuda13-x64.zip",
                archive: "tools/bsroformer/_engine.zip",
                size: 117_036_131,
                sha256: "a6cb96ec5435fc44855f45c8e4fb2223f7aa1390e4d21c2f0fe9f7d55eafbb41",
            },
            Build::Cuda12 => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.0/BSRoformer-windows-cuda12-x64.zip",
                archive: "tools/bsroformer-cuda12/_engine.zip",
                size: 170_818_717,
                sha256: "774dc9be48fd75a919ba1569991d3b6bb13e9fa18a98d7efe75cc53501f80837",
            },
            Build::Vulkan => Asset {
                url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-vulkan.zip",
                archive: "tools/bsroformer-vulkan/_engine.zip",
                size: 22_779_026,
                sha256: "a47653911eb17f68e65bce15aed6c4f1bd277a5fbd48293c4e54f0b005b869a7",
            },
            Build::Cpu => Asset {
                url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-windows-x64-msvc.zip",
                archive: "tools/bsroformer-cpu/_engine.zip",
                size: 671_031,
                sha256: "e002811d56605bce6a51c275cf8f9ba447a3707771289ea6fbcca7f4d3e9ba1f",
            },
        })
    }

    #[cfg(target_os = "macos")]
    pub const fn engine(self) -> Option<Asset> {
        match self {
            Build::Cpu => Some(Asset {
                url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-macos-arm64.tar.xz",
                archive: "tools/bsroformer-cpu/_engine.tar.xz",
                size: 636_996,
                sha256: "d0fc45181d31dcceea99c3828378f4a7376e52e7fdbc43e29a07e806785b4c9a",
            }),
            Build::Cuda13 | Build::Cuda12 | Build::Vulkan => None,
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub const fn engine(self) -> Option<Asset> {
        Some(match self {
            Build::Cuda13 => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.0/BSRoformer-linux-cuda13-x64.tar.gz",
                archive: "tools/bsroformer/_engine.tar.gz",
                size: 118_589_684,
                sha256: "d363cd9c2249417f4d93851e4958c1c6808fa18e5f8b564312ed124c5881bf63",
            },
            Build::Cuda12 => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.0/BSRoformer-linux-cuda12-x64.tar.gz",
                archive: "tools/bsroformer-cuda12/_engine.tar.gz",
                size: 172_267_870,
                sha256: "d6bc6ee2e0af9f7a430ab6f0b3ded880e764ea77ee68e97a1c689cee92083520",
            },
            Build::Vulkan => Asset {
                url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-linux-vulkan.tar.xz",
                archive: "tools/bsroformer-vulkan/_engine.tar.xz",
                size: 5_900_692,
                sha256: "bee2e9b5dd322b8d4fe1081583ce41709c2c1341550bbb557073c1216fef3e85",
            },
            Build::Cpu => Asset {
                url: "https://github.com/chenmozhijin/BSRoformer.cpp/releases/download/v0.1.0/BSRoformer-linux-x64-cpu.tar.xz",
                archive: "tools/bsroformer-cpu/_engine.tar.xz",
                size: 640_204,
                sha256: "bc0f20237b9ed263582ebd0844dfc7dbb61309a67c31f4a8d7ba156e21292c77",
            },
        })
    }

    /// The build a separation device runs (`cpu`, `vulkan`, `gpu`, `auto`) on a card whose CUDA build is `cuda` (13 or
    /// 12.9, none without an NVIDIA card that takes either), `other_make` telling a display adapter of another make,
    /// as Speech Studio chooses: «auto» is the card's CUDA build, else Vulkan on a card of another make, else the
    /// processor; an NVIDIA card runs Vulkan only when it is chosen. None when the card is chosen and it runs no CUDA
    /// build. A Mac has its one build for every choice.
    pub fn for_device(choice: &str, cuda: Option<Build>, other_make: bool) -> Option<Build> {
        if cfg!(target_os = "macos") {
            return Some(Build::Cpu);
        }
        match choice {
            "cpu" => Some(Build::Cpu),
            "vulkan" => Some(Build::Vulkan),
            "gpu" | "cuda" => cuda,
            _ => Some(cuda.unwrap_or(if other_make { Build::Vulkan } else { Build::Cpu })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_build_has_its_own_folder_and_component() {
        let mut components: Vec<&str> = Build::ALL.iter().map(|b| b.component()).collect();
        components.dedup();
        assert_eq!(components.len(), 4);
        for b in Build::ALL {
            let Some(a) = b.engine() else { continue };
            let cli_dir = crate::engine_cli_of(std::path::Path::new("R"), b.name()).parent().unwrap().to_path_buf();
            assert_eq!(std::path::Path::new("R").join(a.archive).parent().unwrap(), cli_dir, "{b:?}");
            assert_eq!(a.sha256.len(), 64);
            assert!(a.size > 0);
            assert!(a.url.ends_with(a.archive.rsplit('.').next().unwrap()), "{b:?}: the archive keeps its format");
        }
        assert_eq!(Build::Cuda13.runtime_component(), Some("cuda-runtime"));
        assert_eq!(Build::Cuda12.runtime_component(), Some("cuda12-runtime"));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn a_device_choice_names_its_build_as_speech_studio_does() {
        assert_eq!(Build::for_device("auto", Some(Build::Cuda12), false), Some(Build::Cuda12));
        assert_eq!(Build::for_device("auto", None, true), Some(Build::Vulkan));
        assert_eq!(Build::for_device("auto", None, false), Some(Build::Cpu));
        assert_eq!(Build::for_device("gpu", None, true), None, "the card is chosen and runs no CUDA build");
        assert_eq!(Build::for_device("vulkan", Some(Build::Cuda13), false), Some(Build::Vulkan));
        assert_eq!(Build::for_device("cpu", Some(Build::Cuda13), false), Some(Build::Cpu));
    }
}
