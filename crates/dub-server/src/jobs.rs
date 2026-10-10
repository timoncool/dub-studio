//! Очередь джоб: ОДИН воркер обрабатывает GPU-задачи строго последовательно (GPU не трогается
//! конкурентно — как в backend/app.py). Прогресс стримится по SSE; последнее событие хранится на джобе
//! и отдаётся первым позднему подписчику. Завершённые джобы остаются в истории (TTL + ограниченный
//! хвост), поэтому результат не теряется, если его первым прочитал кто-то другой.
//!
//! События: {"type":"queued","position":n}, {"type":"running"}, {"type":"progress",...},
//! {"type":"done","result":...}, {"type":"error","error":"..."}, {"type":"cancelled"}.
//!
//! Отмена выполняемой джобы кооперативная: тело проверяет флаг (`check_cancelled`) между стадиями и
//! сегментами, а дочерние процессы джобы (учтённые через `dub_core::proc`) убиваются сразу.

use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, oneshot, Mutex};

use crate::job_store;

/// Функция джобы: получает колбэк прогресса (msg + произвольные поля), возвращает JSON-результат.
pub type JobFn = Box<dyn FnOnce(ProgressFn) -> Result<Value, String> + Send + 'static>;

/// Колбэк прогресса, передаваемый в тело джобы. Кладёт {"type":"progress", ...} в SSE-канал.
pub type ProgressFn = Arc<dyn Fn(Value) + Send + Sync + 'static>;

/// Сколько завершённая джоба живёт в истории.
const HISTORY_TTL: Duration = Duration::from_secs(600);
/// Сколько завершённых джоб держим максимум (старшие вытесняются раньше TTL).
const HISTORY_TAIL: usize = 50;
/// Не чаще раза в интервал переписывать стадию в job.json.
const STAGE_WRITE_EVERY: Duration = Duration::from_secs(1);
/// Верхняя граница ожидания `wait` одним запросом: HTTP-клиенты агентов сдаются около минуты.
pub const WAIT_LONGEST_SECS: u64 = 55;

/// Текст ошибки, которым тело джобы выходит по отмене.
pub const CANCELLED: &str = "job cancelled";

