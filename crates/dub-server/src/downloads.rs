//! Фоновая закачка компонентов («Первый запуск» и менеджер моделей): своим потоком, мимо GPU-очереди джоб,
//! так что 12 ГБ кванта не держат превью-кадры, анализ и рендер, а закачка не стоит за идущим рендером.
//! Состояние видно в GET /setup/status (`active`) без SSE, сохраняется в models/.setup-download.json и
//! переживает перезапуск: оборванная закачка показывается «прервано» и докачивается с места (Range + манифест
//! чанков), пауза — то же самое по кнопке.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::setup::{self, DlError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Downloading,
    Completed,
    /// Остановлена кнопкой: скачанное лежит в .part и докачается следующим запуском.
    Paused,
    /// Оборвана закрытием или падением приложения; докачивается так же, как после паузы.
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartProgress {
    pub id: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    pub id: String,
    /// Компоненты этой закачки (их же шлёт кнопка «Продолжить»).
    pub ids: Vec<String>,
    pub status: DownloadStatus,
    /// Что делается сейчас: waiting (эти компоненты качает кто-то ещё) | verify (SHA-256) | download | extract.
    pub phase: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: u64,
    /// Сколько секунд сервер просит подождать (429/5xx или исчерпанное окно RateLimit); 0 — не ждём.
    pub waiting_s: u64,
    pub parts: Vec<PartProgress>,
    /// Код ошибки (disk_space, hash_mismatch, size_mismatch, rate_limited, network, http_status, io, extract,
    /// proxy, interrupted) и подробность для журнала.
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub started_at: u64,
    pub updated_at: u64,
    /// The update screen's download: once every component is in place, the leftovers of former releases go.
    #[serde(default)]
    pub retire_after: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistentState {
    active: Option<DownloadJob>,
}

struct Inner {
    repo_root: PathBuf,
    state_path: PathBuf,
    state: Mutex<PersistentState>,
    /// Флаг паузы текущей закачки. У каждой закачки свой: поток прошлой, ещё дочитывающий кусок после паузы,
    /// не оживает от старта следующей.
    pause: Mutex<Arc<AtomicBool>>,
    last_persist: Mutex<Instant>,
}

#[derive(Clone)]
pub struct Downloads {
    inner: Arc<Inner>,
}

fn now_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Закачка, оставшаяся «идущей» в файле состояния, оборвалась вместе с прошлым процессом.
fn recover_interrupted(state: &mut PersistentState) -> bool {
    let Some(job) = state.active.as_mut() else { return false };
    if job.status != DownloadStatus::Downloading {
        return false;
    }
    job.status = DownloadStatus::Interrupted;
    job.error_code = Some("interrupted".into());
    job.error = Some(t!("downloads-interrupted"));
    job.speed_bps = 0;
    job.waiting_s = 0;
    true
}

/// A download of `ids` continues a stopped update-screen download of the same components.
fn continues_update(previous: Option<&DownloadJob>, ids: &[String]) -> bool {
    let sorted = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v
    };
    previous.is_some_and(|j| j.retire_after && j.status != DownloadStatus::Completed && j.status != DownloadStatus::Downloading && sorted(&j.ids) == sorted(ids))
}

