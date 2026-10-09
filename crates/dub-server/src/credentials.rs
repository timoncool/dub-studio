//! Local credential storage for cloud providers and the proxy.
//!
//! A desktop Studio has no server to hold secrets for it, so the API key lives
//! beside the other runtime user data. Two rules keep it honest:
//!
//! * the environment variable always wins, so CI and scripted runs never read
//!   or write the user's stored key;
//! * the value is never serialized into settings, catalogs, job records or any
//!   response body — callers can only ask whether a key is configured.
//!
//! Где лежат секреты: в портативной раскладке — `<папка приложения>/secrets` (переезжают вместе с папкой),
//! иначе — `%LOCALAPPDATA%\Dub Studio\secrets`. `DUB_STUDIO_SECRETS_DIR` перекрывает оба варианта.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

pub const OPENROUTER_ENV_VAR: &str = "OPENROUTER_API_KEY";
pub const GOOGLE_ENV_VAR: &str = "GEMINI_API_KEY";
const GOOGLE_FILE: &str = "google-api-key";
pub const SECRETS_DIR_ENV_VAR: &str = "DUB_STUDIO_SECRETS_DIR";
const OPENROUTER_FILE: &str = "openrouter-api-key";
const PROXY_PASSWORD_FILE: &str = "proxy-password";
const LOCAL_SERVER_FILE: &str = "local-server-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Environment,
    LocalStore,
}

pub fn openrouter_api_key() -> Option<(String, CredentialSource)> {
    if let Some(key) = env::var(OPENROUTER_ENV_VAR).ok().map(|key| key.trim().to_owned()).filter(|key| !key.is_empty()) {
        return Some((key, CredentialSource::Environment));
    }
    let stored = read_secret(&secrets_dir()?, OPENROUTER_FILE)?;
    Some((stored, CredentialSource::LocalStore))
}

pub fn openrouter_source() -> Option<CredentialSource> {
    openrouter_api_key().map(|(_, source)| source)
}

pub fn google_api_key() -> Option<(String, CredentialSource)> {
    if let Some(key) = env::var(GOOGLE_ENV_VAR)
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
    {
        return Some((key, CredentialSource::Environment));
    }
    Some((
        read_secret(&secrets_dir()?, GOOGLE_FILE)?,
        CredentialSource::LocalStore,
    ))
}

pub fn store_google_api_key(api_key: Option<&str>) -> Result<Option<CredentialSource>> {
    if env::var(GOOGLE_ENV_VAR).is_ok_and(|key| !key.trim().is_empty()) {
        bail!("{GOOGLE_ENV_VAR} is set in this environment and takes priority; unset it before storing a key in Studio");
    }
    let dir =
        secrets_dir().context("no per-user application data directory for credential storage")?;
    Ok(write_secret(
        &dir,
        GOOGLE_FILE,
        api_key,
        "a Google API key must be a single line",
    )?
    .then_some(CredentialSource::LocalStore))
}

/// Stores or clears the key. Refuses to shadow an environment credential so a
/// user never believes they replaced a key that the process is not using.
pub fn store_openrouter_api_key(api_key: Option<&str>) -> Result<Option<CredentialSource>> {
    if env::var(OPENROUTER_ENV_VAR).is_ok_and(|key| !key.trim().is_empty()) {
        bail!("{OPENROUTER_ENV_VAR} is set in this environment and takes priority; unset it before storing a key in Studio");
    }
    let dir = secrets_dir().context("no per-user application data directory for credential storage")?;
    let stored = write_secret(&dir, OPENROUTER_FILE, api_key, "an OpenRouter API key must be a single line",
    )?;
    Ok(stored.then_some(CredentialSource::LocalStore))
}

/// Ключ локального OpenAI-совместимого сервера (LM Studio, vLLM с --api-key) лежит вместе с адресом, для
/// которого его сохранили (`dub_llm::server_base`), и отдаётся только для этого адреса: сменённый адрес или
/// чужой адрес в запросе ключа не получают.
#[derive(serde::Deserialize)]
struct ServerKeyRecord {
    address: String,
    key: String,
}

/// Ключ локального сервера для запросов на `address`; None — ключа нет или он сохранён для другого адреса.
pub fn local_server_key_for(address: &str) -> Option<String> {
    local_server_key_in(&secrets_dir()?, address)
}

