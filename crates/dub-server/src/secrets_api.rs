//! Ручки секретов: ключ OpenRouter, ключ локального сервера и прокси (адрес без пароля). Сами значения наружу
//! не уходят никогда — только «задан ли» и источник. Ошибки — JSON `{error: <код>, detail}`: текст для окна
//! выбирает фронт по коду.

use std::collections::HashMap;
use std::path::Path;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::credentials::{self, CredentialSource};
use crate::AppState;

fn failure(status: StatusCode, code: &str, detail: impl Into<String>) -> Response {
    (status, Json(json!({ "error": code, "detail": detail.into() })),
    )
        .into_response()
}

fn openrouter_state() -> Value {
    let source = credentials::openrouter_source();
    json!({
        "configured": source.is_some(),
        "source": source,
        "environment_variable": credentials::OPENROUTER_ENV_VAR,
    })
}

fn google_state() -> Value {
    let source = credentials::google_api_key().map(|(_, s)| s);
    json!({"configured":source.is_some(),"source":source,"environment_variable":credentials::GOOGLE_ENV_VAR})
}

pub async fn google_settings() -> Json<Value> {
    Json(google_state())
}

pub async fn update_google_settings(Json(body): Json<Value>) -> Response {
    let key = body["api_key"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if key.is_empty() || key.contains(['\r', '\n']) {
        return failure(
            StatusCode::BAD_REQUEST,
            "invalid_key",
            "Google API key must be a nonempty single line",
        );
    }
    if credentials::google_api_key().is_some_and(|(_, s)| s == CredentialSource::Environment) {
        return failure(
            StatusCode::CONFLICT,
            "environment_key",
            credentials::GOOGLE_ENV_VAR,
        );
    }
    let candidate = key.clone();
    match tokio::task::spawn_blocking(move || crate::google_tts::Client::new(candidate)?.models())
        .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return failure(StatusCode::BAD_GATEWAY, "verify_failed", e),
        Err(e) => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "verify_failed",
                e.to_string(),
            )
        }
    }
    match credentials::store_google_api_key(Some(&key)) {
        Ok(_) => Json(google_state()).into_response(),
        Err(e) => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_failed",
            e.to_string(),
        ),
    }
}

pub async fn delete_google_settings() -> Response {
    match credentials::store_google_api_key(None) {
        Ok(_) => Json(google_state()).into_response(),
        Err(e) => failure(StatusCode::CONFLICT, "environment_key", e.to_string()),
    }
}

pub async fn google_models() -> Response {
    let Some((key, _)) = credentials::google_api_key() else {
        return failure(
            StatusCode::BAD_REQUEST,
            "missing_key",
            "Google API key is not configured",
        );
    };
    match tokio::task::spawn_blocking(move || crate::google_tts::Client::new(key)?.models()).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => failure(StatusCode::BAD_GATEWAY, "google_models_failed", e),
        Err(e) => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "google_models_failed",
            e.to_string(),
        ),
    }
}

// ─── GET /engine/openrouter/settings ────────────────────────────────────────
pub async fn openrouter_settings() -> Json<Value> {
    Json(openrouter_state())
}

