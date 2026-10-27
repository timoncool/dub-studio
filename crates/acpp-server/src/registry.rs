//! The servers of a studio: one per backend (a stage on the card and one on the processor run in two processes), each
//! started at the first need and started again when the studio's set of models changes or the process has ended.

use crate::{AcppError, Backend, Server, ServerConfig};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Servers {
    running: Mutex<Vec<Arc<Server>>>,
}

/// The studio's servers.
pub static SERVERS: Servers = Servers::new();

impl Servers {
    pub const fn new() -> Servers {
        Servers { running: Mutex::new(Vec::new()) }
    }

    /// The running server of `config.backend` with exactly this config: the one there is, or a new one in its place.
    pub fn get(&self, config: &ServerConfig, ready_within: Duration, prepare: &dyn Fn(&mut Command)) -> Result<Arc<Server>, AcppError> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = running.iter().find(|s| s.config().backend == config.backend) {
            if s.config() == config && s.alive() {
                return Ok(s.clone());
            }
        }
        if let Some(at) = running.iter().position(|s| s.config().backend == config.backend) {
            running.remove(at).kill();
        }
        let started = Arc::new(Server::start(config, ready_within, prepare)?);
        running.push(started.clone());
        Ok(started)
    }

    /// The running server of a backend, if there is one.
    pub fn running(&self, backend: Backend) -> Option<Arc<Server>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|s| s.config().backend == backend).cloned()
    }

    /// Ends every server; the next need starts a new one.
    pub fn stop_all(&self) {
        for s in self.running.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
            s.kill();
        }
    }

    /// Frees the memory of these models on every running server that carries them; their next request loads them
    /// again. Each server's refusal is returned with its pid.
    pub fn unload(&self, ids: &[&str]) -> Vec<(u32, AcppError)> {
        let current: Vec<Arc<Server>> = self.running.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut failed = Vec::new();
        for s in current {
            let present: Vec<&str> = ids.iter().copied().filter(|id| s.config().models.iter().any(|m| m.id == *id)).collect();
            if present.is_empty() {
                continue;
            }
            if let Err(e) = s.unload(&present) {
                failed.push((s.pid(), e));
            }
        }
        failed
    }
}