/// Записать ключ локального сервера для адреса `address`; None или пусто — удалить. Возвращает, лежит ли теперь
/// ключ.
pub fn store_local_server_key(address: &str, key: Option<&str>) -> Result<bool> {
    let dir = secrets_dir().context("no per-user application data directory for credential storage")?;
    store_local_server_key_in(&dir, address, key)
}

pub(crate) fn store_local_server_key_in(dir: &Path, address: &str, key: Option<&str>,
) -> Result<bool> {
    let Some(key) = key.map(str::trim).filter(|key| !key.is_empty()) else {
        let stored = read_secret(dir, LOCAL_SERVER_FILE).and_then(|text| serde_json::from_str::<ServerKeyRecord>(&text).ok());
        if stored.is_some_and(|record| record.address != dub_llm::server_base(address)) {
            return Ok(false);
        }
        return write_secret(dir, LOCAL_SERVER_FILE, None, "");
    };
    if key.contains(['\r', '\n']) {
        bail!("a server key must be a single line");
    }
    let address = dub_llm::server_base(address);
    if address.is_empty() {
        bail!("a server key needs the address of its server");
    }
    let record = serde_json::json!({ "address": address, "key": key }).to_string();
    write_secret(dir, LOCAL_SERVER_FILE, Some(&record), "a server key must be a single line",
    )
}

pub(crate) fn local_server_key_in(dir: &Path, address: &str) -> Option<String> {
    let text = read_secret(dir, LOCAL_SERVER_FILE)?;
    match serde_json::from_str::<ServerKeyRecord>(&text) {
        Ok(record) => (record.address == dub_llm::server_base(address)).then_some(record.key),
        Err(error) => {
            tracing::error!("the local server key is stored without its address ({error}); save the key again");
            None
        }
    }
}

/// Пароль прокси: хранится отдельно от адреса, в active.json лежит адрес без пароля.
pub fn proxy_password() -> Option<String> {
    read_secret(&secrets_dir()?, PROXY_PASSWORD_FILE)
}

/// Записать пароль прокси; None или пусто — удалить.
pub(crate) fn store_proxy_password_in(dir: &Path, password: Option<&str>) -> Result<bool> {
    write_secret(dir, PROXY_PASSWORD_FILE, password, "a proxy password must be a single line",
    )
}

pub(crate) fn proxy_password_in(dir: &Path) -> Option<String> {
    read_secret(dir, PROXY_PASSWORD_FILE)
}

pub fn secrets_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os(SECRETS_DIR_ENV_VAR).filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    // Тесты не касаются настоящего хранилища пользователя, каким бы путём они сюда ни пришли.
    if cfg!(test) {
        return Some(env::temp_dir().join(format!("dub-studio-test-secrets-{}", std::process::id())),
        );
    }
    if let Some(app) = portable_app_dir() {
        return Some(app.join("secrets"));
    }

    #[cfg(windows)]
    {
        if let Some(root) = env::var_os("LOCALAPPDATA").or_else(|| env::var_os("APPDATA")) {
            return Some(PathBuf::from(root).join("Dub Studio").join("secrets"));
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(root) = env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(root).join("dub-studio").join("secrets"));
        }
        if let Some(home) = env::var_os("HOME") {
            return Some(PathBuf::from(home).join(".local/share/dub-studio/secrets"));
        }
    }

    None
}

/// Портативная раскладка — тот же маркер, что `is_portable` в desktop/src-tauri: рядом с exe лежат
/// `frontend/` и `models/`. `resolve_repo_root` оболочки возвращает тогда эту же папку, то есть это
/// `<repo_root>/secrets`.
fn portable_app_dir() -> Option<PathBuf> {
    let dir = env::current_exe().ok()?.parent()?.to_path_buf();
    (dir.join("frontend").is_dir() && dir.join("models").is_dir()).then_some(dir)
}

fn read_secret(dir: &Path, name: &str) -> Option<String> {
    let path = dir.join(name);
    match fs::read_to_string(&path) {
        Ok(text) => Some(text.trim().to_owned()).filter(|value| !value.is_empty()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::error!("secret {} is unreadable: {error}", path.display());
            None
        }
    }
}

