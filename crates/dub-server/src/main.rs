//! Точка входа dub-server: поднимает axum на 127.0.0.1:8793 (порт env DUB_STUDIO_PORT).
//! Корень репо резолвится из env DUB_STUDIO_ROOT, иначе — рабочий каталог процесса.

use dub_server::service::{self, Claim};
use dub_server::{augment_path_for_tools, init_proxy_route, serve, AppState};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Рецепт «taskkill //IM dub-server.exe //F» не должен оставлять сирот (llama-server, ffmpeg, roformer).
    let bound = dub_server::process_group::bind_children_to_this_process();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    if !bound {
        tracing::error!("the process did not join its job object: sidecars are stopped only by their own destructors");
    }

    let repo_root = std::env::var("DUB_STUDIO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().expect("cwd"));
    dub_server::ensure_library_path(&repo_root);

    let port = service::listen_port().map_err(anyhow::Error::msg)?;
    let listener = match tokio::task::spawn_blocking(move || service::claim_port(port, Duration::from_secs(20))).await? {
        Ok(Claim::Bound(l)) => l,
        Ok(Claim::AlreadyRunning(r)) => anyhow::bail!(
            "Dub Studio {} is already running on 127.0.0.1:{port} ({}, repo_root={}); not starting a second service",
            r.version,
            r.service_executable,
            r.repo_root
        ),
        Err(busy) => return Err(busy.into()),
    };

    // Прописать в PATH каталоги скачанных бинарей (ffmpeg/llama/higgs-engine) до старта — чтобы после
    // автозакачки они находились без рестарта процесса.
    augment_path_for_tools(&repo_root);
    init_proxy_route(&repo_root);
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    dub_server::set_ort_dylib_env(&repo_root, &exe_dir);

    let state = AppState::new(&repo_root);
    tracing::info!(
        "dub-server {}: repo_root={}, workspace={}, web_root={:?}",
        service::app_version(),
        state.repo_root.display(),
        state.workspace.display(),
        state.web_root
    );

    tracing::info!("listening on http://{}", listener.local_addr()?);
    serve(state, listener).await
}