fn persist(path: &Path, state: &PersistentState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

impl Downloads {
    pub fn open(repo_root: &Path) -> Self {
        let state_path = repo_root.join("models").join(".setup-download.json");
        let mut state: PersistentState = match std::fs::read_to_string(&state_path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("{} is unreadable ({e}); the previous download is forgotten", state_path.display());
                PersistentState::default()
            }),
            Err(_) => PersistentState::default(),
        };
        if recover_interrupted(&mut state) {
            if let Err(e) = persist(&state_path, &state) {
                tracing::warn!("{}: {e}", state_path.display());
            }
        }
        Downloads {
            inner: Arc::new(Inner {
                repo_root: repo_root.to_path_buf(),
                state_path,
                state: Mutex::new(state),
                pause: Mutex::new(Arc::new(AtomicBool::new(false))),
                last_persist: Mutex::new(Instant::now()),
            }),
        }
    }

    pub fn active(&self) -> Option<DownloadJob> {
        lock(&self.inner.state).active.clone()
    }

    fn save(&self, state: &PersistentState) {
        if let Err(e) = persist(&self.inner.state_path, state) {
            tracing::warn!("{}: {e}", self.inner.state_path.display());
        }
        *lock(&self.inner.last_persist) = Instant::now();
    }

    /// Запустить закачку компонентов своим потоком. Вторая, пока идёт первая, — отказ «busy»; нехватка места
    /// на томе моделей — отказ «disk_space» до первого байта.
    pub fn start(&self, ids: Vec<String>) -> Result<DownloadJob, DlError> {
        self.start_job(ids, false)
    }

    /// The update screen's download: the same, and once it completes the leftovers of former releases are removed.
    pub fn start_update(&self, ids: Vec<String>) -> Result<DownloadJob, DlError> {
        self.start_job(ids, true)
    }

    /// A stopped update-screen download continued by its own components (the progress panel's «Continue») stays one.
    fn start_job(&self, ids: Vec<String>, retire_after: bool) -> Result<DownloadJob, DlError> {
        let known: Vec<String> = setup::manifest()
            .iter()
            .filter(|c| c.delivery == setup::Delivery::Download && ids.iter().any(|x| x == c.id))
            .map(|c| c.id.to_string())
            .collect();
        if known.is_empty() {
            return Err(DlError::new("nothing_to_download", t!("downloads-nothing-to-download")));
        }
        let mut state = lock(&self.inner.state);
        if state.active.as_ref().is_some_and(|j| j.status == DownloadStatus::Downloading) {
            return Err(DlError::new("busy", t!("downloads-busy")));
        }
        setup::check_space(&self.inner.repo_root, &known)?;
        let retire_after = retire_after || continues_update(state.active.as_ref(), &known);
        let pause = Arc::new(AtomicBool::new(false));
        *lock(&self.inner.pause) = pause.clone();
        let now = now_s();
        let job = DownloadJob {
            id: uuid::Uuid::new_v4().simple().to_string(),
            ids: known.clone(),
            status: DownloadStatus::Downloading,
            phase: String::new(),
            downloaded: 0,
            total: 0,
            speed_bps: 0,
            waiting_s: 0,
            parts: Vec::new(),
            error_code: None,
            error: None,
            started_at: now,
            updated_at: now,
            retire_after,
        };
        state.active = Some(job.clone());
        self.save(&state);
        drop(state);
        let me = self.clone();
        let id = job.id.clone();
        std::thread::Builder::new()
            .name("setup-download".into())
            .spawn(move || me.run(&id, known, pause, retire_after))
            .map_err(|e| {
                let mut state = lock(&self.inner.state);
                if let Some(j) = state.active.as_mut() {
                    j.status = DownloadStatus::Failed;
                    j.error_code = Some("io".into());
                    j.error = Some(t!("downloads-thread-failed", error = e.to_string()));
                }
                self.save(&state);
                DlError::new("io", t!("downloads-thread-failed", error = e.to_string()))
            })?;
        Ok(job)
    }

    fn run(&self, id: &str, ids: Vec<String>, pause: Arc<AtomicBool>, retire_after: bool) {
        let stop = move || pause.load(Ordering::SeqCst);
        let on_progress = |ev: Value| self.apply_progress(id, &ev);
        // what another program has is taken first, by copy too when it sits on another drive: still faster than the network
        // a panic in the download must end the job as failed: a thread gone silently leaves it «downloading» for good
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            setup::import_from_neighbors(&self.inner.repo_root, &crate::hub::neighbors(), true, Some(&ids));
            setup::download_components(&self.inner.repo_root, &ids, &stop, &on_progress)
        }))
        .unwrap_or_else(|panic| {
            let why = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            tracing::error!("[ERROR] the download thread panicked: {why}");
            Err(setup::DlError::new("panic", t!("downloads-panicked", error = why)))
        });
        if res.is_ok() && retire_after {
            setup::remove_leftovers(&self.inner.repo_root, &crate::models_root(&self.inner.repo_root));
        }
        let mut state = lock(&self.inner.state);
        // Закачку уже сменила следующая — итог этой никому не нужен.
        if let Some(job) = state.active.as_mut().filter(|j| j.id == id) {
            job.speed_bps = 0;
            job.waiting_s = 0;
            job.updated_at = now_s();
            match res {
                Ok(_) => {
                    job.status = DownloadStatus::Completed;
                    job.phase.clear();
                    job.downloaded = job.total;
                    for p in job.parts.iter_mut() {
                        p.done = p.total;
                    }
                }
                Err(e) if e.code == setup::CANCELLED || job.status == DownloadStatus::Paused => {
                    job.status = DownloadStatus::Paused;
                }
                Err(e) => {
                    tracing::warn!("download {:?}: {} ({})", job.ids, e.detail, e.code);
                    crate::hub::setup_failed(&e.code);
                    job.status = DownloadStatus::Failed;
                    job.error_code = Some(e.code.to_string());
                    job.error = Some(e.detail);
                }
            }
            if job.status == DownloadStatus::Completed && job.ids.iter().any(|id| setup::is_model(id)) {
                crate::hub::setup_ready();
            }
        }
        self.save(&state);
    }

    fn apply_progress(&self, id: &str, ev: &Value) {
        let mut state = lock(&self.inner.state);
        let Some(job) = state.active.as_mut().filter(|j| j.id == id && j.status == DownloadStatus::Downloading) else { return };
        let num = |k: &str| ev.get(k).and_then(Value::as_u64);
        if let Some(phase) = ev.get("phase").and_then(Value::as_str) {
            job.phase = phase.to_string();
        }
        if let Some(v) = num("downloaded") {
            job.downloaded = v;
        }
        if let Some(v) = num("total") {
            job.total = v;
        }
        if let Some(v) = num("speed_bps") {
            job.speed_bps = v;
        }
        if let Some(v) = num("waiting_s") {
            job.waiting_s = v;
        }
        if let Some(parts) = ev.get("parts").and_then(Value::as_array) {
            job.parts = parts
                .iter()
                .map(|p| PartProgress {
                    id: p.get("component").and_then(Value::as_str).unwrap_or_default().to_string(),
                    done: p.get("done").and_then(Value::as_u64).unwrap_or(0),
                    total: p.get("total").and_then(Value::as_u64).unwrap_or(0),
                })
                .collect();
        }
        job.updated_at = now_s();
        // Диск — не на каждый тик: после перезапуска достаточно снимка раз в пару секунд.
        if lock(&self.inner.last_persist).elapsed() > Duration::from_secs(2) {
            self.save(&state);
        }
    }

    /// Поставить закачку на паузу: статус «пауза» сразу, воркеры останавливаются на ближайшем чтении, скачанное
    /// остаётся. Поток, дочитывающий кусок, держит компоненты занятыми — следующий старт их подождёт.
    pub fn pause(&self) -> bool {
        let mut state = lock(&self.inner.state);
        let Some(job) = state.active.as_mut().filter(|j| j.status == DownloadStatus::Downloading) else { return false };
        lock(&self.inner.pause).store(true, Ordering::SeqCst);
        job.status = DownloadStatus::Paused;
        job.speed_bps = 0;
        job.waiting_s = 0;
        job.updated_at = now_s();
        self.save(&state);
        true
    }

    /// Забыть остановленную закачку удалённых компонентов, чтобы «Продолжить» не вернуло удалённые гигабайты.
    pub fn forget_covering(&self, removed: &[String]) {
        let mut state = lock(&self.inner.state);
        let stale = state.active.as_ref().is_some_and(|j| {
            j.status != DownloadStatus::Downloading && j.ids.iter().any(|id| removed.contains(id))
        });
        if stale {
            state.active = None;
            self.save(&state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(status: DownloadStatus) -> DownloadJob {
        DownloadJob {
            id: "x".into(),
            ids: vec!["gemma-q8_0".into()],
            status,
            phase: "download".into(),
            downloaded: 5,
            total: 10,
            speed_bps: 7,
            waiting_s: 3,
            parts: vec![],
            error_code: None,
            error: None,
            started_at: 0,
            updated_at: 0,
            retire_after: false,
        }
    }

    #[test]
    fn a_download_running_at_exit_comes_back_as_interrupted() {
        let mut st = PersistentState { active: Some(job(DownloadStatus::Downloading)) };
        assert!(recover_interrupted(&mut st));
        let j = st.active.unwrap();
        assert_eq!(j.status, DownloadStatus::Interrupted);
        assert_eq!(j.error_code.as_deref(), Some("interrupted"));
        assert_eq!((j.speed_bps, j.waiting_s, j.downloaded), (0, 0, 5), "скачанное не обнуляется");

        let mut done = PersistentState { active: Some(job(DownloadStatus::Completed)) };
        assert!(!recover_interrupted(&mut done));
    }

    #[test]
    fn state_survives_a_restart_and_removal_forgets_it() {
        let root = std::env::temp_dir().join(format!("dub-dl-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(root.join("models")).unwrap();
        persist(&root.join("models/.setup-download.json"), &PersistentState { active: Some(job(DownloadStatus::Downloading)) }).unwrap();
        let d = Downloads::open(&root);
        assert_eq!(d.active().unwrap().status, DownloadStatus::Interrupted);
        assert!(!d.pause(), "пауза без идущей закачки ничего не делает");
        d.forget_covering(&["langid".into()]);
        assert!(d.active().is_some(), "чужие компоненты не забываются");
        d.forget_covering(&["gemma-q8_0".into()]);
        assert!(d.active().is_none());
        assert!(Downloads::open(&root).active().is_none(), "забытое не возвращается после перезапуска");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn pause_is_immediate_and_a_late_result_of_an_old_job_is_ignored() {
        let root = std::env::temp_dir().join(format!("dub-dl-{}", uuid::Uuid::new_v4().simple()));
        let d = Downloads::open(&root);
        let flag = Arc::new(AtomicBool::new(false));
        *lock(&d.inner.pause) = flag.clone();
        lock(&d.inner.state).active = Some(job(DownloadStatus::Downloading));
        assert!(d.pause());
        assert!(flag.load(Ordering::SeqCst), "воркеры получают стоп");
        let j = d.active().unwrap();
        assert_eq!((j.status, j.speed_bps), (DownloadStatus::Paused, 0), "статус «пауза» сразу");
        d.apply_progress("x", &serde_json::json!({ "downloaded": 9 }));
        assert_eq!(d.active().unwrap().downloaded, 5, "прогресс остановленной закачки не пишется");
        lock(&d.inner.state).active.as_mut().unwrap().id = "next".into();
        lock(&d.inner.state).active.as_mut().unwrap().status = DownloadStatus::Downloading;
        d.apply_progress("x", &serde_json::json!({ "downloaded": 9 }));
        assert_eq!(d.active().unwrap().downloaded, 5, "прогресс старой закачки не попадает в новую");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_stopped_update_continued_by_its_components_still_removes_the_leftovers() {
        let mut update = job(DownloadStatus::Interrupted);
        update.retire_after = true;
        update.ids = vec!["llama".into(), "ffmpeg".into()];
        assert!(continues_update(Some(&update), &["ffmpeg".into(), "llama".into()]));
        assert!(!continues_update(Some(&update), &["ffmpeg".into()]), "a part of it is another download");
        update.status = DownloadStatus::Completed;
        assert!(!continues_update(Some(&update), &["ffmpeg".into(), "llama".into()]));
        let mut plain = job(DownloadStatus::Paused);
        plain.ids = vec!["ffmpeg".into(), "llama".into()];
        assert!(!continues_update(Some(&plain), &["ffmpeg".into(), "llama".into()]));
        assert!(!continues_update(None, &["ffmpeg".into()]));
        let old: DownloadJob = serde_json::from_value(serde_json::json!({
            "id": "x", "ids": ["llama"], "status": "paused", "phase": "", "downloaded": 0, "total": 0, "speedBps": 0,
            "waitingS": 0, "parts": [], "errorCode": null, "error": null, "startedAt": 0, "updatedAt": 0
        }))
        .unwrap();
        assert!(!old.retire_after, "a state file of a former release reads as a plain download");
    }

    #[test]
    fn unknown_ids_are_refused_before_anything_starts() {
        let root = std::env::temp_dir().join(format!("dub-dl-{}", uuid::Uuid::new_v4().simple()));
        let d = Downloads::open(&root);
        assert_eq!(d.start(vec!["vcruntime".into(), "nope".into()]).unwrap_err().code, "nothing_to_download");
        assert!(d.active().is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