/// Короткий (12 симв.) идентификатор джобы из UUIDv4.
fn new_job_id() -> String {
    let mut id = uuid::Uuid::new_v4().simple().to_string();
    id.truncate(12);
    id
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Пометить в SSE, что стадия взята из чекпоинта: {"type":"progress","stage":..,"resumed":true,..}.
pub fn emit_resumed(progress: &dyn Fn(Value), stage: &str, msg: &str) {
    progress(json!({ "stage": stage, "msg": msg, "resumed": true }));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Analyze,
    Retranslate,
    Remix,
    DubAudio,
    Render,
    ExportLang,
    Download,
    VoicesPack,
    Frame,
    Align,
    Separate,
    DetectText,
    Shorten,
    /// «Собрать глоссарий из текста» (кандидаты в итоге джобы).
    Glossary,
}

impl JobKind {
    const ALL: [JobKind; 14] = [
        JobKind::Analyze,
        JobKind::Retranslate,
        JobKind::Remix,
        JobKind::DubAudio,
        JobKind::Render,
        JobKind::ExportLang,
        JobKind::Download,
        JobKind::VoicesPack,
        JobKind::Frame,
        JobKind::Align,
        JobKind::Separate,
        JobKind::DetectText,
        JobKind::Shorten,
        JobKind::Glossary,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Analyze => "analyze",
            JobKind::Retranslate => "retranslate",
            JobKind::Remix => "remix",
            JobKind::DubAudio => "dub_audio",
            JobKind::Render => "render",
            JobKind::ExportLang => "export_lang",
            JobKind::Download => "download",
            JobKind::VoicesPack => "voices_pack",
            JobKind::Frame => "frame",
            JobKind::Align => "align",
            JobKind::Separate => "separate",
            JobKind::DetectText => "detect_text",
            JobKind::Shorten => "shorten",
            JobKind::Glossary => "glossary",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Класс защиты от дублей: две незавершённые джобы одного класса над одним проектом не ставятся.
    /// text — переписывают проект (перевод/ремикс/анализ), audio — пишут озвучку и выход; сепарация и
    /// чтение вшитого текста проект не меняют и дублируют только сами себя.
    fn class(self) -> Option<&'static str> {
        match self {
            JobKind::Analyze | JobKind::Retranslate | JobKind::Remix | JobKind::Align | JobKind::Shorten | JobKind::Glossary => Some("text"),
            JobKind::DubAudio | JobKind::Render | JobKind::ExportLang => Some("audio"),
            JobKind::Separate => Some("separate"),
            JobKind::DetectText => Some("detect_text"),
            JobKind::Download | JobKind::VoicesPack | JobKind::Frame => None,
        }
    }
}

/// Что ставим: вид, проект и (для персистентных) каталог проекта + аргументы для повтора.
pub struct JobMeta {
    pub kind: JobKind,
    pub pid: Option<String>,
    pub store: Option<(PathBuf, Value)>,
}

impl JobMeta {
    pub fn new(kind: JobKind, pid: Option<&str>) -> Self {
        JobMeta { kind, pid: pid.map(str::to_string), store: None }
    }

    /// Джоба проекта с записью workspace/<pid>/job.json (переживает рестарт, её можно продолжить).
    pub fn persistent(kind: JobKind, pid: &str, dir: PathBuf, args: Value) -> Self {
        JobMeta { kind, pid: Some(pid.to_string()), store: Some((dir, args)) }
    }
}

#[derive(Debug)]
pub enum EnqueueError {
    /// Такая же джоба по этому проекту уже стоит или идёт.
    Conflict { job_id: String, kind: JobKind },
    /// job.json не записался — джобу, которую нельзя сохранить, не запускаем.
    Store(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobStatus {
    Queued,
    Running,
    Done,
    Error,
    Cancelled,
}

impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Error => "error",
            JobStatus::Cancelled => "cancelled",
        }
    }

    fn terminal(self) -> bool {
        matches!(self, JobStatus::Done | JobStatus::Error | JobStatus::Cancelled)
    }
}

/// Управление джобой, общее для воркера, HTTP-ручек и тела джобы: флаг отмены, дочерние процессы,
/// последнее событие, SSE-канал и запись стадии в job.json.
pub struct JobCtl {
    cancel: AtomicBool,
    children: std::sync::Mutex<HashSet<u32>>,
    last: std::sync::Mutex<Option<Value>>,
    tx: broadcast::Sender<Value>,
    /// Каталог проекта и id джобы для job.json (обновляется, только пока запись принадлежит этой джобе).
    store: Option<(PathBuf, String)>,
    stage_write: std::sync::Mutex<StageWrite>,
}

#[derive(Default)]
struct StageWrite {
    stage: String,
    written: String,
    at: Option<Instant>,
}

impl JobCtl {
    fn new(store: Option<(PathBuf, String)>) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        JobCtl {
            cancel: AtomicBool::new(false),
            children: std::sync::Mutex::new(HashSet::new()),
            last: std::sync::Mutex::new(None),
            tx,
            store,
            stage_write: std::sync::Mutex::new(StageWrite::default()),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Взвести отмену и убить дочерние процессы джобы.
    fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let pids: Vec<u32> = self.children.lock().unwrap_or_else(|p| p.into_inner()).iter().copied().collect();
        for pid in pids {
            kill_pid(pid);
        }
    }

    /// Событие, которое становится «последним» (его первым получит поздний подписчик).
    fn publish_last(&self, ev: Value) {
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        *last = Some(ev.clone());
        let _ = self.tx.send(ev);
    }

    fn progress(&self, ev: Value) {
        let mut obj = match ev {
            Value::Object(m) => m,
            other => {
                let mut m = serde_json::Map::new();
                m.insert("msg".into(), other);
                m
            }
        };
        obj.insert("type".into(), json!("progress"));
        let stage = obj.get("stage").and_then(Value::as_str).map(str::to_string);
        self.publish_last(Value::Object(obj));
        if let Some(stage) = stage {
            self.note_stage(&stage);
        }
    }

