//! Персистентная запись о последней джобе проекта: `workspace/<pid>/job.json`. Пишется ДО постановки в
//! очередь (джоба, которую нельзя сохранить, не запускается), стадия обновляется по ходу, терминал —
//! в конце. После рестарта незавершённая запись становится `interrupted`; POST /projects/{pid}/resume
//! ставит тот же вид с теми же аргументами на тот же проект (его кэши стадий и озвучки переиспользуются).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const FILE: &str = "job.json";

pub const STATE_QUEUED: &str = "queued";
pub const STATE_RUNNING: &str = "running";
pub const STATE_FAILED: &str = "failed";
pub const STATE_INTERRUPTED: &str = "interrupted";
pub const STATE_DONE: &str = "done";
pub const STATE_CANCELLED: &str = "cancelled";

/// Код ошибки для UI (текст по коду переводит фронт) + исходный текст для «Подробностей».
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JobError {
    pub code: String,
    pub text: String,
}

impl JobError {
    /// Код по сообщению тела джобы. Префикс ENGINE_STUCK ставит render::voice_clone_guarded.
    pub fn from_message(msg: &str) -> Self {
        let code = if msg.starts_with("ENGINE_STUCK") { "engine_stuck" } else { "failed" };
        JobError { code: code.into(), text: msg.to_string() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobRecord {
    pub kind: String,
    #[serde(default)]
    pub args: Value,
    pub state: String,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub error: Option<JobError>,
    #[serde(default)]
    pub job_id: String,
    /// Сколько раз джобу продолжали после сбоя/прерывания.
    #[serde(default)]
    pub resumes: u32,
    pub started_at: u64,
    pub updated_at: u64,
}

impl JobRecord {
    /// Можно ли продолжить: джоба не дошла до конца.
    pub fn resumable(&self) -> bool {
        matches!(self.state.as_str(), STATE_FAILED | STATE_INTERRUPTED | STATE_CANCELLED)
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Прочитать job.json. Нет файла — Ok(None); битый файл — ошибка с причиной.
pub fn read(dir: &Path) -> Result<Option<JobRecord>, String> {
    let p = dir.join(FILE);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("чтение {}: {e}", p.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("разбор {}: {e}", p.display()))
}

fn write(dir: &Path, rec: &JobRecord) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(rec).map_err(|e| format!("сериализация job.json: {e}"))?;
    dub_core::atomic::write(&dir.join(FILE), &body)
}

/// Новая запись перед постановкой в очередь. Счётчик продолжений переносится с прошлой записи того
/// же вида, если она была незавершённой.
pub fn write_queued(dir: &Path, kind: &str, args: &Value, job_id: &str) -> Result<(), String> {
    let _held = record_writes();
    let now = now_secs();
    let resumes = match read(dir) {
        Ok(Some(prev)) if prev.kind == kind && prev.resumable() => prev.resumes + 1,
        _ => 0,
    };
    write(
        dir,
        &JobRecord {
            kind: kind.to_string(),
            args: args.clone(),
            state: STATE_QUEUED.into(),
            stage: String::new(),
            error: None,
            job_id: job_id.to_string(),
            resumes,
            started_at: now,
            updated_at: now,
        },
    )
}

/// Держится от чтения job.json до записи обратно: постановка следующей джобы проекта не вклинивается
/// между проверкой хозяина записи и её обновлением.
fn record_writes() -> std::sync::MutexGuard<'static, ()> {
    static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Read-modify-write записи. Нет записи — ошибка (обновлять нечего).
pub fn update(dir: &Path, f: impl FnOnce(&mut JobRecord)) -> Result<(), String> {
    let _held = record_writes();
    let mut rec = read(dir)?.ok_or_else(|| format!("{} нет в {}", FILE, dir.display()))?;
    f(&mut rec);
    rec.updated_at = now_secs();
    write(dir, &rec)
}

/// Обновить запись джобы `job_id`. Если job.json уже принадлежит более новой джобе проекта (другой
/// класс поставлен следом), запись не трогаем: job.json — всегда последняя джоба проекта.
pub fn update_owned(dir: &Path, job_id: &str, f: impl FnOnce(&mut JobRecord)) -> Result<(), String> {
    let _held = record_writes();
    match read(dir)? {
        Some(mut rec) if rec.job_id == job_id => {
            f(&mut rec);
            rec.updated_at = now_secs();
            write(dir, &rec)
        }
        Some(_) => Ok(()),
        None => Err(format!("{} нет в {}", FILE, dir.display())),
    }
}

/// При старте сервиса: незавершённые (queued/running) записи всех проектов -> interrupted.
pub fn recover(workspace: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(workspace) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        match read(&dir) {
            Ok(Some(rec)) if rec.state == STATE_QUEUED || rec.state == STATE_RUNNING => {
                let res = update(&dir, |r| {
                    r.state = STATE_INTERRUPTED.into();
                    r.error = Some(JobError {
                        code: "interrupted".into(),
                        text: String::new(),
                    });
                });
                match res {
                    Ok(()) => n += 1,
                    Err(e) => eprintln!("[jobs] recover {}: {e}", dir.display()),
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("[jobs] recover {}: {e}", dir.display()),
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dub_jobstore_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn running_becomes_interrupted_on_startup() {
        let ws = tmp_dir("recover");
        let running = ws.join("aaa");
        let done = ws.join("bbb");
        let queued = ws.join("ccc");
        for d in [&running, &done, &queued] {
            std::fs::create_dir_all(d).unwrap();
        }
        write_queued(&running, "render", &json!({}), "j1").unwrap();
        update(&running, |r| {
            r.state = STATE_RUNNING.into();
            r.stage = "tts".into();
        })
        .unwrap();
        write_queued(&done, "analyze", &json!({"tgt_lang":"ru"}), "j2").unwrap();
        update(&done, |r| r.state = STATE_DONE.into()).unwrap();
        write_queued(&queued, "remix", &json!({"instruction":"x"}), "j3").unwrap();

        assert_eq!(recover(&ws), 2);
        let r = read(&running).unwrap().unwrap();
        assert_eq!(r.state, STATE_INTERRUPTED);
        assert_eq!(r.stage, "tts");
        assert_eq!(r.error.unwrap().code, "interrupted");
        assert!(read(&running).unwrap().map(|r| r.resumable()).unwrap());
        assert_eq!(read(&done).unwrap().unwrap().state, STATE_DONE);
        assert_eq!(read(&queued).unwrap().unwrap().state, STATE_INTERRUPTED);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn interrupted_analysis_preserves_speaker_count_when_requeued() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("project");
        std::fs::create_dir(&dir).unwrap();
        let args = json!({ "speaker_count": "8", "mode": "transcribe", "src_lang": "ru", "tgt_lang": "ru" });
        write_queued(&dir, "analyze", &args, "first").unwrap();
        update(&dir, |r| r.state = STATE_RUNNING.into()).unwrap();
        assert_eq!(recover(root.path()), 1);
        let interrupted = read(&dir).unwrap().unwrap();
        assert!(interrupted.resumable());
        assert_eq!(interrupted.args, args);
        write_queued(&dir, &interrupted.kind, &interrupted.args, "resumed").unwrap();
        let resumed = read(&dir).unwrap().unwrap();
        assert_eq!(resumed.kind, "analyze");
        assert_eq!(resumed.args["speaker_count"], "8");
        assert_eq!(resumed.args, args);
        assert_eq!(resumed.resumes, 1);
    }

    #[test]
    fn resume_counter_carries_over_same_kind() {
        let d = tmp_dir("resumes");
        write_queued(&d, "render", &json!({}), "j1").unwrap();
        update(&d, |r| r.state = STATE_FAILED.into()).unwrap();
        write_queued(&d, "render", &json!({}), "j2").unwrap();
        assert_eq!(read(&d).unwrap().unwrap().resumes, 1);
        write_queued(&d, "remix", &json!({}), "j3").unwrap();
        assert_eq!(read(&d).unwrap().unwrap().resumes, 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn older_job_does_not_overwrite_newer_record() {
        let d = tmp_dir("owned");
        write_queued(&d, "analyze", &json!({}), "old").unwrap();
        write_queued(&d, "dub_audio", &json!({}), "new").unwrap();
        update_owned(&d, "old", |r| r.state = STATE_DONE.into()).unwrap();
        let r = read(&d).unwrap().unwrap();
        assert_eq!((r.kind.as_str(), r.state.as_str()), ("dub_audio", STATE_QUEUED));
        update_owned(&d, "new", |r| r.state = STATE_RUNNING.into()).unwrap();
        assert_eq!(read(&d).unwrap().unwrap().state, STATE_RUNNING);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn broken_file_is_an_error_not_none() {
        let d = tmp_dir("broken");
        std::fs::write(d.join(FILE), b"{not json").unwrap();
        assert!(read(&d).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
