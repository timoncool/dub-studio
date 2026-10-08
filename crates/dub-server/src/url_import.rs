//! Видео по ссылке: проба (что за видео, какие качества, субтитры площадки) и загрузка в новый проект. Загрузка
//! идёт своим потоком мимо GPU-очереди джоб: у джоб один воркер на видеокарту, и часовая закачка по сети держала бы
//! рендеры. Состояние лежит в workspace/.fetch/fetches.json и переживает перезапуск: оборванная загрузка
//! показывается «прервано» и продолжается с места (недокачанное yt-dlp остаётся в её папке workspace/.fetch/<id>).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ytdlp::{self, Line, Quality, UrlError};
use crate::AppState;

/// Сколько загрузок помнит список (старые завершённые уходят вместе со своими папками).
const KEEP: usize = 20;
/// cookies.txt больше этого не бывает.
const COOKIES_LIMIT: u64 = 1024 * 1024;
/// Проба ссылки (`-J`) дольше этого — сайт не отвечает.
const PROBE_TIMEOUT: Duration = Duration::from_secs(180);
/// Шаг субтитров площадки дольше этого — сайт не отвечает (при устаревших адресах yt-dlp заново читает страницу).
const SUBS_TIMEOUT: Duration = Duration::from_secs(180);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FetchStatus {
    Downloading,
    Completed,
    Failed,
    Cancelled,
    /// Оборвана закрытием или падением студии; «Продолжить» докачивает с места.
    Interrupted,
}

/// Одна загрузка по ссылке.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fetch {
    pub id: String,
    pub url: String,
    pub quality: Quality,
    /// Язык субтитров площадки, загруженных людьми; они становятся импортом субтитров проекта.
    pub subs_lang: Option<String>,
    pub cookies: bool,
    pub status: FetchStatus,
    /// probe | download | merge | extract | subtitles | project | done
    pub phase: String,
    pub title: Option<String>,
    pub duration: Option<f64>,
    pub downloaded: u64,
    /// Всего байт (сумма форматов, названная сайтом, или счётчики yt-dlp); null — неизвестно.
    pub total: Option<u64>,
    pub speed_bps: u64,
    pub eta_s: Option<u64>,
    /// Проект, в который легло видео.
    pub pid: Option<String>,
    pub subs_imported: bool,
    /// Субтитры выбраны, но не легли в проект: subs_missing (у видео нет таких) | subs_failed | subs_empty.
    pub warning: Option<String>,
    pub warning_detail: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub hint: Option<String>,
    pub tool_version: Option<String>,
    pub started_at: u64,
    pub updated_at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Persistent {
    fetches: Vec<Fetch>,
}

/// Откуда взять cookies.txt: путь на этом компьютере (агент) или содержимое файла (окно выбирает файл, а путь
/// браузер не отдаёт).
#[derive(Clone, Debug)]
pub enum Cookies {
    Path(PathBuf),
    Text(String),
}

/// Запрос загрузки.
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub url: String,
    pub quality: Quality,
    pub subs_lang: Option<String>,
    pub cookies: Option<Cookies>,
}

struct Inner {
    repo_root: PathBuf,
    root: PathBuf,
    state_path: PathBuf,
    list: Mutex<Vec<Fetch>>,
    stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
    last_persist: Mutex<Instant>,
}

#[derive(Clone)]
pub struct Fetches {
    inner: Arc<Inner>,
}

fn persist(path: &Path, list: &[Fetch]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let body = serde_json::to_vec_pretty(&json!({ "fetches": list })).map_err(|e| e.to_string())?;
    dub_core::atomic::write(path, &body)
}

/// Загрузка, оставшаяся «идущей» в файле, оборвалась вместе с прошлым процессом.
fn recover_interrupted(list: &mut [Fetch]) -> bool {
    let mut any = false;
    for f in list.iter_mut().filter(|f| f.status == FetchStatus::Downloading) {
        f.status = FetchStatus::Interrupted;
        f.error_code = Some("interrupted".into());
        f.error = Some(t!("url-interrupted"));
        f.hint = Some(ytdlp::hint("interrupted").into());
        f.speed_bps = 0;
        f.eta_s = None;
        any = true;
    }
    any
}

/// Проверенная ссылка: http(s) с хостом.
fn check_url(url: &str) -> Result<String, UrlError> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|e| UrlError::new("bad_url", t!("url-not-a-link", url = url.to_string(), error = e.to_string())))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none_or(str::is_empty) {
        return Err(UrlError::new("bad_url", t!("url-not-http", url = url.to_string())));
    }
    Ok(parsed.to_string())
}

/// Код языка субтитров как его пишет yt-dlp (en, pt-BR, zh-Hans): только буквы, цифры и дефис.
fn check_lang(lang: &str) -> Result<String, UrlError> {
    let lang = lang.trim();
    if lang.is_empty() || lang.len() > 24 || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(UrlError::new("bad_url", t!("url-bad-subs-lang", lang = lang.to_string())));
    }
    Ok(lang.to_string())
}