    fn note_stage(&self, stage: &str) {
        let Some((dir, id)) = &self.store else { return };
        let mut sw = self.stage_write.lock().unwrap_or_else(|p| p.into_inner());
        sw.stage = stage.to_string();
        if sw.written == sw.stage {
            return;
        }
        if sw.at.is_some_and(|t| t.elapsed() < STAGE_WRITE_EVERY) {
            return;
        }
        match job_store::update_owned(dir, id, |r| r.stage = stage.to_string()) {
            Ok(()) => {
                sw.written = sw.stage.clone();
                sw.at = Some(Instant::now());
            }
            Err(e) => eprintln!("[jobs] job.json stage: {e}"),
        }
    }

    fn mark_running(&self) {
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = None;
        let _ = self.tx.send(json!({ "type": "running" }));
        if let Some((dir, id)) = &self.store {
            if let Err(e) = job_store::update_owned(dir, id, |r| r.state = job_store::STATE_RUNNING.into()) {
                eprintln!("[jobs] job.json running: {e}");
            }
        }
    }

    /// Терминал: job.json (состояние + последняя стадия + ошибка) и событие подписчикам.
    fn finish(&self, status: JobStatus, error: Option<&str>, ev: Value) {
        if let Some((dir, id)) = &self.store {
            let stage = self.stage_write.lock().map(|s| s.stage.clone()).unwrap_or_default();
            let state = match status {
                JobStatus::Done => job_store::STATE_DONE,
                JobStatus::Cancelled => job_store::STATE_CANCELLED,
                _ => job_store::STATE_FAILED,
            };
            let err = error.map(job_store::JobError::from_message);
            if let Err(e) = job_store::update_owned(dir, id, |r| {
                r.state = state.into();
                if !stage.is_empty() {
                    r.stage = stage.clone();
                }
                r.error = err.clone();
            }) {
                eprintln!("[jobs] job.json terminal: {e}");
            }
        }
        let _ = self.tx.send(ev);
    }
}

/// Убить процесс по pid (TerminateProcess на Windows). Pid берётся из учёта, пока хэндл процесса у
/// владельца открыт, поэтому переиспользован другим процессом быть не может.
fn kill_pid(pid: u32) -> bool {
    let p = sysinfo::Pid::from_u32(pid);
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
    sys.process(p).map(|pr| pr.kill()).unwrap_or(false)
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<JobCtl>>> = const { RefCell::new(None) };
}

/// Привязка потока к джобе; при drop восстанавливает прежнюю.
pub struct Entered(Option<Arc<JobCtl>>);

impl Drop for Entered {
    fn drop(&mut self) {
        let prev = self.0.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}

/// Привязать текущий поток к джобе: её отмену видит `check_cancelled`, а запущенные из потока
/// процессы попадают в учёт джобы. Для потоков, порождённых телом джобы, звать явно.
pub fn enter(ctl: Arc<JobCtl>) -> Entered {
    let prev = CURRENT.with(|c| c.borrow_mut().replace(ctl));
    Entered(prev)
}

/// Джоба, к которой привязан текущий поток.
pub fn current() -> Option<Arc<JobCtl>> {
    CURRENT.with(|c| c.borrow().clone())
}

pub fn cancelled() -> bool {
    current().is_some_and(|c| c.is_cancelled())
}

/// Точка кооперативной отмены: Err(CANCELLED), если джобу этого потока отменили.
pub fn check_cancelled() -> Result<(), String> {
    if cancelled() {
        Err(CANCELLED.to_string())
    } else {
        Ok(())
    }
}

/// Обновить job.json джобы этого потока (пока запись принадлежит ей).
pub fn update_record(f: impl FnOnce(&mut job_store::JobRecord)) -> Result<(), String> {
    let ctl = current().ok_or("the thread is not bound to a job")?;
    let (dir, id) = ctl.store.as_ref().ok_or("the job has no job.json")?;
    job_store::update_owned(dir, id, f)
}

fn child_hook(pid: u32, alive: bool) {
    let Some(ctl) = current() else { return };
    let mut set = ctl.children.lock().unwrap_or_else(|p| p.into_inner());
    if alive {
        set.insert(pid);
        if ctl.is_cancelled() {
            kill_pid(pid);
        }
    } else {
        set.remove(&pid);
    }
}

fn carry_current() -> Box<dyn FnOnce() -> Box<dyn std::any::Any> + Send> {
    let ctl = current();
    Box::new(move || Box::new(ctl.map(enter)))
}

struct Job {
    kind: JobKind,
    pid: Option<String>,
    created_at: u64,
    started_at: Option<u64>,
    finished_at: Option<u64>,
    finished: Option<Instant>,
    status: JobStatus,
    result: Option<Value>,
    error: Option<String>,
    ctl: Arc<JobCtl>,
    result_sender: Option<oneshot::Sender<Result<Value, String>>>, // разбудить ожидающего preview/original
}

#[derive(Default)]
struct Inner {
    jobs: HashMap<String, Job>,
    queue: VecDeque<String>,
    running: Option<String>,
}

impl Inner {
    /// Позиция в очереди = сколько джоб впереди (включая выполняемую).
    fn position(&self, id: &str) -> Option<usize> {
        let ahead = self.queue.iter().position(|x| x == id)?;
        Some(ahead + usize::from(self.running.is_some()))
    }

