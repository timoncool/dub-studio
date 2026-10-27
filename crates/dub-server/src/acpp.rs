//! The studio's audiocpp_server (`acpp_server::SERVERS`, one process per backend): Higgs voicing and Parakeet and
//! CrisperWhisper recognition, with the models on disk now.

use crate::hw::CudaBuild;
use acpp_server::{AcppError, Backend, ModelEntry, Server, ServerConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub const HIGGS: &str = "higgs";
pub const PARAKEET: &str = "parakeet";
pub const CRISPERWHISPER: &str = "crisperwhisper";

/// The CrisperWhisper folder under the models root (the Hugging Face repository as it is; audio.cpp reads the folder).
pub const CRISPERWHISPER_DIR: &str = "crisperwhisper2-turbo";

/// How long a start may take before the server answers `/health` (it loads no model at start).
const READY_WITHIN: Duration = Duration::from_secs(60);

/// The build of the server for this card: CUDA 13, else CUDA 12.9 (Maxwell to Volta, drivers 525-579); a machine
/// whose card runs neither (or that has none) takes the CUDA 13 build for its processor backend.
pub fn build() -> CudaBuild {
    crate::hw::gpu_report().cuda_build.unwrap_or(CudaBuild::Cuda13)
}

/// The folder of a build under the program folder.
pub fn build_dir(build: CudaBuild) -> &'static str {
    match build {
        CudaBuild::Cuda13 => "tools/audiocpp",
        CudaBuild::Cuda12 => "tools/audiocpp-cuda12",
    }
}

/// The CUDA libraries the build's program needs at start and where the runtime component puts them (cudart is
/// linked into the CUDA 13 build); without them Windows refuses to start it (0xC0000135).
pub fn libraries(build: CudaBuild) -> &'static [&'static str] {
    match (build, cfg!(windows)) {
        (CudaBuild::Cuda13, true) => &["models/higgs-engine/cublas64_13.dll", "models/higgs-engine/cublasLt64_13.dll", "models/higgs-engine/cufft64_12.dll"],
        (CudaBuild::Cuda12, true) => &[
            "models/cuda12-runtime/cudart64_12.dll",
            "models/cuda12-runtime/cublas64_12.dll",
            "models/cuda12-runtime/cublasLt64_12.dll",
            "models/cuda12-runtime/cufft64_11.dll",
        ],
        (_, false) => &[],
    }
}

/// The server program: env DUB_STUDIO_AUDIOCPP_SERVER, else the program of this card's build.
pub fn exe(repo_root: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("DUB_STUDIO_AUDIOCPP_SERVER") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) { "audiocpp_server.exe" } else { "audiocpp_server" };
    // a Mac has its one Metal build (acpp_server::asset::SERVER_MACOS)
    let dir = if cfg!(target_os = "macos") { "tools/audiocpp-metal" } else { build_dir(build()) };
    repo_root.join(dir).join(name)
}

pub fn backend(stage_backend: &str) -> Backend {
    if cfg!(target_os = "macos") {
        // the Mac's one build runs Metal and the processor for every choice
        Backend::Metal
    } else if stage_backend == "cpu" {
        Backend::Cpu
    } else {
        Backend::Cuda
    }
}

/// Every model the server can carry now: the chosen Higgs quant when the voice is Higgs, Parakeet TDT v3 and
/// CrisperWhisper. A model whose files are not on disk stays out: the server reads each configured model's contract
/// when it starts, and one missing file would stop all of them. One list for analysis and render, so going from one
/// to the other does not start the server again.
pub fn models(models_root: &Path) -> Vec<ModelEntry> {
    let sel = crate::models::load_selection(models_root);
    let mut out = Vec::new();
    let (higgs_dir, quant) = crate::models::resolve_tts(models_root, &sel);
    let higgs = higgs_dir.join(format!("{quant}.gguf"));
    if higgs.is_file() {
        out.push(ModelEntry::higgs_tts(HIGGS, &higgs));
    }
    let parakeet = dub_asr::tcpp::model("parakeet-v3").map(|m| dub_asr::tcpp::model_path(models_root, m));
    if let Some(p) = parakeet.filter(|p| p.is_file()) {
        out.push(ModelEntry::parakeet_tdt(PARAKEET, &p));
    }
    let crisper = models_root.join(CRISPERWHISPER_DIR);
    if crisper.join("model.safetensors").is_file() {
        out.push(ModelEntry::crisperwhisper(CRISPERWHISPER, &crisper));
    }
    out
}

fn config(repo_root: &Path, models_root: &Path, backend: Backend) -> ServerConfig {
    let name = match backend {
        Backend::Cuda => "cuda",
        Backend::Vulkan => "vulkan",
        Backend::Cpu => "cpu",
        Backend::Metal => "metal",
    };
    ServerConfig {
        exe: exe(repo_root),
        backend,
        device: 0,
        threads: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(8).min(16),
        models: models(models_root),
        data_dir: repo_root.join("logs").join(format!("audiocpp_server-{name}")),
        log: false,
    }
}

/// The running server of this backend carrying `model`: the one there is, or a new one in its place when the models
/// on disk changed or the process ended. An error when `model` is not on disk or the program is missing.
pub fn server(repo_root: &Path, models_root: &Path, backend: Backend, model: &str) -> Result<Arc<Server>, String> {
    let config = config(repo_root, models_root, backend);
    if !config.exe.is_file() {
        return Err(t!("acpp-no-server", path = config.exe.display().to_string()));
    }
    let missing: Vec<String> = libraries(build()).iter().filter(|rel| !repo_root.join(rel).is_file()).map(|rel| rel.to_string()).collect();
    if !missing.is_empty() {
        return Err(t!("acpp-no-libraries", files = missing.join(", ")));
    }
    if !config.models.iter().any(|m| m.id == model) {
        return Err(t!("acpp-no-model", model = model.to_string()));
    }
    let server = acpp_server::SERVERS.get(&config, READY_WITHIN, &|_| {}).map_err(|e| localize(&e))?;
    tracing::info!("audiocpp_server {} ({backend:?}) carries {:?}", server.pid(), config.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>());
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
