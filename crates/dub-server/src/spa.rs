//! Раздача собранного SPA (frontend/dist) с fallback на index.html — как хвост app.py.
//! ОБЯЗАТЕЛЬНАЯ защита от path-traversal: реальный (канонизированный) путь запроса должен лежать
//! ВНУТРИ web-root, иначе `..%2f`-сегменты вычитали бы произвольные файлы. Точно как чинили в app.py:
//! `f == web_root or web_root in f.parents`.

use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::{Path, PathBuf};

/// Отдать статик-файл SPA по пути или index.html (deep-link/refresh). None -> web-root не собран.
pub async fn serve_spa(web_root: Option<&Path>, spa_path: &str) -> Response {
    let Some(web_root) = web_root else {
        return (StatusCode::NOT_FOUND, t!("spa-not-built")).into_response();
    };
    let Ok(root_canon) = web_root.canonicalize() else {
        return (StatusCode::NOT_FOUND, t!("spa-not-built")).into_response();
    };

    if !spa_path.is_empty() {
        let candidate = web_root.join(spa_path);
        // Канонизируем: раскрывает `..` и симлинки. Файл обязан существовать И быть внутри root.
        if let Ok(canon) = candidate.canonicalize() {
            let contained = canon.starts_with(&root_canon); // starts_with(self)==true, равенство покрыто
            if contained && canon.is_file() {
                return file_response(&canon).await;
            }
        }
    }
    // deep-link / refresh / неизвестный путь -> SPA entry (без 404).
    file_response(&root_canon.join("index.html")).await
}

async fn file_response(path: &Path) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                Body::from(bytes),
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Найти frontend/dist относительно рабочего каталога/корня репо. None если не собран.
pub fn find_web_root(repo_root: &Path) -> Option<PathBuf> {
    let dist = repo_root.join("frontend").join("dist");
    if dist.join("index.html").is_file() {
        Some(dist)
    } else {
        None
    }
}
