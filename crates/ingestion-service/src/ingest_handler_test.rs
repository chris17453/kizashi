use super::*;
use crate::event_publisher::event_publisher_test::{FailingEventPublisher, InMemoryEventPublisher};
use crate::raw_record_repository::raw_record_repository_test::{
    FailingRawRecordRepository, InMemoryRawRecordRepository,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use tower::ServiceExt;

fn router(state: IngestState) -> Router {
    Router::new().route("/v1/records", post(ingest_record)).with_state(state)
}

fn valid_body() -> serde_json::Value {
    serde_json::json!({
        "connector_id": "zendesk",
        "source_type": "ticket",
        "tenant_id": Uuid::new_v4(),
        "raw_payload": {"subject": "help"},
    })
}

async fn post_json(app: Router, body: serde_json::Value) -> axum::http::Response<Body> {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/v1/records")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn valid_record_is_persisted_and_published_returns_201() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository: repository.clone(), publisher: publisher.clone() };

    let response = post_json(router(state), valid_body()).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(repository.records.lock().unwrap().len(), 1);
    assert_eq!(publisher.published.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn empty_connector_id_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository: repository.clone(), publisher };

    let mut body = valid_body();
    body["connector_id"] = serde_json::json!("");

    let response = post_json(router(state), body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(repository.records.lock().unwrap().is_empty());
}

#[tokio::test]
async fn nil_tenant_id_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let mut body = valid_body();
    body["tenant_id"] = serde_json::json!(Uuid::nil());

    let response = post_json(router(state), body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn null_raw_payload_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let mut body = valid_body();
    body["raw_payload"] = serde_json::Value::Null;

    let response = post_json(router(state), body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn repository_failure_returns_500_and_does_not_publish() {
    let repository = Arc::new(FailingRawRecordRepository);
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher: publisher.clone() };

    let response = post_json(router(state), valid_body()).await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(publisher.published.lock().unwrap().is_empty());
}

#[tokio::test]
async fn publish_failure_still_returns_201_since_record_is_durably_stored() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(FailingEventPublisher);
    let state = IngestState { repository: repository.clone(), publisher };

    let response = post_json(router(state), valid_body()).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(repository.records.lock().unwrap().len(), 1);
}