/// Some — записать (через tmp + rename: падение не оставит половину ключа), None или пусто — удалить.
/// Возвращает, лежит ли теперь значение.
fn write_secret(dir: &Path, name: &str, value: Option<&str>, multiline_error: &str,
) -> Result<bool> {
    let path = dir.join(name);
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            if value.contains(['\r', '\n']) {
                bail!("{multiline_error}");
            }
            fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
            let tmp = dir.join(format!("{name}.tmp"));
            fs::write(&tmp, value).with_context(|| format!("write {}", tmp.display()))?;
            fs::rename(&tmp, &path).with_context(|| format!("replace {}", path.display()))?;
            Ok(true)
        }
        None => {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("remove {}", path.display()))
                }
            }
            Ok(false)
        }
    }
}

/// Что перенесено из active.json при старте.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Migrated {
    pub openrouter_key: bool,
    pub proxy_password: bool,
}

/// Прежние версии держали ключ OpenRouter (`or_key`) и пароль прокси (внутри `proxy_url`) открытым
/// текстом в models/active.json. Переносит их в хранилище секретов и переписывает active.json без них.
/// Из active.json секрет уходит только после того, как он записан в хранилище.
pub fn migrate_legacy_selection(models_root: &Path) -> Result<Migrated> {
    let dir = secrets_dir().context("no per-user application data directory for credential storage")?;
    migrate_legacy_selection_into(models_root, &dir)
}