/// Положить cookies.txt в `dest`: копия, а не исходный файл — yt-dlp дописывает в файл cookies свои.
fn place_cookies(c: &Cookies, dest: &Path) -> Result<(), UrlError> {
    let text = match c {
        Cookies::Path(p) => {
            let meta = std::fs::metadata(p).map_err(|e| UrlError::new("cookies_invalid", format!("{}: {e}", p.display())))?;
            if !meta.is_file() || meta.len() > COOKIES_LIMIT {
                return Err(UrlError::new("cookies_invalid", t!("url-cookies-not-file", path = p.display().to_string())));
            }
            std::fs::read_to_string(p).map_err(|e| UrlError::new("cookies_invalid", format!("{}: {e}", p.display())))?
        }
        Cookies::Text(t) => t.clone(),
    };
    if text.trim().is_empty() || text.len() as u64 > COOKIES_LIMIT {
        return Err(UrlError::new("cookies_invalid", t!("url-cookies-empty-or-large")));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| UrlError::new("io", format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(dest, text).map_err(|e| UrlError::new("io", format!("{}: {e}", dest.display())))
}

/// Имя проекта из названия видео: без символов, запрещённых в именах файлов Windows.
fn project_name(title: &str, ext: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let clean: String = clean.chars().take(120).collect();
    let clean = clean.trim_end_matches(['.', ' ']);
    format!("{}.{ext}", if clean.is_empty() { "video" } else { clean })
}

impl Fetches {
    pub fn open(repo_root: &Path, workspace: &Path) -> Self {
        let root = workspace.join(".fetch");
        let state_path = root.join("fetches.json");
        let mut state: Persistent = match std::fs::read_to_string(&state_path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("{} is unreadable ({e}); the previous link downloads are forgotten", state_path.display());
                Persistent::default()
            }),
            Err(_) => Persistent::default(),
        };
        if recover_interrupted(&mut state.fetches) {
            if let Err(e) = persist(&state_path, &state.fetches) {
                tracing::warn!("{}: {e}", state_path.display());
            }
        }
        Fetches {
            inner: Arc::new(Inner {
                repo_root: repo_root.to_path_buf(),
                root,
                state_path,
                list: Mutex::new(state.fetches),
                stops: Mutex::new(HashMap::new()),
                last_persist: Mutex::new(Instant::now()),
            }),
        }
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.inner.root.join(id)
    }

    fn save(&self, list: &[Fetch]) {
        if let Err(e) = persist(&self.inner.state_path, list) {
            tracing::warn!("{}: {e}", self.inner.state_path.display());
        }
        *lock(&self.inner.last_persist) = Instant::now();
    }

    pub fn list(&self) -> Vec<Fetch> {
        lock(&self.inner.list).clone()
    }

    pub fn get(&self, id: &str) -> Option<Fetch> {
        lock(&self.inner.list).iter().find(|f| f.id == id).cloned()
    }

    /// Изменить загрузку `id`, пока она идёт; на диск — не чаще раза в пару секунд.
    fn update(&self, id: &str, change: impl FnOnce(&mut Fetch)) {
        let mut list = lock(&self.inner.list);
        let Some(f) = list.iter_mut().find(|f| f.id == id && f.status == FetchStatus::Downloading) else { return };
        change(f);
        f.updated_at = ytdlp::now_s();
        if lock(&self.inner.last_persist).elapsed() > Duration::from_secs(2) {
            self.save(&list);
        }
    }

    /// Начать загрузку своим потоком. Нет yt-dlp или ffmpeg, негодная ссылка или cookies — отказ до старта.
    pub fn start(&self, req: FetchRequest) -> Result<Fetch, UrlError> {
        let url = check_url(&req.url)?;
        let subs_lang = req.subs_lang.as_deref().map(check_lang).transpose()?;
        let tool = ytdlp::tool(&self.inner.repo_root)?;
        ytdlp::update_in_background_if_due(&self.inner.repo_root);
        let mut id = uuid::Uuid::new_v4().simple().to_string();
        id.truncate(12);
        let id = format!("url{id}");
        let now = ytdlp::now_s();
        let fetch = Fetch {
            id: id.clone(),
            url,
            quality: req.quality,
            subs_lang,
            cookies: req.cookies.is_some(),
            status: FetchStatus::Downloading,
            phase: "probe".into(),
            title: None,
            duration: None,
            downloaded: 0,
            total: None,
            speed_bps: 0,
            eta_s: None,
            pid: None,
            subs_imported: false,
            warning: None,
            warning_detail: None,
            error_code: None,
            error: None,
            hint: None,
            tool_version: Some(tool.version.clone()),
            started_at: now,
            updated_at: now,
        };
        // Проверка «эта ссылка уже качается» и вставка — под одним замком: два запроса одной ссылки подряд не
        // становятся двумя загрузками.
        {
            let mut list = lock(&self.inner.list);
            if let Some(busy) = list.iter().find(|f| f.status == FetchStatus::Downloading && f.url == fetch.url) {
                return Err(UrlError::new("busy", t!("url-already-downloading", id = busy.id.clone())));
            }
            list.insert(0, fetch.clone());
            self.evict(&mut list);
            self.save(&list);
        }
        let dir = self.dir(&id);
        let placed = std::fs::create_dir_all(&dir)
            .map_err(|e| UrlError::new("io", format!("{}: {e}", dir.display())))
            .and_then(|()| req.cookies.as_ref().map_or(Ok(()), |c| place_cookies(c, &dir.join("cookies.txt"))));
        if let Err(e) = placed {
            self.drop_unstarted(&id, &dir);
            return Err(e);
        }
        self.spawn(&id, tool)?;
        Ok(fetch)
    }

    /// Убрать из списка загрузку, которая не началась (папка или cookies не легли).
    fn drop_unstarted(&self, id: &str, dir: &Path) {
        if dir.is_dir() {
            if let Err(e) = std::fs::remove_dir_all(dir) {
                tracing::warn!("the folder of a download that did not start, {}, was not removed: {e}", dir.display());
            }
        }
        let mut list = lock(&self.inner.list);
        list.retain(|f| f.id != id);
        self.save(&list);
    }

    /// Старые завершённые загрузки уходят из списка вместе с папками; идущие остаются всегда.
    fn evict(&self, list: &mut Vec<Fetch>) {
        while list.len() > KEEP {
            let Some(at) = list.iter().rposition(|f| f.status != FetchStatus::Downloading) else { break };
            let old = list.remove(at);
            let dir = self.dir(&old.id);
            if dir.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    tracing::warn!("the folder of an old download, {}, was not removed: {e}", dir.display());
                }
            }
        }
    }

    fn spawn(&self, id: &str, tool: ytdlp::Tool) -> Result<(), UrlError> {
        let stop = Arc::new(AtomicBool::new(false));
        lock(&self.inner.stops).insert(id.to_string(), stop.clone());
        let me = self.clone();
        let job = id.to_string();
        std::thread::Builder::new()
            .name("url-fetch".into())
            .spawn(move || me.run(&job, tool, stop))
            .map(|_| ())
            .map_err(|e| {
                self.finish(id, Err(UrlError::new("io", t!("url-thread-failed", error = e.to_string()))), false);
                UrlError::new("io", t!("url-thread-failed", error = e.to_string()))
            })
    }

    fn run(&self, id: &str, tool: ytdlp::Tool, stop: Arc<AtomicBool>) {
        let res = self.run_steps(id, &tool, &stop);
        self.finish(id, res, stop.load(Ordering::SeqCst));
    }

    /// Итог загрузки. Остановленная убирает свою папку; упавшая её оставляет — «Продолжить» докачает.
    fn finish(&self, id: &str, res: Result<(), UrlError>, stopped: bool) {
        lock(&self.inner.stops).remove(id);
        let mut list = lock(&self.inner.list);
        let Some(f) = list.iter_mut().find(|f| f.id == id) else { return };
        f.speed_bps = 0;
        f.eta_s = None;
        f.updated_at = ytdlp::now_s();
        let cancelled = stopped || res.as_ref().is_err_and(|e| e.code == "cancelled");
        match res {
            Ok(()) => {}
            Err(_) if cancelled => {
                f.status = FetchStatus::Cancelled;
                let dir = self.dir(id);
                if dir.is_dir() {
                    if let Err(e) = std::fs::remove_dir_all(&dir) {
                        tracing::warn!("the folder of a cancelled download, {}, was not removed: {e}", dir.display());
                    }
                }
            }
            Err(e) => {
                tracing::warn!("link download {}: {} ({})", f.url, e.detail, e.code);
                f.status = FetchStatus::Failed;
                f.hint = Some(e.hint().into());
                f.error_code = Some(e.code.into());
                f.error = Some(e.detail);
            }
        }
        // Пароль прокси живёт в хранилище, а не в папке загрузки: proxy.conf пишется заново к каждому запуску.
        let conf = self.dir(id).join(ytdlp::PROXY_CONF);
        match std::fs::remove_file(&conf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::error!("{} with the proxy password was not removed: {e}", conf.display()),
        }
        self.save(&list);
    }

    fn run_steps(&self, id: &str, tool: &ytdlp::Tool, stop: &Arc<AtomicBool>) -> Result<(), UrlError> {
        let fetch = self.get(id).ok_or_else(|| UrlError::new("not_found", id.to_string()))?;
        let dir = self.dir(id);
        std::fs::create_dir_all(&dir).map_err(|e| UrlError::new("io", format!("{}: {e}", dir.display())))?;
        let cookies = dir.join("cookies.txt");
        let with_cookies = |cmd: &mut std::process::Command| {
            if fetch.cookies {
                cmd.arg("--cookies").arg(&cookies);
            }
        };
        if fetch.cookies && !cookies.is_file() {
            return Err(UrlError::new("cookies_invalid", t!("url-cookies-gone")));
        }

        // Проба: свежие адреса форматов (у YouTube они живут часы), название, размер, субтитры.
        self.update(id, |f| f.phase = "probe".into());
        let mut cmd = tool.command(&fetch.url, &dir)?;
        cmd.args(ytdlp::PROBE_ARGS).args(fetch.quality.args());
        with_cookies(&mut cmd);
        cmd.arg("--").arg(&fetch.url);
        let stop_probe = stop.clone();
        let out = ytdlp::run_captured(cmd, &move || stop_probe.load(Ordering::SeqCst), Some(PROBE_TIMEOUT))?;
        if !out.success {
            return Err(ytdlp::classify(&out.stderr));
        }
        let info: Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| UrlError::new("ytdlp_failed", t!("url-probe-not-json", error = e.to_string(), stderr = ytdlp::last_error_line(&out.stderr))))?;
        let probe = ytdlp::parse_probe(&info)?;
        let info_path = dir.join("info.json");
        std::fs::write(&info_path, &out.stdout).map_err(|e| UrlError::new("io", format!("{}: {e}", info_path.display())))?;
        let expected = probe.expected_bytes;
        if let (Some(need), Some(free)) = (expected, crate::setup::free_bytes(&dir)) {
            // Слияние видео и звука держит на диске и части, и итог.
            if free < need.saturating_mul(2) {
                return Err(UrlError::new("disk_space", t!("url-disk-space", need = need * 2 / 1_000_000, free = free / 1_000_000)));
            }
        }
        let mut subs_lang = fetch.subs_lang.clone();
        if let Some(lang) = &fetch.subs_lang {
            if !probe.subtitles.iter().any(|t| &t.lang == lang) {
                let have: Vec<&str> = probe.subtitles.iter().map(|t| t.lang.as_str()).collect();
                let detail = if have.is_empty() {
                    t!("url-no-subs-none", lang = lang.to_string())
                } else {
                    t!("url-no-subs", lang = lang.to_string(), have = have.join(", "))
                };
                self.update(id, |f| {
                    f.warning = Some("subs_missing".into());
                    f.warning_detail = Some(detail);
                });
                subs_lang = None;
            }
        }
        self.update(id, |f| {
            f.title = Some(probe.title.clone());
            f.duration = probe.duration;
            f.total = expected;
            f.phase = "download".into();
        });

        // Загрузка по полученному описанию: форматы не выбираются второй раз с другой страницы.
        // Папка — через -P: знак % в её пути шаблон -o прочитал бы как поле.
        let mut cmd = tool.command(&fetch.url, &dir)?;
        cmd.arg("--load-info-json").arg(&info_path).args(fetch.quality.args());
        cmd.arg("-P").arg(&dir).args(["-o", "source.%(ext)s"]);
        with_cookies(&mut cmd);
        cmd.args(["--newline", "--progress", "--progress-template", ytdlp::PROGRESS_TEMPLATE, "--progress-template", ytdlp::POSTPROCESS_TEMPLATE, "--print", ytdlp::FILE_TEMPLATE]);
        let (media, stderr) = self.download(id, cmd, expected, stop)?;
        if stop.load(Ordering::SeqCst) {
            return Err(UrlError::new("cancelled", t!("url-stopped")));
        }
        let media = match media.filter(|p| p.is_file()) {
            Some(p) => p,
            None => find_media(&dir).ok_or_else(|| UrlError::new("ytdlp_failed", t!("url-no-media-file", path = dir.display().to_string(), stderr = ytdlp::last_error_line(&stderr))))?,
        };

        // Субтитры площадки — отдельным вызовом после видео: в общем вызове yt-dlp пишет их до медиа, и их ошибка
        // бросает всю загрузку.
        self.after_media(id, &media, &probe.title, subs_lang.as_deref(), stop, |lang| {
            let mut cmd = tool.command(&fetch.url, &dir)?;
            cmd.arg("--load-info-json").arg(&info_path).args(fetch.quality.args()).arg("--skip-download");
            cmd.args(["--write-subs", "--no-write-auto-subs", "--sub-langs", lang, "--convert-subs", "srt"]);
            cmd.arg("-P").arg(&dir).args(["-o", "source.%(ext)s", "-o", "subtitle:subs.%(ext)s"]);
            with_cookies(&mut cmd);
            let stop_subs = stop.clone();
            ytdlp::run_captured(cmd, &move || stop_subs.load(Ordering::SeqCst), Some(SUBS_TIMEOUT))
        })
    }

    /// Видео скачано: субтитры площадки (если выбраны) шагом `fetch_subs`, затем проект из файла, как из выбранного
    /// на диске, с субтитрами площадки импортом субтитров. Сбой шага субтитров — предупреждение subs_failed, а не
    /// провал: проект создаётся без них. Остановка остаётся остановкой.
    fn after_media(
        &self,
        id: &str,
        media: &Path,
        title: &str,
        subs_lang: Option<&str>,
        stop: &Arc<AtomicBool>,
        fetch_subs: impl FnOnce(&str) -> Result<ytdlp::Captured, UrlError>,
    ) -> Result<(), UrlError> {
        let dir = self.dir(id);
        let subs = match subs_lang {
            Some(lang) => {
                self.update(id, |f| f.phase = "subtitles".into());
                match fetch_subs(lang).and_then(|out| subs_file(&dir, out)) {
                    Ok(p) => Some(p),
                    Err(e) if e.code == "cancelled" => return Err(e),
                    Err(e) => {
                        tracing::warn!("subtitles {lang} of download {id}: {} ({})", e.detail, e.code);
                        let detail = t!("url-subs-failed", lang = lang.to_string(), code = e.code, error = e.detail.clone());
                        self.update(id, |f| {
                            f.warning = Some("subs_failed".into());
                            f.warning_detail = Some(detail);
                        });
                        None
                    }
                }
            }
            None => None,
        };
        if stop.load(Ordering::SeqCst) {
            return Err(UrlError::new("cancelled", t!("url-stopped")));
        }
        self.update(id, |f| f.phase = "project".into());
        let made = make_project(&self.inner.root, media, title, subs.as_deref())?;
        // Проект уже есть: загрузка записывается готовой с ним и тогда, когда отмена пришла, пока он создавался, —
        // иначе он остался бы в списке проектов без ссылки из загрузки.
        {
            let mut list = lock(&self.inner.list);
            if let Some(f) = list.iter_mut().find(|f| f.id == id) {
                if let Some((code, detail)) = made.warning {
                    f.warning = Some(code.into());
                    f.warning_detail = Some(detail);
                }
                f.pid = Some(made.pid.clone());
                f.subs_imported = made.subs_imported;
                f.downloaded = f.total.unwrap_or(f.downloaded).max(f.downloaded);
                f.phase = "done".into();
                f.status = FetchStatus::Completed;
                f.error_code = None;
                f.error = None;
                f.hint = None;
                f.updated_at = ytdlp::now_s();
            }
            self.save(&list);
        }
        crate::mcp::tell_windows(json!({ "changed": "projects", "pid": made.pid, "by": "studio" }));
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            tracing::warn!("the download folder {} was not removed after moving into the project: {e}", dir.display());
        }
        Ok(())
    }

    /// Запустить загрузку yt-dlp и вести прогресс по его строкам. Возвращает путь итогового файла и stderr.
    fn download(&self, id: &str, mut cmd: std::process::Command, expected: Option<u64>, stop: &Arc<AtomicBool>) -> Result<(Option<PathBuf>, String), UrlError> {
        cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| UrlError::new("io", t!("url-ytdlp-start", error = e.to_string())))?;
        let pid = child.id();
        let (tx, rx) = mpsc::channel::<(bool, String)>();
        let readers: Vec<_> = [
            child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
            child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(i, pipe)| {
            let tx = tx.clone();
            pipe.map(|p| ytdlp::lines_of(p, move |line| {
                let _ = tx.send((i == 1, line.to_string()));
            }))
        })
        .collect();
        drop(tx);
        let mut streams = ytdlp::Streams::default();
        let mut file: Option<PathBuf> = None;
        let mut stderr: Vec<String> = Vec::new();
        let mut killed = false;
        let mut handle = |is_err: bool, line: String, streams: &mut ytdlp::Streams| match ytdlp::parse_line(&line) {
            Line::Progress(p) => {
                let (done, total) = streams.apply(&p, expected);
                self.update(id, |f| {
                    f.downloaded = done;
                    f.total = total;
                    f.speed_bps = p.speed.map_or(0, |s| s as u64);
                    f.eta_s = p.eta;
                    f.phase = "download".into();
                });
            }
            Line::Post(state, name) if state == "started" => {
                let phase = match name.as_str() {
                    // Имена обработчиков yt-dlp — класс без FFmpeg и PP: FFmpegMergerPP -> Merger.
                    "Merger" => "merge",
                    "ExtractAudio" => "extract",
                    _ => return,
                };
                self.update(id, |f| {
                    f.phase = phase.into();
                    f.speed_bps = 0;
                    f.eta_s = None;
                });
            }
            Line::File(p) => file = Some(p),
            Line::Post(..) => {}
            Line::Other => {
                if is_err && !line.trim().is_empty() {
                    stderr.push(line);
                    if stderr.len() > 200 {
                        stderr.remove(0);
                    }
                }
            }
        };
        let status = loop {
            while let Ok((is_err, line)) = rx.try_recv() {
                handle(is_err, line, &mut streams);
            }
            match child.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) => {}
                Err(e) => {
                    ytdlp::kill_tree(pid);
                    return Err(UrlError::new("io", t!("url-ytdlp-wait", error = e.to_string())));
                }
            }
            if !killed && stop.load(Ordering::SeqCst) {
                ytdlp::kill_tree(pid);
                killed = true;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        for r in readers {
            let _ = r.join();
        }
        while let Ok((is_err, line)) = rx.try_recv() {
            handle(is_err, line, &mut streams);
        }
        let stderr = stderr.join("\n");
        if killed {
            return Err(UrlError::new("cancelled", t!("url-stopped")));
        }
        if !status.success() {
            return Err(ytdlp::classify(&stderr));
        }
        Ok((file, stderr))
    }

    /// Остановить загрузку: процесс гасится со всеми потомками, её папка уходит.
    pub fn cancel(&self, id: &str) -> Result<Fetch, UrlError> {
        let stop = lock(&self.inner.stops).get(id).cloned();
        let mut list = lock(&self.inner.list);
        let f = list.iter_mut().find(|f| f.id == id).ok_or_else(|| UrlError::new("not_found", t!("url-no-fetch", id = id.to_string())))?;
        if f.status != FetchStatus::Downloading {
            return Ok(f.clone());
        }
        if let Some(stop) = stop {
            stop.store(true, Ordering::SeqCst);
        }
        f.status = FetchStatus::Cancelled;
        f.speed_bps = 0;
        f.eta_s = None;
        f.updated_at = ytdlp::now_s();
        let out = f.clone();
        self.save(&list);
        Ok(out)
    }

    /// Продолжить прерванную или упавшую загрузку с теми же настройками; скачанное в её папке берётся с места.
    pub fn resume(&self, id: &str) -> Result<Fetch, UrlError> {
        let tool = ytdlp::tool(&self.inner.repo_root)?;
        let out = {
            let mut list = lock(&self.inner.list);
            let f = list.iter_mut().find(|f| f.id == id).ok_or_else(|| UrlError::new("not_found", t!("url-no-fetch", id = id.to_string())))?;
            match f.status {
                FetchStatus::Downloading => return Err(UrlError::new("running", t!("url-still-downloading", id = id.to_string()))),
                FetchStatus::Completed => return Ok(f.clone()),
                FetchStatus::Cancelled => {
                    return Err(UrlError::new("not_found", t!("url-cancelled-removed", id = id.to_string())));
                }
                FetchStatus::Failed | FetchStatus::Interrupted => {}
            }
            let url = f.url.clone();
            if let Some(busy) = list.iter().find(|x| x.status == FetchStatus::Downloading && x.url == url) {
                return Err(UrlError::new("busy", t!("url-already-downloading", id = busy.id.clone())));
            }
            let f = list.iter_mut().find(|f| f.id == id).ok_or_else(|| UrlError::new("not_found", t!("url-no-fetch", id = id.to_string())))?;
            f.status = FetchStatus::Downloading;
            f.phase = "probe".into();
            f.error_code = None;
            f.error = None;
            f.hint = None;
            f.warning = None;
            f.warning_detail = None;
            f.tool_version = Some(tool.version.clone());
            f.updated_at = ytdlp::now_s();
            let out = f.clone();
            self.save(&list);
            out
        };
        ytdlp::update_in_background_if_due(&self.inner.repo_root);
        self.spawn(id, tool)?;
        Ok(out)
    }

    /// Убрать загрузку из списка вместе с недокачанным. Идущую сначала отменяют.
    pub fn forget(&self, id: &str) -> Result<(), UrlError> {
        // Отменённая загрузка, чей поток ещё гасит yt-dlp: его файлы в папке ещё заняты.
        if lock(&self.inner.stops).contains_key(id) {
            return Err(UrlError::new("running", t!("url-still-stopping", id = id.to_string())));
        }
        let mut list = lock(&self.inner.list);
        let at = list.iter().position(|f| f.id == id).ok_or_else(|| UrlError::new("not_found", t!("url-no-fetch", id = id.to_string())))?;
        if list[at].status == FetchStatus::Downloading {
            return Err(UrlError::new("running", t!("url-cancel-first", id = id.to_string())));
        }
        let dir = self.dir(id);
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir).map_err(|e| UrlError::new("io", format!("{}: {e}", dir.display())))?;
        }
        list.remove(at);
        self.save(&list);
        Ok(())
    }
}