    fn announce_positions(&self) {
        for id in &self.queue {
            if let (Some(j), Some(pos)) = (self.jobs.get(id), self.position(id)) {
                if j.kind != JobKind::Frame {
                    j.ctl.publish_last(json!({ "type": "queued", "position": pos }));
                }
            }
        }
    }

    fn conflict(&self, pid: &str, class: &str) -> Option<(String, JobKind)> {
        self.jobs
            .iter()
            .find(|(_, j)| {
                !j.status.terminal() && j.pid.as_deref() == Some(pid) && j.kind.class() == Some(class)
            })
            .map(|(id, j)| (id.clone(), j.kind))
    }

    fn prune(&mut self) {
        self.jobs
            .retain(|_, j| !j.status.terminal() || j.finished.is_none_or(|t| t.elapsed() < HISTORY_TTL));
        let mut done: Vec<(u64, String)> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.status.terminal())
            .map(|(id, j)| (j.finished_at.unwrap_or(0), id.clone()))
            .collect();
        if done.len() > HISTORY_TAIL {
            done.sort();
            for (_, id) in done.iter().take(done.len() - HISTORY_TAIL) {
                self.jobs.remove(id);
            }
        }
    }

    fn snapshot(&self, id: &str) -> Option<Value> {
        let j = self.jobs.get(id)?;
        let last = j.ctl.last.lock().map(|l| l.clone()).unwrap_or(None);
        let field = |k: &str| {
            last.as_ref()
                .filter(|v| v.get("type").and_then(Value::as_str) == Some("progress"))
                .and_then(|v| v.get(k).cloned())
                .unwrap_or(Value::Null)
        };
        let mut v = json!({
            "id": id,
            "kind": j.kind.as_str(),
            "pid": j.pid,
            "status": j.status.as_str(),
            "stage": field("stage"),
            "msg": field("msg"),
            "pct": field("pct"),
            "created_at": j.created_at,
            "started_at": j.started_at,
            "finished_at": j.finished_at,
        });
        if j.status == JobStatus::Queued {
            v["position"] = json!(self.position(id));
        }
        if let Some(r) = &j.result {
            v["result"] = r.clone();
        }
        if let Some(e) = &j.error {
            v["error"] = json!(e);
        }
        Some(v)
    }

    fn terminal_event(&self, id: &str) -> Option<Value> {
        let j = self.jobs.get(id)?;
        match j.status {
            JobStatus::Done => Some(json!({ "type": "done", "result": j.result })),
            JobStatus::Error => Some(json!({ "type": "error", "error": j.error.clone().unwrap_or_default() })),
            JobStatus::Cancelled => Some(json!({ "type": "cancelled" })),
            _ => None,
        }
    }
}

/// Подписка на события джобы: первое событие (терминал, если уже завершена, иначе последнее
/// известное) + канал дальнейших событий.
pub struct Subscription {
    pub first: Option<Value>,
    pub terminal: bool,
    pub rx: broadcast::Receiver<Value>,
}

