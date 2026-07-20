use super::*;
use crate::dead_letter::dead_letter_test::{FailingDeadLetterManager, InMemoryDeadLetterManager};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use tower::ServiceExt;

const SECRET: &str = "test-secret";

fn router(state: DeadLetterState) -> Router {
    Router::new()
        .route("/v1/dead-letter", get(get_dead_letter_count))
        .route("/v1/dead-letter/replay", post(post_dead_letter_replay))
        .with_state(state)
}

fn state(manager: Arc<dyn DeadLetterManager>) -> DeadLetterState {
    DeadLetterState { dead_letter_manager: manager, internal_secret: SECRET.to_string() }
}

#[tokio::test]
async fn count_returns_the_current_queue_depth() {
    let manager = Arc::new(InMemoryDeadLetterManager::default());
    manager.queue.lock().unwrap().push(b"one".to_vec());

    let response = router(state(manager))
        .oneshot(
            Request::builder()
                .uri("/v1/dead-letter")
                .header("x-internal-secret", SECRET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["count"], 1);
}

#[tokio::test]
async fn count_requires_the_internal_secret() {
    let response = router(state(Arc::new(InMemoryDeadLetterManager::default())))
        .oneshot(Request::builder().uri("/v1/dead-letter").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn count_returns_500_without_leaking_the_raw_error_on_backend_failure() {
    let response = router(state(Arc::new(FailingDeadLetterManager)))
        .oneshot(
            Request::builder()
                .uri("/v1/dead-letter")
                .header("x-internal-secret", SECRET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}

#[tokio::test]
async fn replay_removes_the_oldest_message_and_reports_true() {
    let manager = Arc::new(InMemoryDeadLetterManager::default());
    manager.queue.lock().unwrap().push(b"one".to_vec());

    let response = router(state(manager.clone()))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/dead-letter/replay")
                .header("x-internal-secret", SECRET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["replayed"], true);
    assert_eq!(manager.count().await.unwrap(), 0);
}

#[tokio::test]
async fn replay_reports_false_when_the_queue_is_empty() {
    let response = router(state(Arc::new(InMemoryDeadLetterManager::default())))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/dead-letter/replay")
                .header("x-internal-secret", SECRET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["replayed"], false);
}

#[tokio::test]
async fn replay_requires_the_internal_secret() {
    let response = router(state(Arc::new(InMemoryDeadLetterManager::default())))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/dead-letter/replay")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
