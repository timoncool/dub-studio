//! HTTP-контракт «Сделать голос»: отказ приходит кодом, по которому окно пишет причину, а не пустотой.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use dub_server::{build_router, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tower::ServiceExt;

const PID: &str = "abc123def456";

fn fixture_root() -> PathBuf {
    dub_server::forbid_downloads();
    let root = std::env::temp_dir().join(format!("dub_speaker_voice_api_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("workspace").join(PID);
    std::fs::create_dir_all(&dir).unwrap();
    let project = json!({
        "mode": "dub",
        "tgt_lang": "en",
        "segments": [{ "id": "s0", "start": 0.0, "end": 2.0, "speaker": "0", "src_text": "hello", "tgt_text": "" }]
    });
    std::fs::write(dir.join("project.json"), project.to_string()).unwrap();
    std::fs::create_dir_all(root.join("models")).unwrap();
    std::fs::write(root.join("models").join("active.json"), json!({ "sep_backend": "cpu" }).to_string()).unwrap();
    root
}

async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", "127.0.0.1:8793")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, body)
}

#[tokio::test]
async fn speaker_voice_refusals_come_as_codes() {
    let root = fixture_root();
    let app = build_router(AppState::new(&root));
    let uri = format!("/projects/{PID}/speaker-voice");

    let (st, body) = post(&app, &uri, json!({ "speaker": "0", "name": "v" })).await;
    assert_eq!(st, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "no_separation", "{body}");
    let missing = body["detail"].as_str().unwrap();
    assert!(missing.contains("bsroformer-cpu"), "движок той сборки, что выбрана для стадии: {missing}");
    assert!(!root.join("voices").join("v.wav").exists());
    assert!(!root.join("workspace").join(PID).join("_voicecut").exists());

    let (st, body) = post(&app, &uri, json!({ "speaker": "7", "name": "v" })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body, json!({ "error": "no_speaker_lines", "detail": "7" }));

    let _ = std::fs::remove_dir_all(&root);
}