#[derive(Clone)]
pub struct JobQueue {
    inner: Arc<Mutex<Inner>>,
    submit_tx: tokio::sync::mpsc::UnboundedSender<(String, JobFn)>,
}

impl JobQueue {
    /// Создать очередь и запустить единственный воркер.
    pub fn new() -> Self {
        dub_core::proc::set_hook(child_hook);
        dub_core::proc::set_carry(carry_current);
        let inner: Arc<Mutex<Inner>> = Arc::new(Mutex::new(Inner::default()));
        let (submit_tx, mut submit_rx) = tokio::sync::mpsc::unbounded_channel::<(String, JobFn)>();

        let worker_inner = inner.clone();
        tokio::spawn(async move {
            while let Some((job_id, fn_)) = submit_rx.recv().await {
                let (ctl, kind) = {
                    let mut g = worker_inner.lock().await;
                    g.queue.retain(|x| x != &job_id);
                    let picked = match g.jobs.get_mut(&job_id) {
                        Some(j) if j.status == JobStatus::Queued => {
                            j.status = JobStatus::Running;
                            j.started_at = Some(now_ms());
                            Some((j.ctl.clone(), j.kind))
                        }
                        _ => None, // отменена в очереди или удалена
                    };
                    let Some(picked) = picked else { continue };
                    g.running = Some(job_id.clone());
                    g.announce_positions();
                    picked
                };
                ctl.mark_running();

                let ctl_p = ctl.clone();
                let progress: ProgressFn = Arc::new(move |ev: Value| ctl_p.progress(ev));
                let ctl_b = ctl.clone();
                // Тело джобы синхронное и тяжёлое -> в блокирующий пул, поток привязан к джобе.
                let res = tokio::task::spawn_blocking(move || {
                    let _entered = enter(ctl_b);
                    fn_(progress)
                })
                .await
                .unwrap_or_else(|e| Err(format!("job panicked: {e}")));
                let cancelled = ctl.is_cancelled() && res.is_err();

                let mut g = worker_inner.lock().await;
                g.running = None;
                let mut drop_frame = false;
                if let Some(j) = g.jobs.get_mut(&job_id) {
                    j.finished_at = Some(now_ms());
                    j.finished = Some(Instant::now());
                    let (status, err, ev) = if cancelled {
                        (JobStatus::Cancelled, None, json!({ "type": "cancelled" }))
                    } else {
                        match &res {
                            Ok(v) => {
                                if kind != JobKind::Frame {
                                    j.result = Some(v.clone());
                                }
                                let ev = if kind == JobKind::Frame {
                                    json!({ "type": "done" })
                                } else {
                                    json!({ "type": "done", "result": v })
                                };
                                (JobStatus::Done, None, ev)
                            }
                            Err(e) => {
                                j.error = Some(e.clone());
                                (JobStatus::Error, Some(e.clone()), json!({ "type": "error", "error": e }))
                            }
                        }
                    };
                    j.status = status;
                    ctl.finish(status, err.as_deref(), ev);
                    crate::hub::job_ended(kind, status, err.as_deref());
                    if let Some(sender) = j.result_sender.take() {
                        let delivered = if cancelled {
                            sender.send(Err(CANCELLED.to_string()))
                        } else {
                            sender.send(res)
                        };
                        drop_frame = kind == JobKind::Frame && delivered.is_err();
                    }
                }
                if drop_frame {
                    g.jobs.remove(&job_id); // ожидающий кадра ушёл по таймауту — хранить нечего
                }
                g.announce_positions();
                g.prune();
            }
        });

        JobQueue { inner, submit_tx }
    }

