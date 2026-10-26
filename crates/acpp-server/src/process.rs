//! The server process: one per studio, started with its config on a free port, stopped with the studio.

use crate::{AcppError, ServerConfig};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A running `audiocpp_server`. Dropping it stops the process.
pub struct Server {
    child: Mutex<Child>,
    pub(crate) config: ServerConfig,
    pub(crate) base: String,
    pub(crate) http: reqwest::blocking::Client,
}

/// The last part of the server's log for an error message.
pub(crate) fn log_tail(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            lines[lines.len().saturating_sub(12)..].join(" | ")
        }
        Err(e) => format!("no log at {} ({e})", path.display()),
    }
}

fn platform_defaults(cmd: &mut Command, exe_dir: Option<&Path>) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = exe_dir;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        if let Some(dir) = exe_dir {
            let mut paths = vec![dir.to_path_buf()];
            if let Some(cur) = std::env::var_os("LD_LIBRARY_PATH") {
                paths.extend(std::env::split_paths(&cur));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                cmd.env("LD_LIBRARY_PATH", joined);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        // The flag lives in the thread and survives exec only if set in pre_exec; only async-signal-safe calls here.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("the studio exited while the server was starting"));
                }
                Ok(())
            });
        }
    }
}

fn free_port() -> Result<u16, AcppError> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| AcppError::Io(format!("a free port: {e}")))?;
    l.local_addr().map(|a| a.port()).map_err(|e| AcppError::Io(format!("a free port: {e}")))
}

impl Server {
    /// Writes the config, starts the server and waits until `/health` answers. The process has no console window,
    /// finds the libraries beside the program and dies with the studio (Windows: the studio's job object, which its
    /// children join; Linux: the parent-death signal). `prepare` adds what else the studio puts on its processes; the
    /// server's output goes to [`ServerConfig::output_path`].
    pub fn start(config: &ServerConfig, ready_within: Duration, prepare: &dyn Fn(&mut Command)) -> Result<Server, AcppError> {
        dub_tcpp::cpu_floor("audio.cpp").map_err(|error| AcppError::Spawn { exe: config.exe.display().to_string(), error })?;
        let io = |p: &Path, e: std::io::Error| AcppError::Io(format!("{}: {e}", p.display()));
        std::fs::create_dir_all(&config.data_dir).map_err(|e| io(&config.data_dir, e))?;
        let port = free_port()?;
        let cfg_path = config.config_path();
        let body = serde_json::to_vec_pretty(&config.json(port)).map_err(|e| AcppError::Io(e.to_string()))?;
        std::fs::write(&cfg_path, body).map_err(|e| io(&cfg_path, e))?;
        let log = config.log_path();
        let _ = std::fs::remove_file(&log);
        let mut cmd = Command::new(&config.exe);
        cmd.arg("--config").arg(&cfg_path).arg("--no-ui");
        if config.log {
            cmd.arg("--log").arg("--log-file").arg(&log);
        }
        if let Some(dir) = config.exe.parent() {
            cmd.current_dir(dir);
        }
        let out_path = config.output_path();
        let out = std::fs::File::create(&out_path).map_err(|e| io(&out_path, e))?;
        let err = out.try_clone().map_err(|e| io(&out_path, e))?;
        cmd.stdin(Stdio::null()).stdout(out).stderr(err);
        platform_defaults(&mut cmd, config.exe.parent());
        prepare(&mut cmd);
        let child = cmd.spawn().map_err(|e| AcppError::Spawn { exe: config.exe.display().to_string(), error: e.to_string() })?;
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(None)
            .build()
            .map_err(|e| AcppError::Io(format!("HTTP client: {e}")))?;
        let server = Server { child: Mutex::new(child), config: config.clone(), base: format!("http://127.0.0.1:{port}"), http };
        server.wait_ready(ready_within)?;
        Ok(server)
    }

    fn wait_ready(&self, within: Duration) -> Result<(), AcppError> {
        let deadline = Instant::now() + within;
        loop {
            self.check_alive()?;
            let probe = self.http.get(format!("{}/health", self.base)).timeout(Duration::from_secs(2)).send();
            if probe.is_ok_and(|r| r.status().is_success()) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(AcppError::NotReady { secs: within.as_secs(), log: log_tail(&self.config.tail_path()) });
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// An error with the log when the process has ended.
    pub fn check_alive(&self) -> Result<(), AcppError> {
        match self.child.lock().unwrap_or_else(|e| e.into_inner()).try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(AcppError::Exited { code: status.code(), log: log_tail(&self.config.tail_path()) }),
            Err(e) => Err(AcppError::Io(format!("audiocpp_server: {e}"))),
        }
    }

    pub fn alive(&self) -> bool {
        self.check_alive().is_ok()
    }

    pub fn pid(&self) -> u32 {
        self.child.lock().unwrap_or_else(|e| e.into_inner()).id()
    }

    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Frees the memory of these models; their next request loads them again.
    pub fn unload(&self, ids: &[&str]) -> Result<(), AcppError> {
        let resp = self
            .http
            .post(format!("{}/v1/tasks/unload_models", self.base))
            .timeout(Duration::from_secs(120))
            .json(&json!({ "model_ids": ids }))
            .send()
            .map_err(|e| AcppError::Io(format!("audiocpp_server unload: {e}")))?;
        let v = crate::speech::json_or_error(resp)?;
        match v.get("not_found").and_then(Value::as_array) {
            Some(missing) if !missing.is_empty() => Err(AcppError::Protocol(format!("unload: unknown models {missing:?}"))),
            _ => Ok(()),
        }
    }

    /// Ends the process; a request running on it fails, and every later one too.
    pub fn kill(&self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.kill();
    }
}