/// Итоговый файл в папке загрузки, если yt-dlp не назвал его сам: `source.<ext>`, а не недокачанный `.part` и не
/// часть слияния `source.f137.mp4`.
fn find_media(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        name.strip_prefix("source.").is_some_and(|ext| !ext.is_empty() && !ext.contains('.')) && p.is_file()
    })
}

/// Субтитры, сведённые yt-dlp в SRT: `subs.<язык>.srt`.
fn find_subs(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        name.starts_with("subs.") && name.ends_with(".srt") && p.is_file()
    })
}

/// Файл, который оставил шаг субтитров yt-dlp; его провал — код по тексту yt-dlp.
fn subs_file(dir: &Path, out: ytdlp::Captured) -> Result<PathBuf, UrlError> {
    if !out.success {
        return Err(ytdlp::classify(&out.stderr));
    }
    find_subs(dir).ok_or_else(|| {
        let said = [ytdlp::last_error_line(&out.stderr), ytdlp::last_error_line(&String::from_utf8_lossy(&out.stdout))]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        UrlError::new("ytdlp_failed", t!("url-no-subs-file", path = dir.display().to_string(), stderr = said.to_string()))
    })
}

struct Made {
    pid: String,
    subs_imported: bool,
    warning: Option<(&'static str, String)>,
}

/// Новый проект из скачанного файла: файл переезжает в workspace/<pid>/source.<ext> (тот же том — мгновенно),
/// имя проекта — название видео, субтитры площадки — import_subs.srt, как у загруженных вместе с видео.
fn make_project(fetch_root: &Path, media: &Path, title: &str, subs: Option<&Path>) -> Result<Made, UrlError> {
    let workspace = fetch_root.parent().ok_or_else(|| UrlError::new("io", t!("url-no-parent", path = fetch_root.display().to_string())))?;
    let mut pid = uuid::Uuid::new_v4().simple().to_string();
    pid.truncate(12);
    let d = workspace.join(&pid);
    let io = |what: &Path, e: std::io::Error| UrlError::new("io", format!("{}: {e}", what.display()));
    std::fs::create_dir_all(&d).map_err(|e| io(&d, e))?;
    let ext = media.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_ascii_lowercase();
    let dst = d.join(format!("source.{ext}"));
    std::fs::rename(media, &dst).map_err(|e| io(&dst, e))?;
    std::fs::write(d.join("source.txt"), dst.to_string_lossy().as_bytes()).map_err(|e| io(&d.join("source.txt"), e))?;
    std::fs::write(d.join("name.txt"), project_name(title, &ext).as_bytes()).map_err(|e| io(&d.join("name.txt"), e))?;
    let mut subs_imported = false;
    let mut warning = None;
    if let Some(src) = subs {
        let text = std::fs::read_to_string(src).map_err(|e| io(src, e))?;
        if crate::subimport::parse(&text, "srt").is_empty() {
            warning = Some(("subs_empty", t!("url-subs-empty", path = src.display().to_string())));
        } else {
            let to = d.join("import_subs.srt");
            std::fs::write(&to, text).map_err(|e| io(&to, e))?;
            subs_imported = true;
        }
    }
    crate::write_initial_project(&d).map_err(|e| UrlError::new("io", e))?;
    Ok(Made { pid, subs_imported, warning })
}

// ─── Маршруты ───────────────────────────────────────────────────────────────

fn status_of(code: &str) -> StatusCode {
    match code {
        "bad_url" | "bad_quality" | "cookies_invalid" => StatusCode::BAD_REQUEST,
        "not_found" => StatusCode::NOT_FOUND,
        "tool_missing" | "ffmpeg_missing" | "busy" | "running" => StatusCode::CONFLICT,
        "network" | "proxy" | "rate_limited" => StatusCode::BAD_GATEWAY,
        "disk_space" => StatusCode::INSUFFICIENT_STORAGE,
        "io" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn refuse(e: UrlError) -> Response {
    (status_of(e.code), Json(e.to_json())).into_response()
}

fn joined<T>(r: Result<Result<T, UrlError>, tokio::task::JoinError>) -> Result<T, UrlError> {
    r.map_err(|e| UrlError::new("io", e.to_string()))?
}

fn body_cookies(body: &Value) -> Option<Cookies> {
    let text = |k: &str| body.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    match (text("cookies"), body.get("cookies_text").and_then(Value::as_str).filter(|s| !s.trim().is_empty())) {
        (Some(path), _) => Some(Cookies::Path(PathBuf::from(path))),
        (None, Some(t)) => Some(Cookies::Text(t.to_string())),
        (None, None) => None,
    }
}

/// GET /url/tool — версия yt-dlp, закреплённая и последняя на GitHub, идёт ли обновление.
pub async fn tool_status(State(st): State<AppState>) -> Response {
    let root = st.repo_root.clone();
    match tokio::task::spawn_blocking(move || ytdlp::status(&root)).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => refuse(UrlError::new("io", e.to_string())),
    }
}

/// POST /url/tool/update — проверить релизы сейчас и поставить новый yt-dlp рядом (в фоне): {started, tool}.
pub async fn tool_update(State(st): State<AppState>) -> Response {
    let root = st.repo_root.clone();
    let res = tokio::task::spawn_blocking(move || {
        let before = ytdlp::status(&root);
        if !before.installed {
            return Err(UrlError::new("tool_missing", t!("url-ytdlp-missing")));
        }
        let started = ytdlp::start_update(&root);
        Ok(json!({ "started": started, "tool": ytdlp::status(&root) }))
    })
    .await;
    match joined(res) {
        Ok(v) => Json(v).into_response(),
        Err(e) => refuse(e),
    }
}

/// Проба ссылки: `yt-dlp -J`, превью картинкой и что предложить (качества, субтитры площадки).
fn probe(repo_root: &Path, fetch_root: &Path, url: &str, cookies: Option<Cookies>) -> Result<Value, UrlError> {
    let url = check_url(url)?;
    let tool = ytdlp::tool(repo_root)?;
    let scratch = fetch_root.join(format!("probe-{}", uuid::Uuid::new_v4().simple()));
    let out = (|| {
        let mut cmd = tool.command(&url, &scratch)?;
        cmd.args(ytdlp::PROBE_ARGS);
        if let Some(c) = &cookies {
            let path = scratch.join("cookies.txt");
            place_cookies(c, &path)?;
            cmd.arg("--cookies").arg(&path);
        }
        cmd.arg("--").arg(&url);
        ytdlp::run_captured(cmd, &|| false, Some(PROBE_TIMEOUT))
    })();
    if scratch.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&scratch) {
            tracing::warn!("{}: {e}", scratch.display());
        }
    }
    let out = out?;
    if !out.success {
        return Err(ytdlp::classify(&out.stderr));
    }
    let info: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| UrlError::new("ytdlp_failed", t!("url-probe-not-json", error = e.to_string(), stderr = ytdlp::last_error_line(&out.stderr))))?;
    let probe = ytdlp::parse_probe(&info)?;
    let mut v = serde_json::to_value(&probe).map_err(|e| UrlError::new("io", e.to_string()))?;
    let (data, problem) = match probe.thumbnail.as_deref().map(ytdlp::thumbnail_data) {
        Some(Ok(d)) => (Some(d), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, None),
    };
    v["thumbnail_data"] = data.into();
    v["thumbnail_error"] = problem.into();
    v["tool_version"] = tool.version.into();
    Ok(v)
}

