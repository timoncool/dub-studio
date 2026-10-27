//! Studio Hub in Dub Studio: anonymous statistics (only after the first-run screen showed its checkbox, off under
//! DO_NOT_TRACK or STUDIO_TELEMETRY=0) and the news feed shown on top of the bundled news.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::jobs::{JobKind, JobStatus};

static HUB: OnceLock<Option<studio_hub_client::Hub>> = OnceLock::new();
/// The models folder, whose active.json says what a job ran on.
static MODELS: OnceLock<PathBuf> = OnceLock::new();
/// The projects folder, whose project.json says how long a finished job's video is.
static WORKSPACE: OnceLock<PathBuf> = OnceLock::new();

/// Starts the hub once, inside the tokio runtime of the server; a state file that cannot be read leaves the studio
/// without it, said once.
pub fn start(repo_root: &Path, workspace: &Path) {
    let _ = MODELS.set(crate::models_root(repo_root));
    let _ = WORKSPACE.set(workspace.to_path_buf());
    HUB.get_or_init(|| {
        let gpu = crate::hw::gpu_report();
        let name = gpu.name.clone().unwrap_or_default().to_ascii_lowercase();
        let vendor = if gpu.nvidia {
            "nvidia"
        } else if name.contains("amd") || name.contains("radeon") {
            "amd"
        } else if name.contains("intel") {
            "intel"
        } else if name.is_empty() {
            "none"
        } else {
            "other"
        };
        let http = match dub_llm::net::async_builder().build() {
            Ok(http) => http,
            Err(e) => {
                eprintln!("[ERROR] Studio Hub is off for this run, its HTTP client was not built: {e}");
                return None;
            }
        };
        let os_label = format!(
            "{} {}",
            sysinfo::System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
            sysinfo::System::os_version().unwrap_or_default()
        )
        .to_ascii_lowercase();
        let config = studio_hub_client::HubConfig {
            app: "dub".into(),
            version: crate::service::app_version().to_string(),
            data_dir: repo_root.to_path_buf(),
            http,
            os_label,
            gpu: studio_hub_client::Gpu {
                vendor: vendor.into(),
                vram_gb: studio_hub_client::Gpu::vram_bucket(crate::hw::snapshot().total_vram),
                backend: if gpu.cuda13_ok { "cuda" } else { "cpu" }.into(),
            },
            ui_lang: "en".into(),
            urls: Vec::new(),
        };
        match studio_hub_client::Hub::new(config) {
            Ok(hub) => {
                hub.spawn();
                Some(hub)
            }
            Err(e) => {
                eprintln!("[ERROR] Studio Hub is off for this run, its state file cannot be read: {e}");
                None
            }
        }
    });
}

/// The day's statistics, sent as the studio closes instead of at its next start; at most a few seconds.
pub fn flush_on_exit() {
    let Some(hub) = HUB.get().and_then(Option::as_ref).cloned() else { return };
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(hub.flush(std::time::Duration::from_secs(8))),
        Err(e) => eprintln!("[ERROR] the statistics were not sent at exit, no runtime: {e}"),
    }
}

/// The hub's routes for the window (`/v1/hub/*`); none when the hub did not start.
pub fn router<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
    HUB.get().and_then(Option::as_ref).map(studio_hub_client::Hub::router).unwrap_or_default()
}

/// Every call to the studio's API counted by route, the features people use; the router as it is without the hub.
pub fn track<S: Clone + Send + Sync + 'static>(router: axum::Router<S>) -> axum::Router<S> {
    match HUB.get().and_then(Option::as_ref) {
        Some(hub) => hub.track(router),
        None => router,
    }
}

/// The other programs of timoncool's on this computer and their data folders, to take models from.
pub fn neighbors() -> Vec<(String, PathBuf)> {
    HUB.get().and_then(Option::as_ref).map(|hub| hub.neighbors().into_iter().map(|n| (n.app, n.data_dir)).collect()).unwrap_or_default()
}

/// A model download finished: the step between installing the studio and the first dub.
pub fn setup_ready() {
    if let Some(hub) = HUB.get().and_then(Option::as_ref) {
        hub.setup_ready();
    }
}

/// A model download failed, with its error code (network, disk_space, hash_mismatch...): the detail is in the
/// window's language and would split one cause into rows per language.
pub fn setup_failed(reason: &str) {
    if let Some(hub) = HUB.get().and_then(Option::as_ref) {
        hub.setup_failed(reason);
    }
}