pub(crate) fn migrate_legacy_selection_into(models_root: &Path, dir: &Path) -> Result<Migrated> {
    let _held = crate::models::selection_writes();
    let mut selection = crate::models::load_selection(models_root);
    let slots = selection.as_object_mut().expect("load_selection returns object");
    let mut migrated = Migrated::default();
    let mut changed = false;

    if let Some(legacy) = slots.remove("or_key") {
        if let Some(key) = legacy.as_str().map(str::trim).filter(|key| !key.is_empty()) {
            write_secret(dir, OPENROUTER_FILE, Some(key), "an OpenRouter API key must be a single line",
            )?;
            migrated.openrouter_key = true;
        }
        changed = true;
    }

    if let Some(url) = slots.get("proxy_url").and_then(Value::as_str).map(str::to_owned) {
        let (bare, password) = crate::models::split_proxy_password(&url);
        if let Some(password) = password {
            store_proxy_password_in(dir, Some(&password))?;
            migrated.proxy_password = true;
        }
        if bare != url {
            slots.insert("proxy_url".into(), Value::String(bare));
            changed = true;
        }
    }

    if changed {
        crate::models::write_selection(models_root, &selection).context("rewrite active.json without secrets")?;
    }
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("dub-credentials-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_environment_key_is_reported_as_such() {
        // The variable is process-wide; only assert the branch that does not
        // depend on mutating global state in a parallel test run.
        if let Ok(key) = env::var(OPENROUTER_ENV_VAR) {
            if !key.trim().is_empty() {
                assert_eq!(openrouter_source(), Some(CredentialSource::Environment));
            }
        }
    }

    #[test]
    fn storing_is_refused_while_the_environment_variable_wins() {
        if env::var(OPENROUTER_ENV_VAR).is_ok_and(|key| !key.trim().is_empty()) {
            assert!(store_openrouter_api_key(Some("test")).is_err());
        }
    }

    #[test]
    fn a_secret_is_written_whole_and_removed_on_empty() {
        let dir = scratch("roundtrip");
        assert!(write_secret(&dir, OPENROUTER_FILE, Some("  sk-or-v1-abc \n"), "one line").unwrap());
        assert_eq!(read_secret(&dir, OPENROUTER_FILE).as_deref(), Some("sk-or-v1-abc"));
        assert!(!dir.join(format!("{OPENROUTER_FILE}.tmp")).exists());
        assert!(write_secret(&dir, OPENROUTER_FILE, Some("a\nb"), "one line").is_err());
        assert!(!write_secret(&dir, OPENROUTER_FILE, Some("   "), "one line").unwrap());
        assert_eq!(read_secret(&dir, OPENROUTER_FILE), None);
        assert!(!write_secret(&dir, OPENROUTER_FILE, None, "one line").unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_local_server_key_belongs_to_its_address() {
        let dir = scratch("server-key");
        let lan = "http://192.168.1.5:1234";
        assert!(store_local_server_key_in(&dir, lan, Some(" lm-studio-key ")).unwrap());
        for same in [lan, "http://192.168.1.5:1234/", "http://192.168.1.5:1234/v1", " http://192.168.1.5:1234/v1/ ",
        ] {
            assert_eq!(local_server_key_in(&dir, same).as_deref(), Some("lm-studio-key"), "{same}");
        }
        for other in ["http://attacker.example:1234", "http://192.168.1.5:1235", "https://192.168.1.5:1234", "http://127.0.0.1:11434",
        ] {
            assert_eq!(local_server_key_in(&dir, other), None, "the key must not reach {other}");
        }
        assert!(!fs::read_to_string(dir.join(LOCAL_SERVER_FILE)).unwrap().contains('\n'));
        assert!(store_local_server_key_in(&dir, "", Some("k")).is_err());
        assert!(store_local_server_key_in(&dir, lan, Some("a\nb")).is_err());

        assert!(store_local_server_key_in(&dir, "http://127.0.0.1:11434", Some("other")).unwrap());
        assert_eq!(local_server_key_in(&dir, lan), None, "a key saved for another address replaces the old one");
        assert_eq!(local_server_key_in(&dir, "http://127.0.0.1:11434").as_deref(), Some("other"));
        assert!(!store_local_server_key_in(&dir, lan, None).unwrap());
        assert_eq!(local_server_key_in(&dir, "http://127.0.0.1:11434").as_deref(), Some("other"), "removing the key of another address keeps this one");

        fs::write(dir.join(LOCAL_SERVER_FILE), "bare-key").unwrap();
        assert_eq!(local_server_key_in(&dir, lan), None, "a key without its address goes nowhere");

        assert!(!store_local_server_key_in(&dir, lan, None).unwrap());
        assert_eq!(local_server_key_in(&dir, lan), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tests_never_reach_the_users_store() {
        if env::var_os(SECRETS_DIR_ENV_VAR).is_none() {
            assert!(secrets_dir().unwrap().starts_with(env::temp_dir()));
        }
    }

    #[test]
    fn legacy_secrets_leave_active_json_for_the_store() {
        let models = scratch("migrate-models");
        let secrets = scratch("migrate-secrets");
        fs::write(
            models.join("active.json"),
            r#"{"tts":"q6_k","or_key":"sk-or-v1-secret","proxy_on":"1","proxy_url":"http://alice:s3cr%40t@proxy.lan:3128"}"#,
        )
        .unwrap();

        let migrated = migrate_legacy_selection_into(&models, &secrets).unwrap();
        assert_eq!(migrated, Migrated { openrouter_key: true, proxy_password: true });

        let on_disk = fs::read_to_string(models.join("active.json")).unwrap();
        assert!(!on_disk.contains("sk-or-v1-secret") && !on_disk.contains("s3cr%40t") && !on_disk.contains("or_key"));
        let selection = crate::models::load_selection(&models);
        assert_eq!(selection["proxy_url"], "http://alice@proxy.lan:3128");
        assert_eq!(selection["tts"], "q6_k");
        assert_eq!(selection["proxy_on"], "1");
        assert_eq!(read_secret(&secrets, OPENROUTER_FILE).as_deref(), Some("sk-or-v1-secret"));
        assert_eq!(proxy_password_in(&secrets).as_deref(), Some("s3cr@t"), "the store keeps the password itself, not its %XX form");
        assert!(!models.join("active.json.tmp").exists());

        assert_eq!(migrate_legacy_selection_into(&models, &secrets).unwrap(), Migrated::default());
        assert_eq!(fs::read_to_string(models.join("active.json")).unwrap(), on_disk);

        fs::remove_dir_all(&models).unwrap();
        fs::remove_dir_all(&secrets).unwrap();
    }

    #[test]
    fn a_selection_without_secrets_is_left_untouched() {
        let models = scratch("untouched-models");
        let secrets = scratch("untouched-secrets");
        let original = "{\n  \"proxy_url\": \"socks5://proxy.lan:1080\",\n  \"tts\": \"q8_0\"\n}";
        fs::write(models.join("active.json"), original).unwrap();
        assert_eq!(migrate_legacy_selection_into(&models, &secrets).unwrap(), Migrated::default());
        assert_eq!(fs::read_to_string(models.join("active.json")).unwrap(), original);
        assert!(fs::read_dir(&secrets).unwrap().next().is_none());
        fs::remove_dir_all(&models).unwrap();
        fs::remove_dir_all(&secrets).unwrap();
    }
}
