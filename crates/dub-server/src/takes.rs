//! История дублей фразы: до `MAX_TAKES` последних озвучек реплики в `takes/<sid>/<n>.wav` и
//! `takes/<sid>/takes.json`. Активный дубль — тот, что лежит в `seg_<sid>.wav` и чей ключ синтеза записан в
//! seg_ckpt.json: его берёт микс, выбор другого дубля меняет только пересборку микса. Закреплённый дубль
//! рендер не заменяет, пока текст реплики тот, что в нём озвучен.

use std::path::{Path, PathBuf};

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AppState;

pub const DIR: &str = "takes";
pub const FILE: &str = "takes.json";
pub const MAX_TAKES: usize = 5;

/// Одна озвучка реплики.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub n: u32,
    /// Озвученный текст.
    pub text: String,
    /// Ключ синтеза (render::seg_key): совпал с ключом реплики — дубль годен без нового синтеза.
    pub key: String,
    /// Нонс «перегенерировать» реплики на момент синтеза (Segment.extra.regen).
    #[serde(default)]
    pub nonce: Option<Value>,
    /// Голос: clone, имя голоса из библиотеки или облачный голос.
    pub voice: String,
    /// Файл референса клона.
    pub reference: String,
    /// Параметры синтеза.
    pub params: String,
    /// Откуда дубль: synth, multitake, qc, shorten.
    pub source: String,
    /// Длительность клипа, сек.
    pub dur: f64,
    /// Сходство услышанного ASR с текстом (QC), если проверялось.
    #[serde(default)]
    pub qc: Option<f64>,
    /// Время создания, секунды эпохи.
    pub created: u64,
}

/// Что известно о новой озвучке, кроме её файла.
pub struct NewTake {
    pub text: String,
    pub key: String,
    pub nonce: Option<Value>,
    pub voice: String,
    pub reference: String,
    pub params: String,
    pub source: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    #[serde(default)]
    pub next: u32,
    #[serde(default)]
    pub active: Option<u32>,
    #[serde(default)]
    pub pinned: Option<u32>,
    /// Дубль, выбранный из истории (take_select): звучит, пока текст и нонс реплики те, с которыми он озвучен,
    /// хотя ключ синтеза с тех пор мог смениться (голос, референс, опции). Новый дубль снимает выбор.
    #[serde(default)]
    pub selected: Option<u32>,
    #[serde(default)]
    pub takes: Vec<Take>,
}

fn dir_of(wd: &Path, sid: &str) -> PathBuf {
    wd.join(DIR).join(sid)
}

