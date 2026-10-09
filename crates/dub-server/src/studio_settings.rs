//! Настройки окна, которые живут у сервиса, а не в localStorage окна: дефолты запуска дубляжа (форма
//! стартового экрана) и пути данных для раздела «О программе». Дефолты лежат рядом с active.json, поэтому
//! их видят все окна и агент, и они не зависят от профиля WebView2 и origin окна.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::AppState;

pub const LAUNCH_FILE: &str = "launch_defaults.json";

const VO_GAIN_MIN_DB: f64 = -24.0;
const VO_GAIN_MAX_DB: f64 = 0.0;
const MAX_TEXT_CHARS: usize = 4000;
const MAX_SLOTS: usize = 64;

/// Чтение-слияние-запись PATCH идут под одним замком: два окна не затирают правки друг друга.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    Nodub,
    Dub,
    Voiceover,
    Transcribe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubsMode {
    None,
    Transcribe,
    Translate,
    /// Перевод и оригинал второй строкой.
    Bilingual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Auto,
    Real,
    Anime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrStyle {
    #[serde(rename = "")]
    Normal,
    Technical,
    Literary,
    Casual,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mp4,
    Mkv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceSource {
    Clone,
    Library,
}

/// Выбор формы запуска, с которым открывается стартовый экран. `tgt_lang: None` — язык интерфейса окна.
/// Поле, которого нет в файле (файл старой версии), берётся из `Default`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchDefaults {
    pub audio: AudioMode,
    pub subs: SubsMode,
    pub burn: bool,
    pub detect_text: bool,
    pub src_lang: String,
    pub speaker_count: usize,
    pub tgt_lang: Option<String>,
    pub casting: bool,
    pub casting_ref: String,
    pub content_type: ContentType,
    pub vo_gain_db: f64,
    pub tr_style: TrStyle,
    pub tr_style_custom: String,
    pub sub_blur: bool,
    pub keep_orig: bool,
    pub container: Container,
    pub voice_src: VoiceSource,
    pub voice_slots_m: Vec<String>,
    pub voice_slots_f: Vec<String>,
}

impl Default for LaunchDefaults {
    fn default() -> Self {
        LaunchDefaults {
            audio: AudioMode::Dub,
            subs: SubsMode::Translate,
            burn: true,
            detect_text: false,
            src_lang: "auto".to_string(),
            speaker_count: 0,
            tgt_lang: None,
            casting: false,
            casting_ref: String::new(),
            content_type: ContentType::Auto,
            vo_gain_db: -12.0,
            tr_style: TrStyle::Normal,
            tr_style_custom: String::new(),
            sub_blur: true,
            keep_orig: false,
            container: Container::Mp4,
            voice_src: VoiceSource::Clone,
            voice_slots_m: Vec::new(),
            voice_slots_f: Vec::new(),
        }
    }
}

fn is_content_lang(code: &str) -> bool {
    dub_translate::WHISPER_LANGS.iter().any(|(c, _)| *c == code)
}

impl LaunchDefaults {
    pub fn validate(&self) -> Result<(), String> {
        if self.speaker_count > dub_asr::MAX_SPEAKERS {
            return Err(t!("settings-bad-speaker-count", max = dub_asr::MAX_SPEAKERS));
        }
        if !self.vo_gain_db.is_finite() || !(VO_GAIN_MIN_DB..=VO_GAIN_MAX_DB).contains(&self.vo_gain_db) {
            return Err(t!("settings-bad-vo-gain", value = self.vo_gain_db, min = VO_GAIN_MIN_DB, max = VO_GAIN_MAX_DB));
        }
        if self.src_lang != "auto" && !is_content_lang(&self.src_lang) {
            return Err(t!("settings-bad-src-lang", value = format!("{:?}", self.src_lang)));
        }
        if let Some(t) = &self.tgt_lang {
            if !is_content_lang(t) {
                return Err(t!("settings-bad-tgt-lang", value = format!("{t:?}")));
            }
        }
        if !self
            .casting_ref
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(t!("settings-bad-casting-ref", value = format!("{:?}", self.casting_ref)));
        }
        if self.tr_style_custom.chars().count() > MAX_TEXT_CHARS {
            return Err(t!("settings-style-too-long", max = MAX_TEXT_CHARS));
        }
        for (name, slots) in [("voice_slots_m", &self.voice_slots_m), ("voice_slots_f", &self.voice_slots_f)] {
            if slots.len() > MAX_SLOTS {
                return Err(t!("settings-too-many-slots", name = name, max = MAX_SLOTS));
            }
        }
        Ok(())
    }
}

pub fn launch_path(models_root: &Path) -> PathBuf {
    models_root.join(LAUNCH_FILE)
}

/// Сохранённые дефолты и признак, что файл есть. Нет файла — встроенные дефолты и `false`
/// (окно по этому признаку один раз переносит свой старый выбор из localStorage).
pub fn load(models_root: &Path) -> Result<(LaunchDefaults, bool), String> {
    let path = launch_path(models_root);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((LaunchDefaults::default(), false)),
        Err(e) => return Err(t!("common-read", path = path.display().to_string(), error = e.to_string())),
    };
    let defaults: LaunchDefaults =
        serde_json::from_str(&text).map_err(|e| t!("common-corrupt", path = path.display().to_string(), error = e.to_string()))?;
    defaults
        .validate()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((defaults, true))
}

