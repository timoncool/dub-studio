//! HTTP-контракт джоб: прерванная джоба проекта видна в списке, «Продолжить» ставит тот же вид с теми
//! же аргументами на тот же проект, история отдаёт снапшот после завершения.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use dub_server::{build_router, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tower::ServiceExt;

const PID: &str = "abc123def456";

fn fixture_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("dub_jobs_api_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("workspace").join(PID);
    std::fs::create_dir_all(&dir).unwrap();
    let project = json!({
        "mode": "transcribe",
        "tgt_lang": "en",
        "segments": [{ "id": "s0", "start": 0.0, "end": 1.0, "speaker": "0", "src_text": "hello", "tgt_text": "" }]
    });
    std::fs::write(dir.join("project.json"), project.to_string()).unwrap();
    // Джоба шла, когда приложение закрылось.
    let job = json!({
        "kind": "retranslate",
        "args": { "lang": "ru", "mode": "nodub" },
        "state": "running",
        "stage": "translate",
        "job_id": "old",
        "started_at": 1,
        "updated_at": 1
    });
    std::fs::write(dir.join("job.json"), job.to_string()).unwrap();
    root
}

async fn call(app: &Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(uri).header("host", "127.0.0.1:8793").body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, body)
}

fn job_file(root: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(root.join("workspace").join(PID).join("job.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn interrupted_job_is_listed_and_resumes_with_same_kind_and_args() {
    let root = fixture_root("resume");
    let app = build_router(AppState::new(&root));

    let (st, list) = call(&app, "GET", "/projects").await;
    assert_eq!(st, StatusCode::OK);
    let item = list["projects"].as_array().unwrap().iter().find(|p| p["pid"] == PID).unwrap().clone();
    assert_eq!(item["job_state"], "interrupted");
    assert_eq!(item["job_kind"], "retranslate");
    assert_eq!(item["job_stage"], "translate");
    assert_eq!(item["job_error"]["code"], "interrupted");

    let (st, resumed) = call(&app, "POST", &format!("/projects/{PID}/resume")).await;
    assert_eq!(st, StatusCode::OK, "{resumed}");
    assert_eq!(resumed["kind"], "retranslate");
    let job_id = resumed["job_id"].as_str().unwrap().to_string();

    let rec = job_file(&root);
    assert_eq!(rec["kind"], "retranslate");
    assert_eq!(rec["args"], json!({ "lang": "ru", "mode": "nodub" }));
    assert_eq!(rec["job_id"], job_id.as_str());
    assert_eq!(rec["resumes"], 1);

    let (st, jobs) = call(&app, "GET", &format!("/jobs?pid={PID}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(jobs["jobs"][0]["id"], job_id.as_str());
    assert_eq!(jobs["jobs"][0]["kind"], "retranslate");
    assert_eq!(jobs["project_job"]["kind"], "retranslate");

    // Long-poll до конца: без LLM перевод не выполнен — джоба падает с причиной, проект не меняется.
    let (st, snap) = call(&app, "GET", &format!("/jobs/{job_id}?wait=30")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(snap["status"], "error", "{snap}");
    assert_eq!(snap["pid"], PID);
    assert!(snap["error"].as_str().is_some_and(|e| e.contains("LLM недоступен")), "{snap}");
    // Снапшот живёт в истории: второй читатель тоже его видит.
    let (_, again) = call(&app, "GET", &format!("/jobs/{job_id}")).await;
    assert_eq!(again["status"], "error");
    assert_eq!(job_file(&root)["state"], "failed");
    let (_, project) = call(&app, "GET", &format!("/projects/{PID}")).await;
    assert_eq!(project["mode"], "transcribe");

    let (st, _) = call(&app, "POST", &format!("/jobs/{job_id}/cancel")).await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = call(&app, "GET", "/jobs/nosuchjob000").await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_finished_job_has_nothing_to_resume() {
    let root = fixture_root("done");
    let mut job = job_file(&root);
    job["state"] = json!("done");
    std::fs::write(root.join("workspace").join(PID).join("job.json"), job.to_string()).unwrap();
    let app = build_router(AppState::new(&root));

    let (st, body) = call(&app, "POST", &format!("/projects/{PID}/resume")).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["error"], "nothing_to_resume");

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn analyze_rejects_malformed_post_options_before_queueing() {
    let root = fixture_root("post");
    let dir = root.join("workspace").join(PID);
    let src = dir.join("source.mp4");
    std::fs::write(&src, b"not a video").unwrap();
    std::fs::write(dir.join("source.txt"), src.to_string_lossy().as_bytes()).unwrap();
    let app = build_router(AppState::new(&root));

    for q in ["vo_gain=loud", "sub_blur=yes", "keep_original=1&container=avi", "voice_slots=%5B1%5D", "speaker_count=-1", "speaker_count=9", "speaker_count=1.5", "speaker_count=eight"] {
        let (st, body) = call(&app, "POST", &format!("/projects/{PID}/analyze?mode=dub&{q}")).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{q}: {body}");
    }
    // Кривые настройки не ставят джобу и не переписывают job.json прошлой джобы.
    assert_eq!(job_file(&root)["job_id"], "old");
    let (_, jobs) = call(&app, "GET", &format!("/jobs?pid={PID}")).await;
    assert_eq!(jobs["jobs"], json!([]));

    let _ = std::fs::remove_dir_all(&root);
}
