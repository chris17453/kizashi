use super::*;
use crate::event_publisher::event_publisher_test::InMemoryEventPublisher;
use crate::raw_record_repository::raw_record_repository_test::{
    FailingRawRecordRepository, InMemoryRawRecordRepository,
};
use crate::raw_record_repository::RawRecordRepository;
use axum::body::Body;
use axum::http::Request;
use axum::routing::delete;
use axum::Router;
use common::{RawRecord, SourceType};
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: IngestState) -> Router {
    Router::new().route("/v1/records/:id", delete(delete_record)).with_state(state)
}

#[tokio::test]
async fn deletes_a_known_record_and_returns_204() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let record =
        RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), serde_json::json!({}));
    repository.insert(&record).await.unwrap();
    let state = IngestState { repository: repository.clone(), publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/records/{}", record.id))
                .header("x-tenant-id", record.tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(repository.records.lock().unwrap().is_empty());
}

#[tokio::test]
async fn wrong_tenant_cannot_delete_returns_404_and_leaves_the_record() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let record =
        RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), serde_json::json!({}));
    repository.insert(&record).await.unwrap();
    let state = IngestState { repository: repository.clone(), publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/records/{}", record.id))
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(repository.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn missing_tenant_header_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/records/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_record_returns_404() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/records/{}", Uuid::new_v4()))
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn repository_failure_returns_500() {
    let repository = Arc::new(FailingRawRecordRepository);
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/records/{}", Uuid::new_v4()))
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