// ─── PUT /engine/openrouter/settings {api_key} ──────────────────────────────
// Ключ сохраняется только после того, как OpenRouter его принял (GET /key).
pub async fn update_openrouter_settings(Json(body): Json<Value>) -> Response {
    let key = body.get("api_key").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
    if key.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "empty_key", "api_key is empty; DELETE /engine/openrouter/settings removes the key",
        );
    }
    if key.contains(['\r', '\n']) {
        return failure(StatusCode::BAD_REQUEST, "invalid_key", "an OpenRouter API key must be a single line",
        );
    }
    if credentials::openrouter_source() == Some(CredentialSource::Environment) {
        return failure(
            StatusCode::CONFLICT,
            "environment_key",
            format!("{} is set in this environment and takes priority", credentials::OPENROUTER_ENV_VAR),
        );
    }
    let candidate = key.clone();
    let verified = tokio::task::spawn_blocking(move || {
        dub_llm::openrouter::OpenRouter::new(Some(candidate)).and_then(|client| client.check_key()).map_err(|e| format!("{e:#}"))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match verified {
        Err(e) => return failure(StatusCode::BAD_GATEWAY, "verify_failed", e),
        Ok(dub_llm::openrouter::KeyCheck::Rejected(detail)) => {
            return failure(StatusCode::BAD_REQUEST, "key_rejected", detail)
        }
        Ok(dub_llm::openrouter::KeyCheck::Accepted(_)) => {}
    }
    match credentials::store_openrouter_api_key(Some(&key)) {
        Ok(_) => Json(openrouter_state()).into_response(),
        Err(e) => failure(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", format!("{e:#}"),
        ),
    }
}

// ─── DELETE /engine/openrouter/settings ─────────────────────────────────────
pub async fn delete_openrouter_settings() -> Response {
    if credentials::openrouter_source() == Some(CredentialSource::Environment) {
        return failure(
            StatusCode::CONFLICT,
            "environment_key",
            format!("{} is set in this environment; unset it to remove the key", credentials::OPENROUTER_ENV_VAR),
        );
    }
    match credentials::store_openrouter_api_key(None) {
        Ok(_) => Json(openrouter_state()).into_response(),
        Err(e) => failure(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", format!("{e:#}"),
        ),
    }
}

// ─── GET|PUT|DELETE /engine/server/key ─────────────────────────────────────
// Ключ локального OpenAI-совместимого сервера (LM Studio, vLLM с --api-key). Не проверяется: сервер может
// быть ещё не запущен. Ключ принадлежит адресу, для которого его сохранили: `url` в запросе (адрес в поле
// окна) или адрес из настроек. Наружу — только «задан ли для этого адреса».
fn server_key_address(models_root: &Path, url: Option<&str>) -> String {
    url.map(str::trim).filter(|url| !url.is_empty()).map_or_else(|| crate::models::server_url(models_root), str::to_string)
}

fn server_key_state(address: &str) -> Value {
    json!({ "configured": credentials::local_server_key_for(address).is_some() })
}

// GET /engine/server/key?url=
pub async fn server_key_settings(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    Json(server_key_state(&server_key_address(&st.models_root, q.get("url").map(String::as_str),
    )))
}

// PUT /engine/server/key {api_key, url?}
pub async fn update_server_key(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let key = body.get("api_key").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if key.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "empty_key", "api_key is empty; DELETE /engine/server/key removes the key",
        );
    }
    let address = server_key_address(&st.models_root, body.get("url").and_then(Value::as_str));
    match credentials::store_local_server_key(&address, Some(key)) {
        Ok(_) => Json(server_key_state(&address)).into_response(),
        Err(e) if key.contains(['\r', '\n']) => {
            failure(StatusCode::BAD_REQUEST, "invalid_key", format!("{e:#}"))
        }
        Err(e) => failure(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", format!("{e:#}"),
        ),
    }
}

// DELETE /engine/server/key?url=
pub async fn delete_server_key(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>,
) -> Response {
    let address = server_key_address(&st.models_root, q.get("url").map(String::as_str));
    match credentials::store_local_server_key(&address, None) {
        Ok(_) => Json(server_key_state(&address)).into_response(),
        Err(e) => failure(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", format!("{e:#}"),
        ),
    }
}

/// Прокси как его видит окно: режим, тип, адрес без пароля, флаг «пароль задан» и `problem` — почему
/// сохранённый свой адрес не читается (запросы тогда идут напрямую). Проверяется адрес, по которому пошли бы
/// запросы, — вместе с паролем; текст причины его маскирует.
pub(crate) fn proxy_view(models_root: &Path, secrets: &Path) -> Value {
    let selection = crate::models::load_selection(models_root);
    let password = credentials::proxy_password_in(secrets);
    let public = crate::models::redact_selection(&selection, false, password.is_some());
    let mode = crate::models::proxy_mode(&selection);
    let kind = crate::models::proxy_kind(&selection);
    let url = public.get("proxy_url").and_then(Value::as_str).unwrap_or_default().to_string();
    let problem = match mode {
        dub_llm::net::ProxyMode::Custom => {
            let address = crate::models::proxy_address(&selection, password.as_deref()).unwrap_or_default();
            dub_llm::net::normalize(&address, kind).err().map(|e| format!("{e:#}"))
        }
        _ => None,
    };
    json!({
        "mode": mode.as_str(),
        "kind": kind.as_str(),
        "on": mode == dub_llm::net::ProxyMode::Custom,
        "url": url,
        "password_set": public["proxy_password_set"],
        "problem": problem,
    })
}

/// Адрес для POST /engine/proxy/test. Пароль из тела подставляется в любой адрес, сохранённый — только в
/// сохранённый адрес: в чужой он ушёл бы хосту из тела запроса (Proxy-Authorization, SOCKS-логин).
pub(crate) fn proxy_test_address(models_root: &Path, secrets: Option<&Path>, url: &str, typed: Option<&str>,
) -> String {
    let url = url.trim();
    let password = match typed.map(str::trim).filter(|typed| !typed.is_empty()) {
        Some(typed) => Some(typed.to_string()),
        None => {
            let selection = crate::models::load_selection(models_root);
            let saved = selection.get("proxy_url").and_then(Value::as_str).map(str::trim);
            if saved == Some(url) {
                secrets.and_then(credentials::proxy_password_in)
            } else {
                None
            }
        }
    };
    crate::models::proxy_with_password(url, password.as_deref())
}

#[derive(Debug)]
pub(crate) struct FormError {
    status: StatusCode,
    code: &'static str,
    detail: String,
}

impl FormError {
    fn bad(code: &'static str, detail: impl Into<String>) -> Self {
        FormError { status: StatusCode::BAD_REQUEST, code, detail: detail.into(),
        }
    }
    fn internal(detail: impl std::fmt::Display) -> Self {
        FormError { status: StatusCode::INTERNAL_SERVER_ERROR, code: "store_failed", detail: detail.to_string(),
        }
    }
}

/// Сохранить форму прокси `{mode?, kind?, url?, password?, on?}`. `mode`: off | system | custom; `on` — прежняя
/// форма (true = custom, false = off). Адрес в любой ходовой записи (host:port:user:password и т.п.)
/// приводится к URL со схемой по `kind`. Пароль: нет поля или пустая строка — оставить сохранённый (форма его
/// не знает), null — удалить, строка — заменить. Пароль, вписанный прямо в адрес, тоже уходит в хранилище;
/// в active.json адрес попадает всегда без пароля.
pub(crate) fn apply_proxy_form(models_root: &Path, secrets: &Path, form: &Value,
) -> Result<(), FormError> {
    let change = match form.get("password") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(password)) if password.trim().is_empty() => None,
        Some(Value::String(password)) => Some(Some(password.trim().to_string())),
        Some(_) => {
            return Err(FormError::bad("invalid_proxy_password", "password must be a string or null",
            ))
        }
    };
    let on = match form.get("on") {
        None => None,
        Some(Value::Bool(on)) => Some(*on),
        Some(_) => return Err(FormError::bad("invalid_proxy_on", "on must be a boolean")),
    };
    let mode = match form.get("mode") {
        None => on.map(|on| {
            if on { dub_llm::net::ProxyMode::Custom } else { dub_llm::net::ProxyMode::Off }
        }),
        Some(Value::String(mode)) => {
            Some(
            dub_llm::net::ProxyMode::parse(mode).ok_or_else(|| {
                FormError::bad("invalid_proxy_mode", "mode must be off, system or custom")
            })?)
        }
        Some(_) => {
            return Err(FormError::bad("invalid_proxy_mode", "mode must be off, system or custom",
            ))
        }
    };

    let _held = crate::models::selection_writes();
    let mut selection = crate::models::load_selection(models_root);
    // Режим, который действует сейчас (в т.ч. выведенный из прежнего proxy_on), — если форма его не меняет.
    let mode = mode.unwrap_or_else(|| crate::models::proxy_mode(&selection));
    let kind = match form.get("kind") {
        None => crate::models::proxy_kind(&selection),
        Some(Value::String(kind)) => dub_llm::net::ProxyKind::parse(kind)
            .ok_or_else(|| {
            FormError::bad("invalid_proxy_kind", "kind must be http, https, socks5 or socks4",
            )
        })?,
        Some(_) => {
            return Err(FormError::bad("invalid_proxy_kind", "kind must be http, https, socks5 or socks4",
            ))
        }
    };
    let slots = selection.as_object_mut().expect("load_selection returns object");
    let store = match form.get("url") {
        None => {
            let stored = slots.get("proxy_url").and_then(Value::as_str).unwrap_or_default();
            if matches!(change, Some(Some(_))) && !crate::models::proxy_has_user(stored) {
                return Err(FormError::bad("proxy_password_without_user", "a proxy password needs a user name in the address (user@host:port)",
                ));
            }
            change
        }
        Some(Value::String(url)) if url.trim().is_empty() => {
            slots.remove("proxy_url");
            Some(None)
        }
        Some(Value::String(url)) => {
            let written = dub_llm::net::normalize(url, kind)
                .map_err(|e| FormError::bad("invalid_proxy_url", format!("{e:#}")))?;
            let (bare, inline) = crate::models::split_proxy_password(&dub_llm::net::normalized_text(&written));
            let change = change.or(inline.map(Some));
            let store = if crate::models::proxy_has_user(&bare) {
                change
            } else {
                if matches!(change, Some(Some(_))) {
                    return Err(FormError::bad("proxy_password_without_user", "a proxy password needs a user name in the address (user@host:port)",
                    ));
                }
                Some(None)
            };
            slots.insert("proxy_url".into(), Value::String(bare));
            store
        }
        Some(_) => return Err(FormError::bad("invalid_proxy_url", "url must be a string")),
    };
    if mode == dub_llm::net::ProxyMode::Custom && slots.get("proxy_url").and_then(Value::as_str).is_none_or(|url| url.trim().is_empty()) {
        return Err(FormError::bad("proxy_url_required", "a proxy of your own needs an address; switch the mode before removing it",
        ));
    }
    if let Some(change) = store {
        credentials::store_proxy_password_in(secrets, change.as_deref()).map_err(FormError::internal)?;
    }
    slots.insert("proxy_mode".into(), Value::String(mode.as_str().into()));
    slots.insert("proxy_kind".into(), Value::String(kind.as_str().into()));
    slots.remove("proxy_on");
    crate::models::write_selection(models_root, &selection).map_err(FormError::internal)
}

// ─── GET /engine/proxy/settings ─────────────────────────────────────────────
pub async fn proxy_settings(State(st): State<AppState>) -> Response {
    let Some(secrets) = credentials::secrets_dir() else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "no_secrets_dir", "no per-user application data directory for credential storage",
        );
    };
    Json(proxy_view(&st.models_root, &secrets)).into_response()
}