/// A finished job, counted as `<kind>_done`, `<kind>_failed` or `<kind>_cancelled` (frame previews are not counted);
/// a failure keeps its reason, which the hub client scrubs, and finished work names the models it ran on. Jobs run one
/// at a time, so an analysis's `worked_ms` (running to finished) against the video's length is the card's speed.
pub fn job_ended(kind: JobKind, status: JobStatus, error: Option<&str>, pid: Option<&str>, worked_ms: Option<u64>) {
    let Some(hub) = HUB.get().and_then(Option::as_ref) else { return };
    if kind == JobKind::Frame {
        return;
    }
    let outcome = match status {
        JobStatus::Done => "done",
        JobStatus::Cancelled => "cancelled",
        _ => "failed",
    };
    hub.count(&format!("{}_{outcome}", kind.as_str()), 1);
    match status {
        JobStatus::Done => {
            let stages: &[&str] = match kind {
                JobKind::Analyze => &["asr", "llm", "vision", "sep"],
                JobKind::Retranslate => &["llm"],
                JobKind::DubAudio => &["tts"],
                JobKind::Render => &["tts", "sep"],
                _ => &[],
            };
            match MODELS.get() {
                Some(root) if !stages.is_empty() => hub.used_models(None, &models_in_use(root, stages), 1),
                _ => {}
            }
            let seconds = pid.and_then(video_seconds);
            // rendering and voicing reuse the lines already voiced, so only analysis always works the whole video
            if let (Some(seconds), Some(worked_ms), JobKind::Analyze) = (seconds, worked_ms, kind) {
                hub.worked(kind.as_str(), seconds, std::time::Duration::from_millis(worked_ms));
            }
        }
        JobStatus::Cancelled => {}
        _ => hub.failed(kind.as_str(), error.unwrap_or("")),
    }
}

/// How long a project's video is, from its project.json.
fn video_seconds(pid: &str) -> Option<f64> {
    let text = std::fs::read_to_string(WORKSPACE.get()?.join(pid).join("project.json")).ok()?;
    dub_core::Project::from_json(&text).ok().map(|project| project.meta.duration)
}

/// What these stages run on now, each as `stage:model`: Dub has no named sets, its choice per stage is the set. The
/// model is the one the job loads (the same resolution), not the raw choice, which is empty until the person picks.
fn models_in_use(mroot: &Path, stages: &[&str]) -> Vec<String> {
    use crate::models::{self, LlmBackend};
    let sel = models::load_selection(mroot);
    let slot = |key: &str, default: &str| sel.get(key).and_then(serde_json::Value::as_str).map(str::trim).filter(|value| !value.is_empty()).unwrap_or(default).to_string();
    let stem = |path: &Path| path.file_stem().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    stages
        .iter()
        .map(|&stage| match stage {
            "tts" => match models::tts_provider(mroot) {
                "local" => match models::resolve_tts(mroot, &sel).1 {
                    quant if quant == models::DRAMA_TTS => format!("tts:{quant}"),
                    quant => format!("tts:higgs-{quant}"),
                },
                provider => format!("tts:{provider}:{}", models::tts_model(mroot)),
            },
            "asr" if models::openrouter_asr_on(mroot) => format!("asr:openrouter:{}", slot("or_asr", "default")),
            "asr" if slot("asr_engine", "parakeet") == "whisper" => "asr:crisperwhisper-turbo".to_string(),
            "asr" => format!("asr:parakeet-{}", slot("asr", "int8")),
            "sep" => format!("sep:roformer-{}", stem(&models::resolve_sep(mroot, &sel)).trim_start_matches("voc_fv6-")),
            stage => match models::llm_backend(mroot, stage) {
                LlmBackend::Local => format!("{stage}:{}", stem(&models::resolve_mt(mroot, &sel).0)),
                LlmBackend::Server => format!("{stage}:server"),
                LlmBackend::OpenRouter => format!("{stage}:openrouter:{}", models::openrouter_model(mroot, stage)),
                LlmBackend::Agent => format!("{stage}:agent"),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::models_in_use;

    #[test]
    fn a_job_names_only_the_stages_it_ran() {
        let dir = tempfile::tempdir().unwrap();
        let file = |rel: &str| {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        };
        file("mt-q6_k/gemma-4-12b-it-Q6_K.gguf");
        file("mt-q6_k/mmproj-gemma-4-12b-it.gguf");
        file("bsroformer/voc_fv6-Q5_0.gguf");
        file("higgs-q6_k/q6_k.gguf");
        std::fs::write(dir.path().join("active.json"), r#"{"tts_provider":"local","tts":"q6_k","llm_provider":"local","mt":"q6_k","sep":"Q5_0"}"#).unwrap();
        let retranslate = models_in_use(dir.path(), &["llm"]);
        assert_eq!(retranslate, ["llm:gemma-4-12b-it-Q6_K"]);
        let render = models_in_use(dir.path(), &["tts", "sep"]);
        assert_eq!(render, ["tts:higgs-q6_k", "sep:roformer-Q5_0"]);
        std::fs::write(dir.path().join("active.json"), r#"{"tts_provider":"local"}"#).unwrap();
        assert_eq!(models_in_use(dir.path(), &["tts", "sep", "asr"]), ["tts:higgs-q6_k", "sep:roformer-Q5_0", "asr:parakeet-int8"], "an empty choice names what loads");
        std::fs::write(dir.path().join("active.json"), r#"{"tts_provider":"local","tts":"drama3"}"#).unwrap();
        assert_eq!(models_in_use(dir.path(), &["tts"]), ["tts:drama3"]);
    }
}