    async fn insert(
        &self,
        meta: JobMeta,
        result_sender: Option<oneshot::Sender<Result<Value, String>>>,
    ) -> Result<String, EnqueueError> {
        let mut g = self.inner.lock().await;
        g.prune();
        if let (Some(pid), Some(class)) = (meta.pid.as_deref(), meta.kind.class()) {
            if let Some((job_id, kind)) = g.conflict(pid, class) {
                return Err(EnqueueError::Conflict { job_id, kind });
            }
        }
        let job_id = new_job_id();
        let store_dir = match meta.store {
            Some((dir, args)) => {
                job_store::write_queued(&dir, meta.kind.as_str(), &args, &job_id)
                    .map_err(EnqueueError::Store)?;
                Some((dir, job_id.clone()))
            }
            None => None,
        };
        let ctl = Arc::new(JobCtl::new(store_dir));
        g.jobs.insert(
            job_id.clone(),
            Job {
                kind: meta.kind,
                pid: meta.pid,
                created_at: now_ms(),
                started_at: None,
                finished_at: None,
                finished: None,
                status: JobStatus::Queued,
                result: None,
                error: None,
                ctl: ctl.clone(),
                result_sender,
            },
        );
        g.queue.push_back(job_id.clone());
        if meta.kind != JobKind::Frame {
            if let Some(pos) = g.position(&job_id) {
                ctl.publish_last(json!({ "type": "queued", "position": pos }));
            }
        }
        Ok(job_id)
    }

    /// Поставить джобу в очередь; вернуть job_id. Персистентная джоба сначала пишет job.json. Её сохранения
    /// проекта — автора запроса, который её поставил (mcp::carry_job).
    pub async fn enqueue(&self, meta: JobMeta, fn_: JobFn) -> Result<String, EnqueueError> {
        let job_id = self.insert(meta, None).await?;
        let _ = self.submit_tx.send((job_id.clone(), crate::mcp::carry_job(fn_)));
        Ok(job_id)
    }

    /// Поставить джобу и получить oneshot-приёмник результата (для синхронного ожидания preview/original).
    pub async fn enqueue_awaitable(
        &self,
        meta: JobMeta,
        fn_: JobFn,
    ) -> Result<(String, oneshot::Receiver<Result<Value, String>>), EnqueueError> {
        let (res_tx, res_rx) = oneshot::channel();
        let job_id = self.insert(meta, Some(res_tx)).await?;
        let _ = self.submit_tx.send((job_id.clone(), crate::mcp::carry_job(fn_)));
        Ok((job_id, res_rx))
    }

    /// Подписаться на события джобы. None — джобы нет (или уже вытеснена из истории).
    pub async fn subscribe(&self, job_id: &str) -> Option<Subscription> {
        let g = self.inner.lock().await;
        let j = g.jobs.get(job_id)?;
        if let Some(ev) = g.terminal_event(job_id) {
            return Some(Subscription { first: Some(ev), terminal: true, rx: j.ctl.tx.subscribe() });
        }
        let last = j.ctl.last.lock().unwrap_or_else(|p| p.into_inner());
        let rx = j.ctl.tx.subscribe();
        Some(Subscription { first: last.clone(), terminal: false, rx })
    }

    pub async fn snapshot(&self, job_id: &str) -> Option<Value> {
        let mut g = self.inner.lock().await;
        g.prune();
        g.snapshot(job_id)
    }