pub fn wav(wd: &Path, sid: &str, n: u32) -> PathBuf {
    dir_of(wd, sid).join(format!("{n}.wav"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Держится от чтения истории фразы до её записи: рендер озвучивает фразу, пока окно и агент выбирают и
/// закрепляют её дубли. Не реентерабелен.
fn writes() -> std::sync::MutexGuard<'static, ()> {
    static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl History {
    /// Правка истории по её свежей копии с диска под замком историй: копия, прочитанная раньше, затёрла бы
    /// то, что за это время записали другие. Возвращает историю после правки и ответ правки.
    pub fn update<T>(wd: &Path, sid: &str, edit: impl FnOnce(&mut History) -> Result<T, String>) -> Result<(History, T), String> {
        let _held = writes();
        let mut h = History::load(wd, sid)?;
        let out = edit(&mut h)?;
        Ok((h, out))
    }

    /// Нет файла — пустая история; битый — ошибка с причиной.
    pub fn load(wd: &Path, sid: &str) -> Result<Self, String> {
        let p = dir_of(wd, sid).join(FILE);
        match std::fs::read_to_string(&p) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| t!("common-parse", path = p.display().to_string(), error = e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(History::default()),
            Err(e) => Err(t!("common-read", path = p.display().to_string(), error = e.to_string())),
        }
    }

    pub fn save(&self, wd: &Path, sid: &str) -> Result<(), String> {
        let d = dir_of(wd, sid);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        dub_core::atomic::write(&d.join(FILE), &body)
    }

    pub fn get(&self, n: u32) -> Option<&Take> {
        self.takes.iter().find(|t| t.n == n)
    }

    pub fn pinned_take(&self) -> Option<&Take> {
        self.pinned.and_then(|n| self.get(n))
    }

    /// Закреплённый дубль, который звучит вместо новой озвучки реплики с текстом `text`.
    pub fn pinned_for(&self, text: &str) -> Option<&Take> {
        self.pinned_take().filter(|p| p.text == text.trim())
    }

    /// Выбранный дубль, который звучит у реплики с текстом `text` и нонсом «перегенерировать» `nonce`.
    pub fn selected_for(&self, text: &str, nonce: Option<&Value>) -> Option<&Take> {
        self.selected.and_then(|n| self.get(n)).filter(|t| t.text == text.trim() && t.nonce.as_ref() == nonce)
    }

    /// Дубль с этим ключом синтеза: активный, если это он (альтернативы multi-take делят один ключ), иначе
    /// самый свежий.
    pub fn by_key(&self, key: &str) -> Option<&Take> {
        self.active.and_then(|n| self.get(n)).filter(|t| t.key == key).or_else(|| self.takes.iter().rev().find(|t| t.key == key))
    }

    /// Реплике с ключом `key`, текстом `text` и нонсом `nonce` синтез не нужен: звучит закреплённый дубль
    /// этого текста, выбранный дубль этого текста и нонса или дубль с этим ключом уже есть.
    pub fn covers(&self, key: &str, text: &str, nonce: Option<&Value>) -> bool {
        self.pinned_for(text).is_some() || self.selected_for(text, nonce).is_some() || self.by_key(key).is_some()
    }

    /// Убрать битую историю фразы в сторону (`takes/<sid>.broken-<время>`) вместе с её клипами: новые дубли не
    /// затирают старые файлы. Возвращает, куда она ушла.
    pub fn quarantine(wd: &Path, sid: &str) -> Result<PathBuf, String> {
        let from = dir_of(wd, sid);
        let to = wd.join(DIR).join(format!("{sid}.broken-{}", now_secs()));
        std::fs::rename(&from, &to).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))?;
        Ok(to)
    }

    /// Положить клип `src` новым дублем, сделать его активным и сохранить историю. Сверх `MAX_TAKES`
    /// удаляются самые старые дубли, кроме активного и закреплённого.
    pub fn add(&mut self, wd: &Path, sid: &str, src: &Path, t: NewTake, dur: f64) -> Result<u32, String> {
        let n = self.next.max(self.takes.iter().map(|t| t.n + 1).max().unwrap_or(0));
        let d = dir_of(wd, sid);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        dub_core::atomic::copy(src, &wav(wd, sid, n))?;
        self.takes.push(Take {
            n,
            text: t.text,
            key: t.key,
            nonce: t.nonce,
            voice: t.voice,
            reference: t.reference,
            params: t.params,
            source: t.source.to_string(),
            dur,
            qc: None,
            created: now_secs(),
        });
        self.next = n + 1;
        self.active = Some(n);
        self.selected = None;
        self.prune(wd, sid)?;
        self.save(wd, sid)?;
        Ok(n)
    }

    fn prune(&mut self, wd: &Path, sid: &str) -> Result<(), String> {
        while self.takes.len() > MAX_TAKES {
            let keep = |t: &Take| Some(t.n) == self.active || Some(t.n) == self.pinned || Some(t.n) == self.selected;
            let Some(pos) = self.takes.iter().enumerate().filter(|(_, t)| !keep(t)).min_by_key(|(_, t)| t.n).map(|(i, _)| i) else {
                break;
            };
            let gone = self.takes.remove(pos);
            let f = wav(wd, sid, gone.n);
            match std::fs::remove_file(&f) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(t!("takes-delete-old", path = f.display().to_string(), error = e.to_string())),
            }
        }
        Ok(())
    }

    /// Положить дубль `n` в файл сегмента и сделать активным (история сохраняется).
    pub fn restore(&mut self, wd: &Path, sid: &str, n: u32, seg_wav: &Path) -> Result<(), String> {
        if self.get(n).is_none() {
            return Err(t!("takes-missing", take = n, line = sid.to_string()));
        }
        dub_core::atomic::copy(&wav(wd, sid, n), seg_wav)?;
        if self.active != Some(n) {
            self.active = Some(n);
            self.save(wd, sid)?;
        }
        Ok(())
    }

    pub fn set_qc(&mut self, wd: &Path, sid: &str, n: u32, sim: f64) -> Result<(), String> {
        match self.takes.iter_mut().find(|t| t.n == n) {
            Some(t) => {
                t.qc = Some(sim);
                self.save(wd, sid)
            }
            None => Ok(()),
        }
    }

    /// Сводка для строки редактора и агента.
    pub fn summary(&self) -> Value {
        json!({ "count": self.takes.len(), "active": self.active, "pinned": self.pinned, "selected": self.selected })
    }
}