#[derive(Deserialize)]
pub struct ProbeQuery {
    url: Option<String>,
    cookies: Option<String>,
}

/// GET /url/probe?url=&cookies= — что за видео по ссылке (cookies — путь к cookies.txt).
pub async fn probe_get(State(st): State<AppState>, Query(q): Query<ProbeQuery>) -> Response {
    let cookies = q.cookies.filter(|c| !c.trim().is_empty()).map(|c| Cookies::Path(PathBuf::from(c.trim())));
    run_probe(st, q.url.unwrap_or_default(), cookies).await
}

/// POST /url/probe {url, cookies?, cookies_text?} — то же, cookies.txt содержимым (окно не знает путей файлов).
pub async fn probe_post(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let url = body.get("url").and_then(Value::as_str).unwrap_or_default().to_string();
    run_probe(st, url, body_cookies(&body)).await
}

async fn run_probe(st: AppState, url: String, cookies: Option<Cookies>) -> Response {
    let root = st.repo_root.clone();
    let fetch_root = st.workspace.join(".fetch");
    match joined(tokio::task::spawn_blocking(move || probe(&root, &fetch_root, &url, cookies)).await) {
        Ok(v) => Json(v).into_response(),
        Err(e) => refuse(e),
    }
}

/// POST /projects/from_url {url, quality, subs_lang?, cookies?, cookies_text?} — начать загрузку в новый проект: {fetch}.
/// Идёт в фоне мимо очереди джоб; готовый проект — fetch.pid в GET /url/fetches/{id}.
pub async fn create_from_url(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let quality_text = body.get("quality").and_then(Value::as_str).unwrap_or("best");
    let Some(quality) = Quality::parse(quality_text) else {
        return refuse(UrlError::new("bad_quality", t!("url-bad-quality", quality = quality_text.to_string())));
    };
    let req = FetchRequest {
        url: body.get("url").and_then(Value::as_str).unwrap_or_default().to_string(),
        quality,
        subs_lang: body.get("subs_lang").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        cookies: body_cookies(&body),
    };
    let fetches = st.fetches.clone();
    match joined(tokio::task::spawn_blocking(move || fetches.start(req)).await) {
        Ok(f) => Json(json!({ "fetch": f })).into_response(),
        Err(e) => refuse(e),
    }
}

