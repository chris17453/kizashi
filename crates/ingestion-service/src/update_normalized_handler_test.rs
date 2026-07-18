use super::*;
use crate::event_publisher::event_publisher_test::InMemoryEventPublisher;
use crate::raw_record_repository::raw_record_repository_test::InMemoryRawRecordRepository;
use crate::raw_record_repository::RawRecordRepository;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::patch;
use axum::Router;
use common::RawRecord;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: IngestState) -> Router {
    Router::new()
        .route("/v1/records/:id/normalized", patch(update_normalized_payload))
        .with_state(state)
}

#[tokio::test]
async fn updates_normalized_payload_and_returns_204_for_a_known_record() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        Uuid::new_v4(),
        serde_json::json!({}),
    );
    repository.insert(&record).await.unwrap();
    let state = IngestState {
        repository: repository.clone(),
        publisher: Arc::new(InMemoryEventPublisher::default()),
    };

    let body = serde_json::json!({"normalized_payload": {"text": "hi"}});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/records/{}/normalized", record.id))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        repository.records.lock().unwrap()[0].normalized_payload,
        Some(serde_json::json!({"text": "hi"}))
    );
}

#[tokio::test]
async fn returns_404_for_unknown_record() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let state = IngestState { repository, publisher: Arc::new(InMemoryEventPublisher::default()) };

    let body = serde_json::json!({"normalized_payload": {}});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/records/{}/normalized", Uuid::new_v4()))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