// ─── PUT /engine/proxy/settings {on?, url?, password?} ──────────────────────
pub async fn update_proxy_settings(State(st): State<AppState>, Json(form): Json<Value>,
) -> Response {
    let Some(secrets) = credentials::secrets_dir() else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "no_secrets_dir", "no per-user application data directory for credential storage",
        );
    };
    match apply_proxy_form(&st.models_root, &secrets, &form) {
        Ok(()) => {
            crate::models::apply_proxy_route(&st.models_root);
            Json(proxy_view(&st.models_root, &secrets)).into_response()
        }
        Err(e) => failure(e.status, e.code, e.detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::header;
    use tower::ServiceExt;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dub-secrets-api-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn resolved(models: &Path, secrets: &Path) -> String {
        let bare = crate::models::load_selection(models)["proxy_url"].as_str().unwrap().to_string();
        crate::models::proxy_with_password(&bare, credentials::proxy_password_in(secrets).as_deref(),
        )
    }

    #[test]
    fn saving_the_proxy_form_unchanged_keeps_the_password() {
        let models = scratch("form-models");
        let secrets = scratch("form-secrets");
        apply_proxy_form(&models, &secrets, &json!({ "on": true, "url": "http://alice:hunter2@proxy.lan:3128" }),
        ).unwrap();
        let active = std::fs::read_to_string(models.join("active.json")).unwrap();
        assert!(!active.contains("hunter2"));
        assert_eq!(resolved(&models, &secrets), "http://alice:hunter2@proxy.lan:3128");

        let view = proxy_view(&models, &secrets);
        assert_eq!(
            view,
            json!({ "mode": "custom", "kind": "http", "on": true, "url": "http://alice@proxy.lan:3128", "password_set": true, "problem": null })
        );

        for untouched in [
            json!({ "on": view["on"], "url": view["url"] }),
            json!({ "on": view["on"], "url": view["url"], "password": "" }),
            json!({ "on": false }),
        ] {
            apply_proxy_form(&models, &secrets, &untouched).unwrap();
            assert_eq!(resolved(&models, &secrets), "http://alice:hunter2@proxy.lan:3128", "{untouched}");
        }
        assert_eq!(proxy_view(&models, &secrets)["on"], false);

        apply_proxy_form(&models, &secrets, &json!({ "url": "http://alice@proxy2.lan:3128", "password": "n3w" }),
        ).unwrap();
        assert_eq!(resolved(&models, &secrets), "http://alice:n3w@proxy2.lan:3128");

        apply_proxy_form(&models, &secrets, &json!({ "password": null })).unwrap();
        assert_eq!(proxy_view(&models, &secrets)["password_set"], false);
        assert_eq!(resolved(&models, &secrets), "http://alice@proxy2.lan:3128");

        let refused = apply_proxy_form(&models, &secrets, &json!({ "url": "socks5://proxy.lan:1080", "password": "x" }),
        );
        assert_eq!(refused.err().map(|e| e.code), Some("proxy_password_without_user"));

        apply_proxy_form(&models, &secrets, &json!({ "url": "http://bob:pw@proxy.lan:8080" }),
        ).unwrap();
        apply_proxy_form(&models, &secrets, &json!({ "url": "" })).unwrap();
        assert!(crate::models::load_selection(&models).get("proxy_url").is_none());
        assert_eq!(credentials::proxy_password_in(&secrets), None);

        std::fs::remove_dir_all(&models).unwrap();
        std::fs::remove_dir_all(&secrets).unwrap();
    }

    #[test]
    fn a_seller_address_is_stored_as_a_url_without_its_password() {
        let models = scratch("seller-models");
        let secrets = scratch("seller-secrets");
        apply_proxy_form(&models, &secrets, &json!({ "mode": "custom", "kind": "socks5", "url": "1.2.3.4:8000:bob:p@ss" }),
        ).unwrap();
        let active = std::fs::read_to_string(models.join("active.json")).unwrap();
        assert!(!active.contains("p@ss") && !active.contains("p%40ss"), "{active}");
        let view = proxy_view(&models, &secrets);
        assert_eq!(view["url"], "socks5h://bob@1.2.3.4:8000");
        assert_eq!((view["mode"].as_str(), view["kind"].as_str(), view["password_set"].as_bool()), (Some("custom"), Some("socks5"), Some(true)));
        assert_eq!(resolved(&models, &secrets), "socks5h://bob:p%40ss@1.2.3.4:8000");

        apply_proxy_form(&models, &secrets, &json!({ "mode": "system" })).unwrap();
        assert_eq!(proxy_view(&models, &secrets)["mode"], "system");
        assert_eq!(resolved(&models, &secrets), "socks5h://bob:p%40ss@1.2.3.4:8000", "switching the mode keeps the address");

        let bad = apply_proxy_form(&models, &secrets, &json!({ "url": "not a proxy" }));
        assert_eq!(bad.err().map(|e| e.code), Some("invalid_proxy_url"));
        let bad = apply_proxy_form(&models, &secrets, &json!({ "mode": "on" }));
        assert_eq!(bad.err().map(|e| e.code), Some("invalid_proxy_mode"));
        let bad = apply_proxy_form(&models, &secrets, &json!({ "kind": "ftp" }));
        assert_eq!(bad.err().map(|e| e.code), Some("invalid_proxy_kind"));
        apply_proxy_form(&models, &secrets, &json!({ "url": "" })).unwrap();
        let bad = apply_proxy_form(&models, &secrets, &json!({ "mode": "custom" }));
        assert_eq!(bad.err().map(|e| e.code), Some("proxy_url_required"));

        std::fs::write(models.join("active.json"), r#"{"proxy_on":"1","proxy_url":"http://carol@legacy.lan:3128"}"#,
        ).unwrap();
        apply_proxy_form(&models, &secrets, &json!({ "password": "pw" })).unwrap();
        let view = proxy_view(&models, &secrets);
        assert_eq!((view["mode"].as_str(), view["url"].as_str()), (Some("custom"), Some("http://carol@legacy.lan:3128")), "the old switch stays on");
        let bad = apply_proxy_form(&models, &secrets, &json!({ "url": "" }));
        assert_eq!(bad.err().map(|e| e.code), Some("proxy_url_required"));
        assert_eq!(credentials::proxy_password_in(&secrets).as_deref(), Some("pw"), "a refused form changes nothing");

        std::fs::write(models.join("active.json"), r#"{"proxy_mode":"custom","proxy_url":"garbage"}"#,
        ).unwrap();
        assert!(proxy_view(&models, &secrets)["problem"].as_str().is_some_and(|p| p.contains("garbage")));

        std::fs::remove_dir_all(&models).unwrap();
        std::fs::remove_dir_all(&secrets).unwrap();
    }

    #[test]
    fn the_saved_proxy_password_goes_only_to_the_saved_address() {
        let models = scratch("probe-models");
        let secrets = scratch("probe-secrets");
        apply_proxy_form(&models, &secrets, &json!({ "on": false, "url": "http://alice:hunter2@proxy.lan:3128" }),
        ).unwrap();
        let probe = |url: &str, typed: Option<&str>| {
            proxy_test_address(&models, Some(&secrets), url, typed)
        };

        assert_eq!(probe("http://alice@proxy.lan:3128", None), "http://alice:hunter2@proxy.lan:3128");
        assert_eq!(probe(" http://alice@proxy.lan:3128 ", Some("  ")), "http://alice:hunter2@proxy.lan:3128");
        for foreign in [
            "http://alice@attacker.example:3128",
            "socks5://alice@attacker.example:1080",
            "https://alice@proxy.lan:3128",
            "http://alice@proxy.lan:3129",
            "alice@proxy.lan:3128",
        ] {
            assert_eq!(probe(foreign, None), foreign, "the saved password must not reach {foreign}");
        }
        assert_eq!(probe("http://alice@attacker.example:3128", Some("typed")), "http://alice:typed@attacker.example:3128");
        assert_eq!(probe("http://alice@proxy.lan:3128", Some("typed")), "http://alice:typed@proxy.lan:3128");
        assert_eq!(probe("http://alice:own@attacker.example:3128", None), "http://alice:own@attacker.example:3128");
        assert_eq!(probe("", None), "");
        assert_eq!(proxy_test_address(&models, None, "http://alice@proxy.lan:3128", None), "http://alice@proxy.lan:3128");

        apply_proxy_form(&models, &secrets, &json!({ "url": "http://alice@proxy2.lan:3128" }),
        ).unwrap();
        assert_eq!(probe("http://alice@proxy.lan:3128", None), "http://alice@proxy.lan:3128");
        assert_eq!(probe("http://alice@proxy2.lan:3128", None), "http://alice:hunter2@proxy2.lan:3128");

        std::fs::remove_dir_all(&models).unwrap();
        std::fs::remove_dir_all(&secrets).unwrap();
    }

    #[test]
    fn a_password_with_url_delimiters_keeps_the_route() {
        let models = scratch("delimiters-models");
        let secrets = scratch("delimiters-secrets");
        for password in ["pa/ss", "pa?ss", "pa#ss", "p@ss", "pa:ss", "p@ss:1/x", "pa\\ss", "100%", "пароль",
        ] {
            let encoded = dub_llm::net::encode_userinfo(password);
            for form in [
                json!({ "mode": "custom", "url": "http://bob@1.2.3.4:8000", "password": password }),
                json!({ "mode": "custom", "url": format!("http://bob:{encoded}@1.2.3.4:8000") }),
                json!({ "mode": "custom", "url": format!("1.2.3.4:8000:bob:{password}") }),
            ] {
                if password.contains(':') && form["url"].as_str().is_some_and(|url| !url.contains("://")) {
                    continue;
                }
                apply_proxy_form(&models, &secrets, &form).unwrap_or_else(|e| panic!("{form}: {e:?}"));
                assert_eq!(credentials::proxy_password_in(&secrets).as_deref(), Some(password), "{form}");
                let view = proxy_view(&models, &secrets);
                assert_eq!((view["problem"].clone(), view["password_set"].clone()), (Value::Null, Value::Bool(true)), "{form}");
                let shown = view.to_string();
                assert!(!shown.contains(password) && !shown.contains(&encoded), "{shown}");

                let selection = crate::models::load_selection(&models);
                let address = crate::models::proxy_address(&selection, credentials::proxy_password_in(&secrets).as_deref(),
                ).unwrap();
                let settings = dub_llm::net::ProxySettings {
                    mode: dub_llm::net::ProxyMode::Custom,
                    address: Some(address.clone()),
                    kind: crate::models::proxy_kind(&selection),
                };
                let through = settings.custom_url().unwrap_or_else(|| panic!("{form}: the route went direct"));
                assert_eq!((through.host_str(), through.port()), (Some("1.2.3.4"), Some(8000)), "{form}");
                assert_eq!(dub_llm::net::decode_userinfo(through.password().unwrap()), password);
                let logged = dub_llm::net::masked(through.as_str());
                assert!(!logged.contains(&encoded) && !logged.contains(password), "{logged}");
                let probe = proxy_test_address(&models, Some(&secrets), view["url"].as_str().unwrap(), None,
                );
                assert_eq!(probe, address, "the test probes the saved route");
            }
        }
        std::fs::remove_dir_all(&models).unwrap();
        std::fs::remove_dir_all(&secrets).unwrap();
    }

    #[test]
    fn the_proxy_test_hands_the_password_over_as_is_and_never_answers_with_it() {
        let proxy = dub_llm::test_http::serve(vec![
            dub_llm::test_http::Reply::json(407, "{}"),
            dub_llm::test_http::Reply::json(407, "{}"),
        ]);
        let models = scratch("probe-real-models");
        let secrets = scratch("probe-real-secrets");
        let saved = proxy.base().replace("http://", "http://bob@");
        apply_proxy_form(&models, &secrets, &json!({ "mode": "custom", "url": saved, "password": "p@ss:1/x" }),
        ).unwrap();
        let address = proxy_test_address(&models, Some(&secrets), &saved, None);
        let settings = dub_llm::net::ProxySettings {
            mode: dub_llm::net::ProxyMode::Custom,
            address: Some(address),
            kind: dub_llm::net::ProxyKind::Http,
        };
        let answer = dub_llm::net::test(settings).unwrap();
        assert_eq!((answer["huggingface"].as_bool(), answer["openrouter"].as_bool()), (Some(false), Some(false)), "{answer}");
        assert!(answer["huggingface_error"].as_str().is_some_and(|e| !e.is_empty()), "{answer}");
        let text = answer.to_string();
        assert!(!text.contains("p@ss") && !text.contains("p%40ss"), "{text}");
        for index in 0..2 {
            let request = proxy.request(index).to_ascii_lowercase();
            assert!(request.starts_with("connect "), "{request}");
            assert!(request.contains("proxy-authorization: basic ym9ionbac3m6ms94"), "bob:p@ss:1/x as is: {request}");
        }
        std::fs::remove_dir_all(&models).unwrap();
        std::fs::remove_dir_all(&secrets).unwrap();
    }

    async fn body_text(app: &axum::Router, request: Request) -> (StatusCode, String) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    fn local(method: &str, uri: &str, body: Option<Value>) -> Request {
        let builder = Request::builder().method(method).uri(uri).header(header::HOST, "127.0.0.1:8793");
        match body {
            Some(body) => builder.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())).unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    #[tokio::test]
    async fn no_response_carries_a_secret() {
        let root = scratch("leak-root");
        std::fs::create_dir_all(root.join("models")).unwrap();
        std::fs::write(
            root.join("models").join("active.json"),
            r#"{"or_key":"sk-or-v1-leaktest","proxy_on":"1","proxy_url":"http://alice:hunter2@proxy.lan:3128","bench":"1"}"#,
        )
        .unwrap();
        let app = crate::build_router(crate::AppState::new(&root));

        let on_disk = std::fs::read_to_string(root.join("models").join("active.json")).unwrap();
        assert!(!on_disk.contains("sk-or-v1-leaktest") && !on_disk.contains("hunter2"), "migrated on start: {on_disk}");

        let (status, capabilities) = body_text(&app, local("GET", "/engine/capabilities", None)).await;
        assert_eq!(status, StatusCode::OK);
        let parsed: Value = serde_json::from_str(&capabilities).unwrap();
        assert_eq!(parsed["selection"]["or_key_set"], true);
        assert_eq!(parsed["selection"]["proxy_password_set"], true);
        assert_eq!(parsed["selection"]["proxy_url"], "http://alice@proxy.lan:3128");

        let (status, selected) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "bench", "value": "0" })),
            ),
        ).await;
        assert_eq!(status, StatusCode::OK);
        let (status, by_component) = body_text(&app, local("POST", "/engine/select", Some(json!({ "id": "higgs-q6_k" })),
            ),
        ).await;
        assert_eq!(status, StatusCode::OK);
        let (_, openrouter) = body_text(&app, local("GET", "/engine/openrouter/settings", None)).await;
        let (_, proxy) = body_text(&app, local("GET", "/engine/proxy/settings", None)).await;
        let (status, server_key) = body_text(&app, local("PUT", "/engine/server/key", Some(json!({ "api_key": "lm-leaktest" })),
            ),
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(serde_json::from_str::<Value>(&server_key).unwrap()["configured"], true);
        let (_, capabilities_again) = body_text(&app, local("GET", "/engine/capabilities", None)).await;
        for answer in [&capabilities, &selected, &by_component, &openrouter, &proxy, &server_key, &capabilities_again,
        ] {
            assert!(!answer.contains("lm-leaktest"), "{answer}");
            assert!(!answer.contains("sk-or-v1-leaktest") && !answer.contains("hunter2"), "{answer}");
        }
        assert_eq!(serde_json::from_str::<Value>(&openrouter).unwrap()["configured"], true);
        assert_eq!(serde_json::from_str::<Value>(&proxy).unwrap()["password_set"], true);

        let (status, _) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "or_key", "value": "sk-x" })),
            ),
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "proxy_url", "value": "http://a:b@h:1" })),
            ),
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "srv_key", "value": "x" })),
            ),
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = body_text(&app, local("DELETE", "/engine/server/key", None)).await;
        assert_eq!(status, StatusCode::OK);
        let (status, empty) = body_text(&app, local("PUT", "/engine/openrouter/settings", Some(json!({ "api_key": " " })),
            ),
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(empty.contains("empty_key"));

        if std::env::var(credentials::OPENROUTER_ENV_VAR).is_err() {
            let (status, removed) = body_text(&app, local("DELETE", "/engine/openrouter/settings", None)).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(serde_json::from_str::<Value>(&removed).unwrap()["configured"], false);
        }

        std::fs::remove_dir_all(&root).unwrap();
        if let Some(dir) = credentials::secrets_dir() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
