//! Studio Hub in Dub Studio: anonymous statistics (only after the first-run screen showed its checkbox, off under
//! DO_NOT_TRACK or STUDIO_TELEMETRY=0) and the news feed shown on top of the bundled news.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::jobs::{JobKind, JobStatus};

static HUB: OnceLock<Option<studio_hub_client::Hub>> = OnceLock::new();
/// The models folder, whose active.json says what a job ran on.
static MODELS: OnceLock<PathBuf> = OnceLock::new();

/// Starts the hub once, inside the tokio runtime of the server; a state file that cannot be read leaves the studio
/// without it, said once.
pub fn start(repo_root: &Path) {
    let _ = MODELS.set(crate::models_root(repo_root));
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

/// A finished job, counted as `<kind>_done`, `<kind>_failed` or `<kind>_cancelled` (frame previews are not counted);
/// a failure keeps its reason, which the hub client scrubs, and finished work names the models it ran on.
pub fn job_ended(kind: JobKind, status: JobStatus, error: Option<&str>) {
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
        }
        JobStatus::Cancelled => {}
        _ => hub.failed(kind.as_str(), error.unwrap_or("")),
    }
}

/// What these stages run on now, each as `stage:model`: Dub has no named sets, its choice per stage is the set.
fn models_in_use(mroot: &Path, stages: &[&str]) -> Vec<String> {
    use crate::models::{self, LlmBackend};
    let sel = models::load_selection(mroot);
    let slot = |key: &str| sel.get(key).and_then(serde_json::Value::as_str).map(str::trim).filter(|value| !value.is_empty()).unwrap_or("default").to_string();
    stages
        .iter()
        .map(|&stage| match stage {
            "tts" => match models::tts_provider(mroot) {
                "local" => format!("tts:higgs-{}", slot("tts")),
                provider => format!("tts:{provider}:{}", models::tts_model(mroot)),
            },
            "asr" if models::openrouter_asr_on(mroot) => format!("asr:openrouter:{}", slot("or_asr")),
            "asr" if slot("asr_engine") == "whisper" => format!("asr:whisper-{}", slot("whisper_model")),
            "asr" => format!("asr:parakeet-{}", slot("asr")),
            "sep" => format!("sep:roformer-{}", slot("sep")),
            stage => match models::llm_backend(mroot, stage) {
                LlmBackend::Local => format!("{stage}:gemma-{}", slot("mt")),
                LlmBackend::Server => format!("{stage}:server"),
                LlmBackend::OpenRouter => format!("{stage}:openrouter:{}", models::openrouter_model(mroot, stage)),
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
        std::fs::write(dir.path().join("active.json"), r#"{"tts_provider":"local","tts":"q8_0","llm_provider":"local","mt":"e4b","sep":"fv6"}"#).unwrap();
        let retranslate = models_in_use(dir.path(), &["llm"]);
        assert_eq!(retranslate, ["llm:gemma-e4b"]);
        let render = models_in_use(dir.path(), &["tts", "sep"]);
        assert_eq!(render, ["tts:higgs-q8_0", "sep:roformer-fv6"]);
    }
}
