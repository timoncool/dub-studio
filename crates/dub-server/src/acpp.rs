//! The studio's audiocpp_server (audio.cpp, through the crate `acpp-server`: `SERVERS`, one process per build): Higgs
//! voicing, CrisperWhisper, Parakeet TDT v3 and Nemotron 3.5 recognition, Nemotron 3 Diarization, with the models on
//! disk now. The same server,
//! builds and layout as Speech Studio's, so a model of one program serves the other.

pub use acpp_server::asset::Build;
use acpp_server::{AcppError, ModelEntry, Server, ServerConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub const HIGGS: &str = "higgs";
pub const CRISPERWHISPER: &str = "crisperwhisper";
pub const PARAKEET: &str = "parakeet-v3";
pub const NEMOTRON: &str = "nemotron";
pub const DIAR: &str = "nemotron-diar";

/// The models of transcribe.cpp's catalogue the server runs, reading their GGUF in place: Parakeet TDT v3 and Nemotron
/// 3.5. Parakeet Ultra stays on transcribe.cpp: audio.cpp does not read its built-in VAD head.
pub const SERVER_ASR: [&str; 2] = [PARAKEET, NEMOTRON];

/// The CrisperWhisper folder under the models root (the Hugging Face repository as it is; audio.cpp reads the folder).
pub const CRISPERWHISPER_DIR: &str = "crisperwhisper2-turbo";

/// How long a start may take before the server answers `/health` (it loads no model at start).
const READY_WITHIN: Duration = Duration::from_secs(60);

/// The build of the server a device choice runs: `cpu`; `vulkan`; `gpu` (the card's CUDA build: 13, else 12.9); or
/// «auto»: the card's CUDA build, else Vulkan on a card of another make, else the processor. An NVIDIA card runs Vulkan
/// only when it is chosen. An error when the card is chosen and it runs neither CUDA build. A Mac with Apple silicon
/// has its one Metal build for every choice.
pub fn build_for(choice: &str) -> Result<Build, String> {
    if let Some(mac) = Build::for_this_mac() {
        return Ok(mac);
    }
    let cuda = crate::hw::gpu_report().cuda_build.map(|b| match b {
        crate::hw::CudaBuild::Cuda13 => Build::Cuda13,
        crate::hw::CudaBuild::Cuda12 => Build::Cuda12,
    });
    match choice {
        "cpu" => Ok(Build::Cpu),
        "vulkan" => Ok(Build::Vulkan),
        "gpu" | "cuda" => cuda.ok_or_else(|| t!("acpp-no-cuda-build")),
        _ => Ok(cuda.unwrap_or_else(|| {
            let other_make = crate::hw::display_adapter().is_some_and(|name| !name.to_ascii_lowercase().contains("nvidia"));
            if other_make { Build::Vulkan } else { Build::Cpu }
        })),
    }
}

/// The build «auto» runs on this computer.
pub fn build() -> Build {
    build_for("auto").expect("auto always names a build")
}

/// The CUDA build of this card, as the engines with CUDA builds alone take it (transcribe.cpp, BSRoformer).
pub fn cuda_build() -> crate::hw::CudaBuild {
    crate::hw::gpu_report().cuda_build.unwrap_or(crate::hw::CudaBuild::Cuda13)
}

/// The server program of a build: env DUB_STUDIO_AUDIOCPP_SERVER, else `tools/<build folder>` of the program.
pub fn exe(repo_root: &Path, build: Build) -> PathBuf {
    if let Some(p) = std::env::var_os("DUB_STUDIO_AUDIOCPP_SERVER") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) { "audiocpp_server.exe" } else { "audiocpp_server" };
    repo_root.join("tools").join(build.folder()).join(name)
}

/// The component that downloads a build: `higgs-engine` is the build «auto» runs here, the others are fetched when a
/// stage is set to them.
pub fn component(build: Build) -> &'static str {
    if build == crate::acpp::build() {
        return "higgs-engine";
    }
    match build {
        Build::Cuda13 => "audiocpp-cuda13",
        Build::Cuda12 => "audiocpp-cuda12",
        Build::Vulkan => "audiocpp-vulkan",
        Build::Cpu => "audiocpp-cpu",
        Build::Metal => "audiocpp-metal",
    }
}

/// The CUDA runtime component a build needs, if any.
pub fn runtime_component(build: Build) -> Option<&'static str> {
    match build {
        Build::Cuda13 => Some("cuda-runtime"),
        Build::Cuda12 => Some("cuda12-runtime"),
        Build::Vulkan | Build::Cpu | Build::Metal => None,
    }
}

/// The CUDA libraries a CUDA build imports that are in none of the places they come to: beside the program, the CUDA
/// 13 runtime component (`models/higgs-engine`), the CUDA 12.9 one (`models/cuda12-runtime`).
pub fn missing_libraries(repo_root: &Path, build: Build) -> Vec<&'static str> {
    let beside = repo_root.join("tools").join(build.folder());
    let cuda13 = repo_root.join("models").join("higgs-engine");
    let cuda12 = repo_root.join("models").join("cuda12-runtime");
    acpp_server::asset::missing_libraries(build, &[&beside, &cuda13, &cuda12])
}

/// Nemotron 3 Diarization, which the server reads as it is: env DUB_STUDIO_SORTFORMER, else
/// `<models>/nemotron-diar/nemotron3_diar_v3.onnx`.
pub fn diar_onnx(models_root: &Path) -> PathBuf {
    std::env::var_os("DUB_STUDIO_SORTFORMER")
        .map(PathBuf::from)
        .unwrap_or_else(|| models_root.join(dub_asr::DIAR_MODEL_DIR).join(dub_asr::DIAR_MODEL_FILE))
}