/// GET /url/fetches — загрузки по ссылке, новые первыми.
pub async fn fetches_list(State(st): State<AppState>) -> Json<Value> {
    Json(json!({ "fetches": st.fetches.list() }))
}

/// GET /url/fetches/{id} — одна загрузка.
pub async fn fetch_get(State(st): State<AppState>, AxPath(id): AxPath<String>) -> Response {
    match st.fetches.get(&id) {
        Some(f) => Json(f).into_response(),
        None => refuse(UrlError::new("not_found", t!("url-no-fetch", id = id.to_string()))),
    }
}

/// POST /url/fetches/{id}/cancel — остановить загрузку (недокачанное удаляется).
pub async fn fetch_cancel(State(st): State<AppState>, AxPath(id): AxPath<String>) -> Response {
    match st.fetches.cancel(&id) {
        Ok(f) => Json(json!({ "fetch": f })).into_response(),
        Err(e) => refuse(e),
    }
}

/// POST /url/fetches/{id}/resume — продолжить прерванную или упавшую загрузку с места.
pub async fn fetch_resume(State(st): State<AppState>, AxPath(id): AxPath<String>) -> Response {
    let fetches = st.fetches.clone();
    match joined(tokio::task::spawn_blocking(move || fetches.resume(&id)).await) {
        Ok(f) => Json(json!({ "fetch": f })).into_response(),
        Err(e) => refuse(e),
    }
}

