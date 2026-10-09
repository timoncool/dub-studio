//! Studio Hub in Dub Studio: anonymous statistics (only after the first-run screen showed its checkbox, off under
//! DO_NOT_TRACK or STUDIO_TELEMETRY=0) and the news feed shown on top of the bundled news.

use std::path::Path;
use std::sync::OnceLock;

use crate::jobs::{JobKind, JobStatus};

static HUB: OnceLock<Option<studio_hub_client::Hub>> = OnceLock::new();

/// Starts the hub once, inside the tokio runtime of the server; a state file that cannot be read leaves the studio
/// without it, said once.
pub fn start(repo_root: &Path) {
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

/// The hub's routes for the window (`/v1/hub/*`); none when the hub did not start.
pub fn router<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
    HUB.get().and_then(Option::as_ref).map(studio_hub_client::Hub::router).unwrap_or_default()
}

/// A finished job, counted as `<kind>_done`, `<kind>_failed` or `<kind>_cancelled` (frame previews are not counted).
pub fn job_ended(kind: JobKind, status: JobStatus) {
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
}