    /// Ждать терминала не дольше `secs` и вернуть снапшот (long-poll).
    pub async fn wait(&self, job_id: &str, secs: u64) -> Option<Value> {
        let sub = self.subscribe(job_id).await?;
        if !sub.terminal {
            let mut rx = sub.rx;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(secs.min(WAIT_LONGEST_SECS));
            loop {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Ok(ev)) if is_terminal_event(&ev) => break,
                    Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                    Ok(Err(_)) | Err(_) => break,
                }
            }
        }
        self.snapshot(job_id).await
    }

    /// Активные и недавние джобы (без кадр-джоб), новые сверху. `pid` — только этого проекта.
    pub async fn list(&self, pid: Option<&str>) -> Vec<Value> {
        let mut g = self.inner.lock().await;
        g.prune();
        let mut ids: Vec<(u64, String)> = g
            .jobs
            .iter()
            .filter(|(_, j)| j.kind != JobKind::Frame)
            .filter(|(_, j)| pid.is_none() || j.pid.as_deref() == pid)
            .map(|(id, j)| (j.created_at, id.clone()))
            .collect();
        ids.sort_by(|a, b| b.cmp(a));
        ids.iter().filter_map(|(_, id)| g.snapshot(id)).collect()
    }

    /// Незавершённая джоба проекта (кроме кадр-джоб): id и вид.
    pub async fn active_for(&self, pid: &str) -> Option<(String, JobKind)> {
        let g = self.inner.lock().await;
        g.jobs
            .iter()
            .find(|(_, j)| !j.status.terminal() && j.kind != JobKind::Frame && j.pid.as_deref() == Some(pid))
            .map(|(id, j)| (id.clone(), j.kind))
    }

    /// Отменить: в очереди — сразу cancelled; выполняется — кооперативная отмена + kill дочерних
    /// процессов (терминал cancelled придёт от воркера).
    pub async fn cancel(&self, job_id: &str) -> Result<Value, CancelError> {
        let mut g = self.inner.lock().await;
        let j = g.jobs.get_mut(job_id).ok_or(CancelError::NotFound)?;
        match j.status {
            JobStatus::Queued => {
                j.status = JobStatus::Cancelled;
                j.finished_at = Some(now_ms());
                j.finished = Some(Instant::now());
                j.ctl.finish(JobStatus::Cancelled, None, json!({ "type": "cancelled" }));
                if let Some(sender) = j.result_sender.take() {
                    let _ = sender.send(Err(CANCELLED.to_string()));
                }
            }
            JobStatus::Running => {
                j.ctl.request_cancel();
                return Ok(json!({ "id": job_id, "status": "cancelling" }));
            }
            _ => return Err(CancelError::Finished),
        }
        if j.kind == JobKind::Frame {
            g.jobs.remove(job_id);
        }
        g.queue.retain(|x| x != job_id);
        g.announce_positions();
        Ok(json!({ "id": job_id, "status": "cancelled" }))
    }

    pub async fn remove(&self, job_id: &str) {
        self.inner.lock().await.jobs.remove(job_id);
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Терминальное ли событие SSE (после него поток закрывается).
pub fn is_terminal_event(ev: &Value) -> bool {
    matches!(ev.get("type").and_then(Value::as_str), Some("done" | "error" | "cancelled"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Джоба, которая ждёт сигнала (или отмены) — держит воркер занятым.
    fn gated(rx: mpsc::Receiver<()>) -> JobFn {
        Box::new(move |p: ProgressFn| {
            p(json!({ "stage": "tts", "msg": "сегмент 1" }));
            loop {
                if rx.recv_timeout(Duration::from_millis(20)).is_ok() {
                    return Ok(json!({ "ok": true }));
                }
                check_cancelled()?;
            }
        })
    }

    async fn until_status(q: &JobQueue, id: &str, status: &str) -> Value {
        for _ in 0..500 {
            let s = q.snapshot(id).await.expect("snapshot");
            if s["status"] == status {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("job {id} не дошла до {status}");
    }

    #[tokio::test]
    async fn duplicate_class_on_same_pid_conflicts() {
        let q = JobQueue::new();
        let (tx, rx) = mpsc::channel();
        let a = q.enqueue(JobMeta::new(JobKind::Render, Some("p1")), gated(rx)).await.unwrap();
        let dup = q.enqueue(JobMeta::new(JobKind::DubAudio, Some("p1")), Box::new(|_| Ok(json!(1)))).await;
        match dup {
            Err(EnqueueError::Conflict { job_id, kind }) => {
                assert_eq!(job_id, a);
                assert_eq!(kind, JobKind::Render);
            }
            other => panic!("ожидался конфликт, получено {other:?}"),
        }
        // Другой класс и другой проект — ставятся.
        assert!(q.enqueue(JobMeta::new(JobKind::Remix, Some("p1")), Box::new(|_| Ok(json!(1)))).await.is_ok());
        assert!(q.enqueue(JobMeta::new(JobKind::Render, Some("p2")), Box::new(|_| Ok(json!(1)))).await.is_ok());
        tx.send(()).unwrap();
        until_status(&q, &a, "done").await;
    }

    #[tokio::test]
    async fn cancel_queued_job_never_runs() {
        let q = JobQueue::new();
        let (tx, rx) = mpsc::channel();
        let first = q.enqueue(JobMeta::new(JobKind::Render, Some("p1")), gated(rx)).await.unwrap();
        until_status(&q, &first, "running").await;
        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = ran.clone();
        let second = q
            .enqueue(
                JobMeta::new(JobKind::Render, Some("p2")),
                Box::new(move |_| {
                    ran2.store(true, Ordering::SeqCst);
                    Ok(json!(1))
                }),
            )
            .await
            .unwrap();
        let snap = q.snapshot(&second).await.unwrap();
        assert_eq!(snap["status"], "queued");
        assert_eq!(snap["position"], 1);
        assert_eq!(q.cancel(&second).await.unwrap()["status"], "cancelled");
        tx.send(()).unwrap();
        until_status(&q, &first, "done").await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!ran.load(Ordering::SeqCst));
        assert_eq!(q.snapshot(&second).await.unwrap()["status"], "cancelled");
        assert_eq!(q.cancel(&second).await, Err(CancelError::Finished));
    }

    #[tokio::test]
    async fn cancel_running_job_is_cooperative() {
        let q = JobQueue::new();
        let (_tx, rx) = mpsc::channel();
        let id = q.enqueue(JobMeta::new(JobKind::Render, Some("p1")), gated(rx)).await.unwrap();
        until_status(&q, &id, "running").await;
        assert_eq!(q.cancel(&id).await.unwrap()["status"], "cancelling");
        until_status(&q, &id, "cancelled").await;
    }

    #[tokio::test]
    async fn cancel_kills_tracked_child_process() {
        let q = JobQueue::new();
        let id = q
            .enqueue(
                JobMeta::new(JobKind::Render, Some("p1")),
                Box::new(|p: ProgressFn| {
                    p(json!({ "stage": "separate" }));
                    let mut cmd = if cfg!(windows) {
                        let mut c = std::process::Command::new("ping");
                        c.args(["-n", "60", "127.0.0.1"]);
                        c
                    } else {
                        let mut c = std::process::Command::new("sleep");
                        c.arg("60");
                        c
                    };
                    let _ = dub_core::proc::output(&mut cmd);
                    check_cancelled()?;
                    Ok(json!("процесс не убит"))
                }),
            )
            .await
            .unwrap();
        for _ in 0..500 {
            let s = q.snapshot(&id).await.unwrap();
            if s["stage"] == "separate" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await; // процесс успел стартовать и попасть в учёт
        let t0 = Instant::now();
        q.cancel(&id).await.unwrap();
        until_status(&q, &id, "cancelled").await;
        assert!(t0.elapsed() < Duration::from_secs(5), "отмена ждала завершения процесса");
    }

    #[tokio::test]
    async fn late_subscriber_gets_last_progress_then_terminal_from_history() {
        let q = JobQueue::new();
        let (tx, rx) = mpsc::channel();
        let id = q.enqueue(JobMeta::new(JobKind::Render, Some("p1")), gated(rx)).await.unwrap();
        let mut first = None;
        for _ in 0..500 {
            let sub = q.subscribe(&id).await.unwrap();
            if sub.first.as_ref().and_then(|v| v.get("type")).and_then(Value::as_str) == Some("progress") {
                first = sub.first;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let first = first.expect("поздний подписчик не получил прогресс");
        assert_eq!(first["stage"], "tts");
        tx.send(()).unwrap();
        until_status(&q, &id, "done").await;
        // Два подписчика подряд после терминала — оба видят результат (история, а не удаление).
        for _ in 0..2 {
            let sub = q.subscribe(&id).await.unwrap();
            assert!(sub.terminal);
            assert_eq!(sub.first.unwrap()["result"]["ok"], true);
        }
        let listed = q.list(Some("p1")).await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["kind"], "render");
    }

    #[tokio::test]
    async fn wait_returns_on_terminal_or_deadline() {
        let q = JobQueue::new();
        let (tx, rx) = mpsc::channel();
        let id = q.enqueue(JobMeta::new(JobKind::Remix, Some("p1")), gated(rx)).await.unwrap();
        let snap = q.wait(&id, 0).await.unwrap();
        assert_ne!(snap["status"], "done");
        tx.send(()).unwrap();
        let snap = q.wait(&id, 5).await.unwrap();
        assert_eq!(snap["status"], "done");
    }
}