/// DELETE /url/fetches/{id} — убрать загрузку из списка вместе с недокачанным.
pub async fn fetch_forget(State(st): State<AppState>, AxPath(id): AxPath<String>) -> Response {
    let fetches = st.fetches.clone();
    match joined(tokio::task::spawn_blocking(move || fetches.forget(&id).map(|_| ())).await) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => refuse(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetch(id: &str, status: FetchStatus) -> Fetch {
        Fetch {
            id: id.into(),
            url: "https://example.com/v".into(),
            quality: Quality::H720,
            subs_lang: Some("en".into()),
            cookies: false,
            status,
            phase: "download".into(),
            title: Some("t".into()),
            duration: None,
            downloaded: 5,
            total: Some(10),
            speed_bps: 7,
            eta_s: Some(3),
            pid: None,
            subs_imported: false,
            warning: None,
            warning_detail: None,
            error_code: None,
            error: None,
            hint: None,
            tool_version: None,
            started_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn a_download_running_at_exit_comes_back_interrupted_and_resumable() {
        let root = tempfile::tempdir().unwrap();
        let ws = root.path().join("workspace");
        persist(&ws.join(".fetch").join("fetches.json"), &[fetch("url1", FetchStatus::Downloading), fetch("url2", FetchStatus::Completed)]).unwrap();
        let f = Fetches::open(root.path(), &ws);
        let list = f.list();
        assert_eq!(list[0].status, FetchStatus::Interrupted);
        assert_eq!((list[0].error_code.as_deref(), list[0].speed_bps, list[0].downloaded), (Some("interrupted"), 0, 5), "what came is kept");
        assert_eq!(list[1].status, FetchStatus::Completed);
        let again = Fetches::open(root.path(), &ws);
        assert_eq!(again.get("url1").unwrap().status, FetchStatus::Interrupted, "the state is on disk");
        assert_eq!(f.resume("url1").unwrap_err().code, "tool_missing", "resuming needs yt-dlp");
        std::fs::create_dir_all(ws.join(".fetch").join("url1")).unwrap();
        f.forget("url1").unwrap();
        assert!(f.get("url1").is_none() && !ws.join(".fetch").join("url1").exists(), "forgetting drops the partial download");
        assert_eq!(f.forget("nope").unwrap_err().code, "not_found");
    }

    #[test]
    fn a_running_download_is_cancelled_and_not_forgotten_while_it_runs() {
        let root = tempfile::tempdir().unwrap();
        let ws = root.path().join("workspace");
        let f = Fetches::open(root.path(), &ws);
        lock(&f.inner.list).push(fetch("url9", FetchStatus::Downloading));
        let stop = Arc::new(AtomicBool::new(false));
        lock(&f.inner.stops).insert("url9".into(), stop.clone());
        assert_eq!(f.forget("url9").unwrap_err().code, "running");
        let c = f.cancel("url9").unwrap();
        assert_eq!((c.status, c.speed_bps), (FetchStatus::Cancelled, 0));
        assert!(stop.load(Ordering::SeqCst), "the process gets the stop");
        f.update("url9", |x| x.downloaded = 99);
        assert_eq!(f.get("url9").unwrap().downloaded, 5, "a stopped download takes no more progress");
        f.finish("url9", Err(UrlError::new("cancelled", t!("url-stopped"))), true);
        assert_eq!(f.get("url9").unwrap().status, FetchStatus::Cancelled);
        assert_eq!(f.resume("url9").unwrap_err().code, "tool_missing");
    }

    #[test]
    fn a_failure_keeps_its_code_and_what_to_do() {
        let root = tempfile::tempdir().unwrap();
        let f = Fetches::open(root.path(), &root.path().join("workspace"));
        lock(&f.inner.list).push(fetch("url3", FetchStatus::Downloading));
        f.finish("url3", Err(UrlError::new("geo_blocked", "not available in your country")), false);
        let x = f.get("url3").unwrap();
        assert_eq!((x.status, x.error_code.as_deref()), (FetchStatus::Failed, Some("geo_blocked")));
        assert!(x.hint.unwrap().contains("proxy"));
    }

    #[test]
    fn links_languages_and_names_are_checked() {
        assert!(check_url("https://www.youtube.com/watch?v=abc").is_ok());
        assert_eq!(check_url("ftp://x/y").unwrap_err().code, "bad_url");
        assert_eq!(check_url("C:\\video.mp4").unwrap_err().code, "bad_url");
        assert_eq!(check_url("").unwrap_err().code, "bad_url");
        assert_eq!(check_lang("pt-BR").unwrap(), "pt-BR");
        assert!(check_lang("en,ru").is_err(), "one language, not a yt-dlp list");
        assert!(check_lang(" ").is_err() && check_lang("en/../x").is_err());
        assert_eq!(project_name("Клип: «часть 1/2»?", "mp4"), "Клип «часть 1 2».mp4");
        assert_eq!(project_name("  ...  ", "m4a"), "video.m4a");
        assert_eq!(project_name(&"x".repeat(300), "mp4").chars().count(), 124);
    }

    #[test]
    fn cookies_are_copied_and_bad_ones_refused() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("c.txt");
        std::fs::write(&src, "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\tA\tB\n").unwrap();
        let dest = root.path().join("fetch").join("cookies.txt");
        place_cookies(&Cookies::Path(src.clone()), &dest).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), std::fs::read_to_string(&src).unwrap());
        assert_eq!(place_cookies(&Cookies::Path(root.path().join("none.txt")), &dest).unwrap_err().code, "cookies_invalid");
        assert_eq!(place_cookies(&Cookies::Text("  ".into()), &dest).unwrap_err().code, "cookies_invalid");
        place_cookies(&Cookies::Text("# Netscape HTTP Cookie File\n".into()), &dest).unwrap();
    }

    #[test]
    fn a_downloaded_file_becomes_a_project_with_the_sites_subtitles() {
        let root = tempfile::tempdir().unwrap();
        let fetch_root = root.path().join("workspace").join(".fetch");
        let dir = fetch_root.join("url5");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.mp4"), b"not really a video").unwrap();
        std::fs::write(dir.join("subs.en.srt"), "1\n00:00:01,000 --> 00:00:02,500\nHello there\n\n").unwrap();
        assert_eq!(find_media(&dir), Some(dir.join("source.mp4")));
        let subs = find_subs(&dir).unwrap();
        let made = make_project(&fetch_root, &dir.join("source.mp4"), "My: clip", Some(&subs)).unwrap();
        let p = root.path().join("workspace").join(&made.pid);
        assert!(made.subs_imported && made.warning.is_none());
        assert!(p.join("source.mp4").is_file() && !dir.join("source.mp4").exists(), "the file moves, not copies");
        assert_eq!(std::fs::read_to_string(p.join("name.txt")).unwrap(), "My clip.mp4");
        assert!(p.join("import_subs.srt").is_file());
        let project: Value = serde_json::from_str(&std::fs::read_to_string(p.join("project.json")).unwrap()).unwrap();
        assert_eq!(project["segments"][0]["src_text"], "Hello there");
        std::fs::write(dir.join("source.m4a"), b"x").unwrap();
        std::fs::write(dir.join("empty.srt"), "garbage").unwrap();
        let made = make_project(&fetch_root, &dir.join("source.m4a"), "a", Some(&dir.join("empty.srt"))).unwrap();
        assert_eq!((made.subs_imported, made.warning.map(|w| w.0)), (false, Some("subs_empty")));
    }

    fn downloaded(f: &Fetches, id: &str) -> PathBuf {
        lock(&f.inner.list).push(fetch(id, FetchStatus::Downloading));
        let dir = f.dir(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.mp4"), b"not really a video").unwrap();
        dir.join("source.mp4")
    }

    fn ran(success: bool, stderr: &str) -> ytdlp::Captured {
        ytdlp::Captured { success, code: Some(i32::from(!success)), stdout: Vec::new(), stderr: stderr.into() }
    }

    #[test]
    fn a_failed_subtitles_step_completes_the_download_without_them() {
        let root = tempfile::tempdir().unwrap();
        let ws = root.path().join("workspace");
        let f = Fetches::open(root.path(), &ws);
        let stop = Arc::new(AtomicBool::new(false));
        let media = downloaded(&f, "url7");
        let mut asked = None;
        let res = f.after_media("url7", &media, "clip", Some("en"), &stop, |lang| {
            asked = Some(lang.to_string());
            Ok(ran(false, "[info] Writing video subtitles to: source.en.vtt\nERROR: Unable to download video subtitles for 'en': HTTP Error 429: Too Many Requests"))
        });
        f.finish("url7", res, false);
        let x = f.get("url7").unwrap();
        assert_eq!(asked.as_deref(), Some("en"));
        assert_eq!((x.status, x.error_code.as_deref(), x.warning.as_deref()), (FetchStatus::Completed, None, Some("subs_failed")));
        assert!(x.warning_detail.as_deref().unwrap().contains("rate_limited"), "{:?}", x.warning_detail);
        assert!(!x.subs_imported);
        let p = ws.join(x.pid.unwrap());
        assert!(p.join("source.mp4").is_file() && p.join("project.json").is_file() && !p.join("import_subs.srt").exists());
        assert!(!f.dir("url7").exists(), "the download's folder goes once the project has the file");

        let media = downloaded(&f, "url8");
        let res = f.after_media("url8", &media, "clip", Some("en"), &stop, |_| Err(UrlError::new("network", "yt-dlp не ответил за 180 с")));
        f.finish("url8", res, false);
        assert_eq!((f.get("url8").unwrap().status, f.get("url8").unwrap().warning.as_deref()), (FetchStatus::Completed, Some("subs_failed")));

        let media = downloaded(&f, "url9");
        let res = f.after_media("url9", &media, "clip", Some("en"), &stop, |_| Ok(ran(true, "")));
        f.finish("url9", res, false);
        let x = f.get("url9").unwrap();
        assert_eq!((x.status, x.warning.as_deref()), (FetchStatus::Completed, Some("subs_failed")), "a step without its file is a warning too");
    }

    #[test]
    fn the_sites_subtitles_come_after_the_video_and_a_stop_stays_a_stop() {
        let root = tempfile::tempdir().unwrap();
        let ws = root.path().join("workspace");
        let f = Fetches::open(root.path(), &ws);
        let stop = Arc::new(AtomicBool::new(false));
        let media = downloaded(&f, "url4");
        let dir = f.dir("url4");
        let res = f.after_media("url4", &media, "clip", Some("en"), &stop, |_| {
            assert!(dir.join("source.mp4").is_file(), "the subtitles are fetched once the video is on disk");
            std::fs::write(dir.join("subs.en.srt"), "1\n00:00:01,000 --> 00:00:02,500\nHello there\n\n").unwrap();
            Ok(ran(true, ""))
        });
        f.finish("url4", res, false);
        let x = f.get("url4").unwrap();
        assert_eq!((x.status, x.subs_imported, x.warning.as_deref()), (FetchStatus::Completed, true, None));
        assert!(ws.join(x.pid.unwrap()).join("import_subs.srt").is_file());

        let media = downloaded(&f, "url5");
        let res = f.after_media("url5", &media, "clip", None, &stop, |_| panic!("no subtitles were asked for"));
        f.finish("url5", res, false);
        assert_eq!(f.get("url5").unwrap().status, FetchStatus::Completed);

        let media = downloaded(&f, "url6");
        let res = f.after_media("url6", &media, "clip", Some("en"), &stop, |_| Err(UrlError::new("cancelled", t!("url-stopped"))));
        assert_eq!(res.as_ref().unwrap_err().code, "cancelled");
        f.finish("url6", res, false);
        let x = f.get("url6").unwrap();
        assert_eq!((x.status, x.pid.as_deref()), (FetchStatus::Cancelled, None));
    }

    #[test]
    fn only_the_final_file_is_taken_for_the_project() {
        let root = tempfile::tempdir().unwrap();
        for name in ["source.mp4.part", "source.f137.mp4", "source.f140.m4a", "info.json", "cookies.txt"] {
            std::fs::write(root.path().join(name), b"x").unwrap();
        }
        assert_eq!(find_media(root.path()), None, "parts of a merge are not the video");
        std::fs::write(root.path().join("source.webm"), b"x").unwrap();
        assert_eq!(find_media(root.path()), Some(root.path().join("source.webm")));
    }

    #[test]
    fn eviction_keeps_running_downloads() {
        let root = tempfile::tempdir().unwrap();
        let f = Fetches::open(root.path(), &root.path().join("workspace"));
        let mut list: Vec<Fetch> = (0..KEEP + 3).map(|i| fetch(&format!("url{i}"), if i % 2 == 0 { FetchStatus::Downloading } else { FetchStatus::Completed })).collect();
        f.evict(&mut list);
        assert!(list.len() <= KEEP || list.iter().all(|x| x.status == FetchStatus::Downloading));
        assert_eq!(list.iter().filter(|x| x.status == FetchStatus::Downloading).count(), (KEEP + 3).div_ceil(2));
    }

    #[test]
    fn errors_answer_with_the_right_status() {
        assert_eq!(status_of("bad_url"), StatusCode::BAD_REQUEST);
        assert_eq!(status_of("tool_missing"), StatusCode::CONFLICT);
        assert_eq!(status_of("geo_blocked"), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status_of("network"), StatusCode::BAD_GATEWAY);
        let v = UrlError::new("login_required", "Sign in").to_json();
        assert_eq!((v["error"].as_str(), v["detail"].as_str()), (Some("login_required"), Some("Sign in")));
        assert!(v["hint"].as_str().unwrap().contains("cookies.txt"));
    }
}
