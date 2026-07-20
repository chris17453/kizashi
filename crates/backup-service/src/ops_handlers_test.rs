use super::*;
use crate::backup_run_repository::backup_run_repository_test::InMemoryBackupRunRepository;
use crate::backup_store::backup_store_test::InMemoryBackupStore;
use crate::pg_dump_runner::pg_dump_runner_test::InMemoryPgDumpRunner;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/backup/run", post(trigger_backup))
        .route("/v1/backup/status", get(get_backup_status))
        .with_state(state)
}

fn sample_state() -> AppState {
    AppState {
        run_repository: Arc::new(InMemoryBackupRunRepository::default()),
        store: Arc::new(InMemoryBackupStore::default()),
        dump_runner: Arc::new(InMemoryPgDumpRunner { bytes: vec![1, 2, 3] }),
        internal_secret: "test-secret".to_string(),
    }
}

#[tokio::test]
async fn trigger_backup_runs_a_backup_and_returns_the_outcome() {
    let state = sample_state();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/backup/run")
                .header("x-internal-secret", "test-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], "success");
}

#[tokio::test]
async fn trigger_backup_rejects_a_missing_internal_secret() {
    let state = sample_state();

    let response = router(state)
        .oneshot(
            Request::builder().method("POST").uri("/v1/backup/run").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_backup_status_returns_recent_runs_for_an_admin() {
    let state = sample_state();
    state
        .run_repository
        .start(Uuid::new_v4(), chrono::Utc::now(), "postgres/test.dump")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/backup/status")
                .header("x-internal-secret", "test-secret")
                .header("x-role", "admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_backup_status_is_forbidden_for_a_non_admin_role() {
    let state = sample_state();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/backup/status")
                .header("x-internal-secret", "test-secret")
                .header("x-role", "operator")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_backup_status_rejects_a_missing_internal_secret() {
    let state = sample_state();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/backup/status")
                .header("x-role", "admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