/// Every model the server can carry now: the chosen Higgs quant, CrisperWhisper, and the transcribe.cpp GGUFs of
/// Parakeet TDT v3 and Nemotron 3.5. A model whose files are not on disk stays out: the server reads each configured
/// model's contract when it starts, and one missing file would stop all of them. One list for analysis and render, so
/// going from one to the other does not start the server again.
pub fn models(models_root: &Path) -> Vec<ModelEntry> {
    let sel = crate::models::load_selection(models_root);
    let mut out = Vec::new();
    let (higgs_dir, quant) = crate::models::resolve_tts(models_root, &sel);
    let higgs = higgs_dir.join(format!("{quant}.gguf"));
    if higgs.is_file() {
        out.push(ModelEntry::higgs_tts(HIGGS, &higgs));
    }
    let crisper = models_root.join(CRISPERWHISPER_DIR);
    if crisper.join("model.safetensors").is_file() {
        out.push(ModelEntry::crisperwhisper(CRISPERWHISPER, &crisper));
    }
    let diar = diar_onnx(models_root);
    if diar.is_file() {
        out.push(ModelEntry::nemotron_3_diar(DIAR, &diar));
    }
    for id in SERVER_ASR {
        let Some(model) = dub_asr::tcpp::model(id) else { continue };
        let gguf = dub_asr::tcpp::model_path(models_root, model);
        if gguf.is_file() {
            out.push(if id == NEMOTRON { ModelEntry::nemotron_asr(id, &gguf) } else { ModelEntry::parakeet_tdt(id, &gguf) });
        }
    }
    out
}

fn config(repo_root: &Path, models_root: &Path, build: Build) -> ServerConfig {
    ServerConfig {
        exe: exe(repo_root, build),
        backend: build.backend(),
        device: 0,
        threads: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(8).min(16),
        models: models(models_root),
        data_dir: repo_root.join("logs").join(build.folder()),
        log: false,
    }
}

/// The running server of this build carrying `model`: the one there is, or a new one in its place when the models
/// on disk changed or the process ended. An error, before the program starts, when the processor lacks the build's
/// floor, the program or its CUDA libraries are missing, or `model` is not on disk.
pub fn server(repo_root: &Path, models_root: &Path, build: Build, model: &str) -> Result<Arc<Server>, String> {
    dub_asr::tcpp::cpu_floor("audio.cpp")?;
    let config = config(repo_root, models_root, build);
    if !config.exe.is_file() {
        return Err(t!("acpp-no-server", path = config.exe.display().to_string()));
    }
    if std::env::var_os("DUB_STUDIO_AUDIOCPP_SERVER").is_none() {
        let missing = missing_libraries(repo_root, build);
        if !missing.is_empty() {
            return Err(t!("acpp-no-libraries", files = missing.join(", ")));
        }
    }
    if !config.models.iter().any(|m| m.id == model) {
        return Err(t!("acpp-no-model", model = model.to_string()));
    }
    let server = acpp_server::SERVERS.get(&config, READY_WITHIN, &|_| {}).map_err(|e| localize(&e))?;
    tracing::info!("audiocpp_server {} ({build:?}) carries {:?}", server.pid(), config.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>());
    Ok(server)
}

/// Stops every server (one stalled); the next need starts a new one.
pub fn stop() {
    acpp_server::SERVERS.stop_all();
}

/// Frees the memory of these models on every running server; their next request loads them again.
pub fn unload(ids: &[&str]) {
    for (pid, e) in acpp_server::SERVERS.unload(ids) {
        tracing::error!("[ERROR] audiocpp_server {pid} did not unload {ids:?}: {e}");
    }
}

/// The server's error in the window's language.
pub fn localize(e: &AcppError) -> String {
    t!("acpp-error", error = e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_choice_names_its_build() {
        assert_eq!(build_for("cpu").unwrap(), Build::for_this_mac().unwrap_or(Build::Cpu));
        assert_eq!(build_for("vulkan").unwrap(), Build::for_this_mac().unwrap_or(Build::Vulkan));
        let auto = build();
        assert!(matches!(auto, Build::Cuda13 | Build::Cuda12 | Build::Vulkan | Build::Cpu | Build::Metal));
        assert_eq!(component(auto), "higgs-engine", "the build «auto» runs is the server component");
        let folders = [Build::Cuda13, Build::Cuda12, Build::Vulkan, Build::Cpu].map(|b| exe(Path::new("R"), b).parent().unwrap().to_path_buf());
        assert_eq!(folders.map(|f| f.file_name().unwrap().to_string_lossy().into_owned()), ["audiocpp", "audiocpp-cuda12", "audiocpp-vulkan", "audiocpp-cpu"]);
    }

    #[test]
    fn the_installed_diarization_model_is_on_the_server() {
        let root = tempfile::tempdir().unwrap();
        assert!(!models(root.path()).iter().any(|m| m.id == DIAR));
        let onnx = root.path().join(dub_asr::DIAR_MODEL_DIR).join(dub_asr::DIAR_MODEL_FILE);
        std::fs::create_dir_all(onnx.parent().unwrap()).unwrap();
        std::fs::write(&onnx, b"onnx").unwrap();
        if std::env::var_os("DUB_STUDIO_SORTFORMER").is_none() {
            assert_eq!(diar_onnx(root.path()), onnx);
            assert!(models(root.path()).iter().any(|m| m.id == DIAR));
        }
    }
}