/// Снять закрепление, если текст реплики теперь не тот, что озвучен в закреплённом дубле. true — снято.
pub fn unpin_if_stale(wd: &Path, sid: &str, text: &str) -> Result<bool, String> {
    let _held = writes();
    let mut h = History::load(wd, sid)?;
    match h.pinned_take() {
        Some(_) if h.pinned_for(text).is_none() => {
            h.pinned = None;
            h.save(wd, sid)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Сводка истории фразы по её takes.json, прочитанная, пока файл не менялся: ответ проекта строится на каждую
/// правку, а историй на длинном проекте сотни.
fn cached_summary(wd: &Path, sid: &str) -> Value {
    type Cache = std::sync::Mutex<std::collections::HashMap<PathBuf, (std::time::SystemTime, Value)>>;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let file = dir_of(wd, sid).join(FILE);
    let stamp = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(stamp) = stamp {
        if let Some((at, v)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&file) {
            if *at == stamp {
                return v.clone();
            }
        }
    }
    // Битая история — ошибка у своей строки, а не у всего проекта.
    let v = match History::load(wd, sid) {
        Ok(h) if h.takes.is_empty() => Value::Null,
        Ok(h) => h.summary(),
        Err(e) => json!({ "error": e }),
    };
    if let Some(stamp) = stamp {
        cache.lock().unwrap_or_else(|e| e.into_inner()).insert(file, (stamp, v.clone()));
    }
    v
}

/// Сводки историй всех фраз проекта: sid -> {count, active, pinned, selected} или {error} у битой истории.
pub fn summaries(wd: &Path) -> Result<std::collections::HashMap<String, Value>, String> {
    let root = wd.join(DIR);
    let mut out = std::collections::HashMap::new();
    let rd = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("{}: {e}", root.display())),
    };
    for e in rd {
        let e = e.map_err(|e| format!("{}: {e}", root.display()))?;
        if !e.path().is_dir() {
            continue;
        }
        let sid = e.file_name().to_string_lossy().into_owned();
        if sid.contains(".broken-") {
            continue;
        }
        let v = cached_summary(wd, &sid);
        if !v.is_null() {
            out.insert(sid, v);
        }
    }
    Ok(out)
}

/// Убрать истории фраз, которых в проекте больше нет (`live` — имена файлов его реплик, скрытые тоже).
/// Возвращает, сколько убрано.
pub fn drop_orphans(wd: &Path, live: &std::collections::HashSet<String>) -> Result<usize, String> {
    let root = wd.join(DIR);
    let rd = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", root.display())),
    };
    let mut gone = 0;
    for e in rd {
        let e = e.map_err(|e| format!("{}: {e}", root.display()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let sid = name.split(".broken-").next().unwrap_or_default();
        if e.path().is_dir() && !live.contains(sid) {
            std::fs::remove_dir_all(e.path()).map_err(|err| format!("{}: {err}", e.path().display()))?;
            gone += 1;
        }
    }
    Ok(gone)
}

/// Убрать все истории дублей проекта: новый анализ заменил его реплики.
pub fn drop_all(wd: &Path) -> Result<(), String> {
    let root = wd.join(DIR);
    match std::fs::remove_dir_all(&root) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", root.display())),
    }
}

// ---------------------------------------------------------------- правки проекта (PATCH take_select / take_pin)

pub fn is_take_op(op: &str) -> bool {
    matches!(op, "take_select" | "take_pin")
}

fn seg_sid(proj: &dub_core::Project, id: &str) -> Result<String, (u16, String)> {
    if !proj.segments.iter().any(|s| s.id == id) {
        return Err((404, format!("segment {id:?} not found")));
    }
    crate::render::seg_file_id(id).ok_or((400, format!("segment id {id:?} has no file name")))
}

fn edit_id(edit: &Value) -> Result<String, (u16, String)> {
    edit.get("id").and_then(Value::as_str).map(str::to_string).ok_or((400, "missing segment id".into()))
}

/// Дополнить правку take_select тем, что знает история: текст, нонс и ключ выбранного дубля (их ставит
/// patch::apply в реплику).
pub fn resolve(wd: &Path, proj: &dub_core::Project, edit: &Value) -> Result<Value, (u16, String)> {
    let op = edit.get("op").and_then(Value::as_str).unwrap_or_default();
    let id = edit_id(edit)?;
    let sid = seg_sid(proj, &id)?;
    let h = History::load(wd, &sid).map_err(|e| (500, e))?;
    let mut out = edit.clone();
    match op {
        "take_select" => {
            let n = edit.get("take").and_then(Value::as_u64).ok_or((400, "take_select needs take (its n from takes_list)".to_string()))?;
            let t = u32::try_from(n).ok().and_then(|n| h.get(n)).ok_or((404, format!("take {n} of segment {id:?} not found")))?;
            // Пока дубль закреплён, рендер держит в файле сегмента его.
            if let Some(p) = h.pinned.filter(|&p| p != t.n) {
                return Err((409, format!("take {p} of segment {id:?} is pinned: unpin it (take_pin pinned false) before selecting another take")));
            }
            out["take_text"] = t.text.clone().into();
            out["take_nonce"] = t.nonce.clone().unwrap_or(Value::Null);
            out["take_key"] = t.key.clone().into();
        }
        "take_pin" => {
            let pin = edit.get("pinned").and_then(Value::as_bool).ok_or((400, "take_pin needs pinned (true or false)".to_string()))?;
            if pin {
                let active = h.active.and_then(|n| h.get(n)).ok_or((409, format!("segment {id:?} has no voiced take to pin")))?;
                let seg = proj.segments.iter().find(|s| s.id == id).expect("seg_sid checked it");
                if active.text != seg.tgt_text.trim() {
                    return Err((409, format!("the active take of {id:?} voices other text: voice the line again or pick its take first")));
                }
            }
        }
        other => return Err((400, format!("{other:?} is not a take op"))),
    }
    Ok(out)
}

/// Файловая часть правки после сохранения проекта: выбранный дубль — в файл сегмента и seg_ckpt.json,
/// закрепление — в историю.
pub fn commit(wd: &Path, proj: &dub_core::Project, edit: &Value) -> Result<(), String> {
    let op = edit.get("op").and_then(Value::as_str).unwrap_or_default();
    let id = edit_id(edit).map_err(|(_, m)| m)?;
    let sid = seg_sid(proj, &id).map_err(|(_, m)| m)?;
    let _held = writes();
    let mut h = History::load(wd, &sid)?;
    match op {
        "take_select" => {
            let n = edit.get("take").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or("take_select without take")?;
            let key = h.get(n).map(|t| t.key.clone()).ok_or_else(|| format!("take {n} of segment {id:?} not found"))?;
            h.selected = Some(n);
            h.restore(wd, &sid, n, &wd.join(format!("seg_{sid}.wav")))?;
            h.save(wd, &sid)?;
            let mut ck = crate::render::SegCkpts::load(wd)?;
            ck.set(&sid, &key)?;
            Ok(())
        }
        "take_pin" => {
            let pin = edit.get("pinned").and_then(Value::as_bool).unwrap_or(false);
            h.pinned = if pin { h.active } else { None };
            h.save(wd, &sid)
        }
        other => Err(format!("{other:?} is not a take op")),
    }
}

// ---------------------------------------------------------------- маршруты

fn take_row(wd: &Path, sid: &str, t: &Take, current: &str) -> Value {
    json!({
        "n": t.n, "text": t.text, "text_matches": t.text == current.trim(), "dur": t.dur, "qc": t.qc,
        "source": t.source, "voice": t.voice, "reference": t.reference, "params": t.params, "created": t.created,
        "file": wav(wd, sid, t.n).to_string_lossy(),
    })
}

/// GET /projects/{pid}/segments/{id}/takes — дубли фразы, активный и закреплённый.
pub async fn list(State(st): State<AppState>, AxPath((pid, id)): AxPath<(String, String)>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let sid = match seg_sid(&proj, &id) {
        Ok(s) => s,
        Err((code, msg)) => return (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), msg).into_response(),
    };
    let current = proj.segments.iter().find(|s| s.id == id).map(|s| s.tgt_text.clone()).unwrap_or_default();
    match History::load(&dir, &sid) {
        Ok(h) => {
            let rows: Vec<Value> = h.takes.iter().rev().map(|t| take_row(&dir, &sid, t, &current)).collect();
            Json(json!({ "id": id, "active": h.active, "pinned": h.pinned, "takes": rows })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// GET /projects/{pid}/segments/{id}/takes/{n}/audio — клип дубля для прослушивания.
pub async fn audio(
    State(st): State<AppState>,
    AxPath((pid, id, n)): AxPath<(String, String, u32)>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let sid = match seg_sid(&proj, &id) {
        Ok(s) => s,
        Err((code, msg)) => return (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), msg).into_response(),
    };
    match History::load(&dir, &sid) {
        Ok(h) if h.get(n).is_some() => crate::serve_file_range(&wav(&dir, &sid, n), req, None).await,
        Ok(_) => (StatusCode::NOT_FOUND, format!("take {n} of segment {id:?} not found")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wd(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dub_takes_{tag}_{}_{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn take(text: &str, key: &str) -> NewTake {
        NewTake {
            text: text.into(),
            key: key.into(),
            nonce: None,
            voice: "clone".into(),
            reference: "ref_spk0.wav".into(),
            params: "{}".into(),
            source: "synth",
        }
    }

    fn clip(d: &Path, name: &str, body: &[u8]) -> PathBuf {
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn history_keeps_five_and_never_drops_the_pinned_one() {
        let d = wd("prune");
        let mut h = History::default();
        let first = h.add(&d, "s0", &clip(&d, "a.wav", b"a"), take("Привет", "k0"), 1.0).unwrap();
        h.pinned = Some(first);
        for i in 1..=6u8 {
            h.add(&d, "s0", &clip(&d, "a.wav", &[i]), take("Привет", &format!("k{i}")), 1.0).unwrap();
        }
        assert_eq!(h.takes.len(), MAX_TAKES);
        assert!(h.get(first).is_some(), "the pinned take stays");
        assert_eq!(h.active, Some(6));
        assert!(!wav(&d, "s0", 1).exists() && !wav(&d, "s0", 2).exists(), "the oldest unpinned takes are gone with their files");
        assert!(wav(&d, "s0", 6).exists());
        let back = History::load(&d, "s0").unwrap();
        assert_eq!(back, h);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn restore_puts_the_take_into_the_segment_file() {
        let d = wd("restore");
        let mut h = History::default();
        let a = h.add(&d, "s1", &clip(&d, "a.wav", b"AAA"), take("Один", "ka"), 1.0).unwrap();
        h.add(&d, "s1", &clip(&d, "b.wav", b"BBB"), take("Два", "kb"), 1.0).unwrap();
        let seg = d.join("seg_s1.wav");
        h.restore(&d, "s1", a, &seg).unwrap();
        assert_eq!(std::fs::read(&seg).unwrap(), b"AAA");
        assert_eq!(History::load(&d, "s1").unwrap().active, Some(a));
        assert_eq!(h.by_key("kb").map(|t| t.n), Some(1));
        assert!(h.restore(&d, "s1", 9, &seg).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_changed_text_unpins() {
        let d = wd("unpin");
        let mut h = History::default();
        h.add(&d, "s2", &clip(&d, "a.wav", b"A"), take("Привет", "k"), 1.0).unwrap();
        h.pinned = h.active;
        h.save(&d, "s2").unwrap();
        assert!(!unpin_if_stale(&d, "s2", " Привет ").unwrap());
        assert!(unpin_if_stale(&d, "s2", "Пока").unwrap());
        assert_eq!(History::load(&d, "s2").unwrap().pinned, None);
        let all = summaries(&d).unwrap();
        assert_eq!(all["s2"]["count"], 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_take_added_after_a_pin_from_the_window_keeps_the_pin() {
        let d = wd("pin-race");
        let mut held = History::default();
        let first = held.add(&d, "s8", &clip(&d, "a.wav", b"A"), take("Привет", "k1"), 1.0).unwrap();
        // the window pins while the render still holds its earlier copy of the history
        History::update(&d, "s8", |h| {
            h.pinned = h.active;
            h.save(&d, "s8")
        })
        .unwrap();
        let (fresh, second) = History::update(&d, "s8", |h| h.add(&d, "s8", &clip(&d, "b.wav", b"B"), take("Привет", "k2"), 1.0)).unwrap();
        assert_eq!(fresh.pinned, Some(first));
        assert_eq!(fresh.active, Some(second));
        assert_eq!(History::load(&d, "s8").unwrap().pinned, Some(first));
        assert_eq!(held.pinned, None);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn broken_history_is_an_error() {
        let d = wd("broken");
        std::fs::create_dir_all(d.join(DIR).join("s3")).unwrap();
        std::fs::write(d.join(DIR).join("s3").join(FILE), b"{").unwrap();
        assert!(History::load(&d, "s3").unwrap_err().contains("takes.json"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn take_select_is_resolved_and_committed() {
        let d = wd("select");
        let mut proj = dub_core::Project::default();
        proj.segments.push(dub_core::Segment { id: "s4".into(), tgt_text: "Новый".into(), ..Default::default() });
        let mut h = History::default();
        let old = h.add(&d, "s4", &clip(&d, "a.wav", b"OLD"), NewTake { nonce: Some(json!("n1")), ..take("Старый", "k-old") }, 1.0).unwrap();
        h.add(&d, "s4", &clip(&d, "b.wav", b"NEW"), take("Новый", "k-new"), 1.0).unwrap();
        let edit = json!({ "op": "take_select", "id": "s4", "take": old });
        let resolved = resolve(&d, &proj, &edit).unwrap();
        assert_eq!(resolved["take_text"], "Старый");
        assert_eq!(resolved["take_nonce"], "n1");
        assert_eq!(resolved["take_key"], "k-old");
        commit(&d, &proj, &resolved).unwrap();
        assert_eq!(std::fs::read(d.join("seg_s4.wav")).unwrap(), b"OLD");
        assert_eq!(crate::render::SegCkpts::load(&d).unwrap().get("s4"), Some("k-old"));
        assert_eq!(resolve(&d, &proj, &json!({ "op": "take_select", "id": "s4", "take": 7 })).unwrap_err().0, 404);
        assert_eq!(resolve(&d, &proj, &json!({ "op": "take_select", "id": "zz", "take": 0 })).unwrap_err().0, 404);
        assert_eq!(resolve(&d, &proj, &json!({ "op": "take_pin", "id": "s4", "pinned": true })).unwrap_err().0, 409, "the active take voices other text");
        proj.segments[0].tgt_text = "Старый".into();
        let pin = json!({ "op": "take_pin", "id": "s4", "pinned": true });
        resolve(&d, &proj, &pin).unwrap();
        commit(&d, &proj, &pin).unwrap();
        assert_eq!(History::load(&d, "s4").unwrap().pinned, Some(old));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn another_take_is_not_selected_while_one_is_pinned() {
        let d = wd("select_pinned");
        let mut proj = dub_core::Project::default();
        proj.segments.push(dub_core::Segment { id: "s5".into(), tgt_text: "Текст".into(), ..Default::default() });
        let mut h = History::default();
        let a = h.add(&d, "s5", &clip(&d, "a.wav", b"A"), take("Текст", "ka"), 1.0).unwrap();
        let b = h.add(&d, "s5", &clip(&d, "b.wav", b"B"), take("Текст", "kb"), 1.0).unwrap();
        let pin = json!({ "op": "take_pin", "id": "s5", "pinned": true });
        commit(&d, &proj, &resolve(&d, &proj, &pin).unwrap()).unwrap();
        let other = resolve(&d, &proj, &json!({ "op": "take_select", "id": "s5", "take": a })).unwrap_err();
        assert_eq!(other.0, 409);
        assert!(other.1.contains(&format!("take {b}")) && other.1.contains("unpin"), "{}", other.1);
        resolve(&d, &proj, &json!({ "op": "take_select", "id": "s5", "take": b })).expect("the pinned take itself stays selectable");
        let unpin = json!({ "op": "take_pin", "id": "s5", "pinned": false });
        commit(&d, &proj, &resolve(&d, &proj, &unpin).unwrap()).unwrap();
        let sel = resolve(&d, &proj, &json!({ "op": "take_select", "id": "s5", "take": a })).unwrap();
        commit(&d, &proj, &sel).unwrap();
        assert_eq!(History::load(&d, "s5").unwrap().active, Some(a));
        assert_eq!(std::fs::read(d.join("seg_s5.wav")).unwrap(), b"A");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_selected_take_plays_under_new_settings_until_the_text_or_the_nonce_changes() {
        let d = wd("selected");
        let mut proj = dub_core::Project::default();
        proj.segments.push(dub_core::Segment { id: "s7".into(), tgt_text: "Текст".into(), ..Default::default() });
        let mut h = History::default();
        let a = h.add(&d, "s7", &clip(&d, "a.wav", b"A"), take("Текст", "k-old-voice"), 1.0).unwrap();
        h.add(&d, "s7", &clip(&d, "b.wav", b"B"), take("Текст", "k-b"), 1.0).unwrap();
        let sel = resolve(&d, &proj, &json!({ "op": "take_select", "id": "s7", "take": a })).unwrap();
        commit(&d, &proj, &sel).unwrap();
        let h = History::load(&d, "s7").unwrap();
        assert_eq!(h.selected, Some(a));
        assert!(h.covers("k-new-voice", "Текст", None), "another voice since: the chosen take still plays");
        assert!(!h.covers("k-new-voice", "Другой", None), "other text is voiced anew");
        assert!(!h.covers("k-new-voice", "Текст", Some(&json!("n2"))), "a regeneration is voiced anew");
        let mut h = h;
        h.add(&d, "s7", &clip(&d, "c.wav", b"C"), take("Текст", "k-new-voice"), 1.0).unwrap();
        assert_eq!(h.selected, None, "a new take ends the choice");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_active_take_answers_its_key_before_a_newer_alternative() {
        let d = wd("by_key");
        let mut h = History::default();
        let first = h.add(&d, "s8", &clip(&d, "a.wav", b"A"), take("Текст", "k"), 1.0).unwrap();
        h.add(&d, "s8", &clip(&d, "b.wav", b"B"), take("Текст", "k"), 1.0).unwrap();
        h.active = Some(first);
        assert_eq!(h.by_key("k").map(|t| t.n), Some(first), "multi-take alternatives share a key: the active one wins");
        h.active = None;
        assert_eq!(h.by_key("k").map(|t| t.n), Some(first + 1));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_broken_history_is_set_aside_and_shows_as_its_line_s_error() {
        let d = wd("quarantine");
        std::fs::create_dir_all(d.join(DIR).join("s9")).unwrap();
        std::fs::write(d.join(DIR).join("s9").join(FILE), b"{").unwrap();
        std::fs::write(d.join(DIR).join("s9").join("0.wav"), b"old").unwrap();
        let all = summaries(&d).unwrap();
        assert!(all["s9"]["error"].as_str().unwrap().contains("takes.json"), "{all:?}");
        let moved = History::quarantine(&d, "s9").unwrap();
        assert_eq!(std::fs::read(moved.join("0.wav")).unwrap(), b"old", "the old clip is kept");
        assert!(summaries(&d).unwrap().is_empty(), "a set-aside history is not a line's");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn histories_of_lines_that_are_gone_are_dropped() {
        let d = wd("orphans");
        let mut h = History::default();
        h.add(&d, "s1", &clip(&d, "a.wav", b"A"), take("Один", "k1"), 1.0).unwrap();
        let mut g = History::default();
        g.add(&d, "s2", &clip(&d, "b.wav", b"B"), take("Два", "k2"), 1.0).unwrap();
        std::fs::create_dir_all(d.join(DIR).join("s2.broken-1")).unwrap();
        let live: std::collections::HashSet<String> = ["s1".to_string()].into();
        assert_eq!(drop_orphans(&d, &live).unwrap(), 2, "the deleted line's history and its set-aside copy");
        assert!(dir_of(&d, "s1").exists() && !dir_of(&d, "s2").exists());
        drop_all(&d).unwrap();
        assert!(!d.join(DIR).exists());
        drop_all(&d).unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_pinned_take_covers_a_regenerated_line_until_it_is_unpinned() {
        let d = wd("covers");
        let mut h = History::default();
        let p = h.add(&d, "s6", &clip(&d, "a.wav", b"P"), take("Привет", "k-first"), 1.0).unwrap();
        h.pinned = Some(p);
        assert!(h.covers("k-regen", " Привет ", None), "a regeneration changes the key, the pinned take still plays");
        assert_eq!(h.pinned_for("Привет").map(|t| t.n), Some(p));
        assert!(!h.covers("k-regen", "Пока", None), "other text is voiced anew");
        assert!(h.pinned_for("Пока").is_none());
        assert!(h.by_key("k-regen").is_none(), "covering adds nothing to the history");
        h.pinned = None;
        assert!(!h.covers("k-regen", "Привет", None), "unpinned, the regeneration is voiced");
        assert!(h.covers("k-first", "Привет", None));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