/// Слить частичную правку в сохранённые дефолты и записать атомарно. Незнакомое поле или неверное
/// значение — ошибка целиком, файл не меняется.
pub fn apply_patch(models_root: &Path, patch: &Map<String, Value>) -> Result<LaunchDefaults, String> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| "the launch defaults write lock is poisoned by a panic".to_string())?;
    let (current, _) = load(models_root)?;
    let mut merged = match serde_json::to_value(&current) {
        Ok(Value::Object(m)) => m,
        Ok(other) => return Err(format!("the launch defaults serialized to a non-object: {other}")),
        Err(e) => return Err(format!("serializing the launch defaults: {e}")),
    };
    for (key, value) in patch {
        if !merged.contains_key(key) {
            return Err(t!("settings-unknown-field", key = format!("{key:?}")));
        }
        merged.insert(key.clone(), value.clone());
    }
    let next: LaunchDefaults = serde_json::from_value(Value::Object(merged)).map_err(|e| e.to_string())?;
    next.validate()?;
    let body = serde_json::to_string_pretty(&next).map_err(|e| format!("serializing: {e}"))?;
    std::fs::create_dir_all(models_root).map_err(|e| t!("common-create-dir", path = models_root.display().to_string(), error = e.to_string()))?;
    let path = launch_path(models_root);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body.as_bytes()).map_err(|e| t!("common-write", what = tmp.display().to_string(), error = e.to_string()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), path.display()))?;
    Ok(next)
}

/// GET /settings/launch -> {defaults, saved}
pub async fn launch_get(State(st): State<AppState>) -> Response {
    let root = st.models_root.clone();
    match tokio::task::spawn_blocking(move || load(&root)).await {
        Ok(Ok((defaults, saved))) => Json(json!({ "defaults": defaults, "saved": saved })).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, t!("settings-read-failed", error = e.to_string())).into_response(),
    }
}

/// PATCH /settings/launch {поле: значение, …} -> {defaults, saved: true}
pub async fn launch_patch(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let Value::Object(patch) = body else {
        return (StatusCode::BAD_REQUEST, t!("settings-patch-not-object")).into_response();
    };
    let root = st.models_root.clone();
    match tokio::task::spawn_blocking(move || apply_patch(&root, &patch)).await {
        Ok(Ok(defaults)) => Json(json!({ "defaults": defaults, "saved": true })).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, t!("settings-write-failed", error = e.to_string())).into_response(),
    }
}

/// GET /app/paths — где лежат данные студии (раздел «О программе»).
pub async fn app_paths(State(st): State<AppState>) -> Json<Value> {
    Json(json!({
        "data_dir": st.repo_root.display().to_string(),
        "projects_dir": st.workspace.display().to_string(),
        "models_dir": st.models_root.display().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dub_launch_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => unreachable!(),
        }
    }

    #[test]
    fn missing_file_gives_builtin_defaults_not_saved() {
        let root = temp_root("missing");
        let (d, saved) = load(&root).unwrap();
        assert!(!saved);
        assert_eq!(d, LaunchDefaults::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn patch_merges_and_persists() {
        let root = temp_root("merge");
        apply_patch(&root, &obj(json!({ "audio": "voiceover", "vo_gain_db": -6.5, "speaker_count": 8 }))).unwrap();
        let d = apply_patch(&root, &obj(json!({ "tr_style": "", "voice_slots_m": ["RU_Male_A"] }))).unwrap();
        assert_eq!(d.audio, AudioMode::Voiceover);
        assert_eq!(d.vo_gain_db, -6.5);
        assert_eq!(d.speaker_count, 8);
        assert_eq!(d.tr_style, TrStyle::Normal);
        assert_eq!(d.voice_slots_m, vec!["RU_Male_A".to_string()]);
        let d = apply_patch(&root, &obj(json!({ "subs": "bilingual" }))).unwrap();
        assert_eq!(d.subs, SubsMode::Bilingual);
        let (loaded, saved) = load(&root).unwrap();
        assert!(saved);
        assert_eq!(loaded, d);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_unknown_field_and_bad_values_without_writing() {
        let root = temp_root("reject");
        assert!(apply_patch(&root, &obj(json!({ "audio": "karaoke" }))).is_err());
        assert!(apply_patch(&root, &obj(json!({ "colour": "red" }))).is_err());
        assert!(apply_patch(&root, &obj(json!({ "vo_gain_db": 3.0 }))).is_err());
        assert!(apply_patch(&root, &obj(json!({ "tgt_lang": "xx" }))).is_err());
        assert!(apply_patch(&root, &obj(json!({ "src_lang": "" }))).is_err());
        for count in [json!(-1), json!(9), json!(1.5), json!("8")] {
            assert!(apply_patch(&root, &obj(json!({ "speaker_count": count }))).is_err());
        }
        assert!(apply_patch(&root, &obj(json!({ "casting_ref": "../x" }))).is_err());
        assert!(!launch_path(&root).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn target_language_can_return_to_the_window_language() {
        let root = temp_root("tgt");
        apply_patch(&root, &obj(json!({ "tgt_lang": "de", "src_lang": "ja" }))).unwrap();
        let d = apply_patch(&root, &obj(json!({ "tgt_lang": null }))).unwrap();
        assert_eq!(d.tgt_lang, None);
        assert_eq!(d.src_lang, "ja");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn older_file_without_new_fields_is_filled_from_defaults() {
        let root = temp_root("old");
        std::fs::write(launch_path(&root), r#"{"audio":"nodub","burn":false}"#).unwrap();
        let (d, saved) = load(&root).unwrap();
        assert!(saved);
        assert_eq!(d.audio, AudioMode::Nodub);
        assert!(!d.burn);
        assert_eq!(d.container, Container::Mp4);
        assert_eq!(d.speaker_count, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_file_is_an_error() {
        let root = temp_root("broken");
        std::fs::write(launch_path(&root), "{not json").unwrap();
        assert!(load(&root).is_err());
        assert!(apply_patch(&root, &obj(json!({ "burn": true }))).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
