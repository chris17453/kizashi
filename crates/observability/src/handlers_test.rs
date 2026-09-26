use super::*;
use crate::backlog::backlog_test::{FailingBacklogReader, InMemoryBacklogReader};
use crate::backlog::QueueDepth;
use crate::platform_health::platform_health_test::InMemoryServiceHealthChecker;
use crate::platform_health::Status;
use crate::service_registry::ServiceEndpoint;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(get_platform_health))
        .route("/v1/service-metrics", get(get_service_metrics))
        .route("/v1/backlog", get(get_backlog))
        .route("/metrics", get(get_metrics))
        .with_state(state)
}

fn default_state() -> AppState {
    AppState {
        health_checker: Arc::new(InMemoryServiceHealthChecker::default()),
        registry: Arc::new(vec![]),
        backlog_reader: Arc::new(InMemoryBacklogReader::default()),
    }
}

#[tokio::test]
async fn health_returns_200_when_platform_is_up() {
    let checker = InMemoryServiceHealthChecker::default();
    checker.statuses.lock().unwrap().insert("a".to_string(), Status::Up);
    let state = AppState {
        health_checker: Arc::new(checker),
        registry: Arc::new(vec![ServiceEndpoint {
            name: "a".to_string(),
            url: "http://a".to_string(),
        }]),
        backlog_reader: Arc::new(InMemoryBacklogReader::default()),
    };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_returns_503_when_any_service_is_down() {
    let checker = InMemoryServiceHealthChecker::default();
    checker.statuses.lock().unwrap().insert("a".to_string(), Status::Down);
    let state = AppState {
        health_checker: Arc::new(checker),
        registry: Arc::new(vec![ServiceEndpoint {
            name: "a".to_string(),
            url: "http://a".to_string(),
        }]),
        backlog_reader: Arc::new(InMemoryBacklogReader::default()),
    };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn backlog_returns_configured_queue_depths() {
    let reader = InMemoryBacklogReader::default();
    reader.depths.lock().unwrap().push(QueueDepth {
        stage: "ingest_to_normalize".to_string(),
        queue_name: "normalization-service.record.ingested".to_string(),
        messages: 4,
    });
    let mut state = default_state();
    state.backlog_reader = Arc::new(reader);

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/backlog").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let depths: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(depths.len(), 1);
    assert_eq!(depths[0]["messages"], 4);
}

#[tokio::test]
async fn backlog_returns_500_on_backend_failure() {
    let mut state = default_state();
    state.backlog_reader = Arc::new(FailingBacklogReader);

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/backlog").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn metrics_exposes_platform_service_and_queue_gauges() {
    let checker = InMemoryServiceHealthChecker::default();
    checker.statuses.lock().unwrap().insert("api".to_string(), Status::Up);
    let reader = InMemoryBacklogReader::default();
    reader.depths.lock().unwrap().push(QueueDepth {
        stage: "ingest_to_normalize".to_string(),
        queue_name: "records".to_string(),
        messages: 7,
    });
    let state = AppState {
        health_checker: Arc::new(checker),
        registry: Arc::new(vec![ServiceEndpoint {
            name: "api".to_string(),
            url: "http://api".to_string(),
        }]),
        backlog_reader: Arc::new(reader),
    };
    let response = router(state)
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[axum::http::header::CONTENT_TYPE],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    let body = String::from_utf8(
        axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec(),
    )
    .unwrap();
    assert!(body.contains("kizashi_platform_up 1"));
    assert!(body.contains("kizashi_service_up{service=\"api\"} 1"));
    assert!(
        body.contains("kizashi_queue_messages{stage=\"ingest_to_normalize\",queue=\"records\"} 7")
    );
}
