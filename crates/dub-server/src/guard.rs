//! Кто может управлять студией: только её окно и агенты этого компьютера. Проверка стоит на всём роутере
//! (API, SPA и всё, что появится), а не на отдельных ручках: простые запросы без preflight (multipart,
//! form-POST) выполняются при любом CORS, браузер лишь не отдаёт ответ странице.

use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

const LOCAL_HOSTS: [&str; 4] = ["localhost", "127.0.0.1", "[::1]", "tauri.localhost"];

/// Only the studio's own page and local agents may drive it: a web page in
/// the user's browser, or one rebinding a domain to this computer, sends its
/// own origin and is refused.
pub(crate) fn local_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else { return true };
    local_origin_value(origin)
}

fn local_origin_value(origin: &HeaderValue) -> bool {
    let Ok(origin) = origin.to_str() else { return false };
    let Some((scheme, rest)) = origin.split_once("://") else { return false };
    let host = rest.split('/').next().unwrap_or_default();
    let host = if host.starts_with('[') { host.split(']').next().map(|name| format!("{name}]")).unwrap_or_default() } else { host.split(':').next().unwrap_or_default().to_string() };
    scheme == "tauri" || LOCAL_HOSTS.contains(&host.as_str())
}

pub(crate) fn foreign_origin() -> Response {
    (StatusCode::FORBIDDEN, "This studio answers only its own window and agents on this computer.").into_response()
}

/// DNS-rebinding: страница чужого домена, перепривязанного на 127.0.0.1, шлёт свой Host. Порт в Host браузер
/// берёт из того же URL, по которому подключился, поэтому решает имя хоста.
fn local_host(request: &Request) -> bool {
    let host = request
        .headers()
        .get(header::HOST)
        .map(|host| host.to_str().ok())
        .unwrap_or_else(|| request.uri().authority().map(|authority| authority.as_str()));
    let Some(host) = host else { return false };
    let host = host.rsplit('@').next().unwrap_or_default();
    let name = if host.starts_with('[') {
        host.split(']').next().map(|name| format!("{name}]")).unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or_default().to_string()
    };
    LOCAL_HOSTS.iter().any(|local| local.eq_ignore_ascii_case(&name))
}

/// Middleware на весь роутер: чужой Origin или чужой Host -> 403 до любой ручки.
pub(crate) async fn origin_guard(request: Request, next: Next) -> Response {
    if !local_origin(request.headers()) || !local_host(&request) {
        let shown = |name: header::HeaderName| {
            request.headers().get(name).map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned()).unwrap_or_default()
        };
        tracing::warn!(
            "rejected request {} {}: Origin={:?} Host={:?}",
            request.method(),
            request.uri().path(),
            shown(header::ORIGIN),
            shown(header::HOST)
        );
        return foreign_origin();
    }
    next.run(request).await
}

/// CORS отвечает только локальным источникам (dev-фронт Vite на localhost:<порт>, окно студии).
pub(crate) fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| local_origin_value(origin)))
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([header::HeaderName::from_static(crate::mcp::REV_HEADER)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn only_local_pages_and_agents_may_drive_the_studio() {
        let from = |origin: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            local_origin(&headers)
        };
        assert!(local_origin(&HeaderMap::new()), "an agent sends no origin");
        assert!(from("http://127.0.0.1:3791") && from("http://localhost") && from("http://tauri.localhost") && from("tauri://localhost") && from("http://[::1]:8791"));
        assert!(!from("https://example.com") && !from("http://127.0.0.1.evil.com") && !from("null"));
    }

    #[test]
    fn only_a_local_host_name_is_served() {
        let host = |value: Option<&str>| {
            let mut builder = Request::builder().uri("/engine/capabilities");
            if let Some(value) = value {
                builder = builder.header(header::HOST, value);
            }
            local_host(&builder.body(Body::empty()).unwrap())
        };
        assert!(host(Some("127.0.0.1:8793")) && host(Some("localhost:5173")) && host(Some("[::1]:8793")));
        assert!(host(Some("tauri.localhost")) && host(Some("LOCALHOST:8793")) && host(Some("127.0.0.1")));
        assert!(!host(Some("evil.example:8793")) && !host(Some("127.0.0.1.evil.com:8793")) && !host(Some("[::2]:8793")));
        assert!(!host(None), "a request without any host is refused");
        let absolute = Request::builder().uri("http://localhost:8793/x").body(Body::empty()).unwrap();
        assert!(local_host(&absolute), "HTTP/2 carries the host in the authority");
    }

    fn guarded() -> Router {
        Router::new()
            .route("/probe", get(|| async { "ok" }).post(|| async { "posted" }))
            .layer(cors())
            .layer(axum::middleware::from_fn(origin_guard))
    }

    async fn status(request: Request) -> (StatusCode, Option<HeaderValue>) {
        let response = guarded().oneshot(request).await.unwrap();
        (response.status(), response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).cloned())
    }

    #[tokio::test]
    async fn the_guard_refuses_foreign_pages_before_any_handler() {
        let request = |origin: Option<&str>, host: &str| {
            let mut builder = Request::builder().method("POST").uri("/probe").header(header::HOST, host);
            if let Some(origin) = origin {
                builder = builder.header(header::ORIGIN, origin);
            }
            builder.body(Body::empty()).unwrap()
        };
        assert_eq!(status(request(Some("https://evil.example"), "127.0.0.1:8793")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(status(request(None, "evil.example:8793")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(status(request(Some("http://evil.example:8793"), "evil.example:8793")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(status(request(None, "127.0.0.1:8793")).await.0, StatusCode::OK);

        let (code, allowed) = status(request(Some("http://localhost:5173"), "127.0.0.1:8793")).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(allowed.as_ref().and_then(|v| v.to_str().ok()), Some("http://localhost:5173"));
    }

    #[tokio::test]
    async fn only_local_pages_get_a_cors_preflight() {
        let preflight = |origin: &str| {
            Request::builder()
                .method("OPTIONS")
                .uri("/probe")
                .header(header::HOST, "127.0.0.1:8793")
                .header(header::ORIGIN, origin)
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "DELETE")
                .body(Body::empty())
                .unwrap()
        };
        let (code, allowed) = status(preflight("http://127.0.0.1:5178")).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(allowed.as_ref().and_then(|v| v.to_str().ok()), Some("http://127.0.0.1:5178"));
        let (code, allowed) = status(preflight("https://evil.example")).await;
        assert_eq!(code, StatusCode::FORBIDDEN);
        assert!(allowed.is_none());
    }

    #[tokio::test]
    async fn the_real_router_is_guarded_for_api_and_spa() {
        let root = std::env::temp_dir().join(format!("dub-guard-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let app = crate::build_router(crate::AppState::new(&root));
        for path in ["/engine/capabilities", "/", "/index.html"] {
            let foreign = Request::builder()
                .uri(path)
                .header(header::HOST, "127.0.0.1:8793")
                .header(header::ORIGIN, "https://evil.example")
                .body(Body::empty())
                .unwrap();
            assert_eq!(app.clone().oneshot(foreign).await.unwrap().status(), StatusCode::FORBIDDEN, "{path}");
            let rebound = Request::builder().uri(path).header(header::HOST, "evil.example:8793").body(Body::empty()).unwrap();
            assert_eq!(app.clone().oneshot(rebound).await.unwrap().status(), StatusCode::FORBIDDEN, "{path}");
        }
        let agent = Request::builder().uri("/engine/capabilities").header(header::HOST, "127.0.0.1:8793").body(Body::empty()).unwrap();
        assert_eq!(app.oneshot(agent).await.unwrap().status(), StatusCode::OK);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
