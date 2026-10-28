//! The pinned downloads of the separation engine BSRoformer.cpp v0.1.0, one per build, the same for every program.
//! Windows and Linux are the studios' builds (timoncool/BSRoformer.cpp studio-0.1.1): the program with ggml, a ggml-cpu
//! per instruction level and the device's backend module, so any x86-64 runs it; CUDA as `-real` SASS without PTX,
//! Ubuntu 22.04 on Linux, RUNPATH `$ORIGIN` and the programs' CUDA runtime folders. The Mac is the author's
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
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-windows-cuda13-x64.zip",
                archive: "tools/bsroformer/_engine.zip",
                size: 119_367_905,
                sha256: "c89128404d29c52ac0efed1a1072a76b277ccf13a33247611032746932e8d032",
            },
            Build::Cuda12 => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-windows-cuda12-x64.zip",
                archive: "tools/bsroformer-cuda12/_engine.zip",
                size: 173_177_321,
                sha256: "72211af3451d9d4266406da8039a3ecdd9eabde1373cd235ff408711af8f7400",
            },
            Build::Vulkan => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-windows-vulkan-x64.zip",
                archive: "tools/bsroformer-vulkan/_engine.zip",
                size: 25_661_617,
                sha256: "fc82682b4071d065e2e3e84998817877a4d936da7fbb3e580799904c54a6a4ab",
            },
            Build::Cpu => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-windows-cpu-x64.zip",
                archive: "tools/bsroformer-cpu/_engine.zip",
                size: 3_006_666,
                sha256: "0b212654ea598503992119328daec051bbce5f14b73808c5cc588f278817f908",
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
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-linux-cuda13-x64.tar.gz",
                archive: "tools/bsroformer/_engine.tar.gz",
                size: 123_464_218,
                sha256: "807f59113f17eb706bc7289155bdf507d05ae01650a5c2ccd9685f5574e9cd04",
            },
            Build::Cuda12 => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-linux-cuda12-x64.tar.gz",
                archive: "tools/bsroformer-cuda12/_engine.tar.gz",
                size: 177_134_761,
                sha256: "82540c02b5a9730137b0059ffda8c0c79b589a649af554dff1f88867a6c7444d",
            },
            Build::Vulkan => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-linux-vulkan-x64.tar.gz",
                archive: "tools/bsroformer-vulkan/_engine.tar.gz",
                size: 28_805_838,
                sha256: "d0b6acc7b4b05c453bbfe99ea8154251fde4d96b2edd957fdab6e596f7a64927",
            },
            Build::Cpu => Asset {
                url: "https://github.com/timoncool/BSRoformer.cpp/releases/download/studio-0.1.1/BSRoformer-linux-cpu-x64.tar.gz",
                archive: "tools/bsroformer-cpu/_engine.tar.gz",
                size: 5_849_988,
                sha256: "65eb3cf26dfed0a23f01906a9f179ae7ab503f2bdac4f411e49ec902a5addafd",
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
