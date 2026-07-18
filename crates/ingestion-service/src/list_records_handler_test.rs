use super::*;
use crate::event_publisher::event_publisher_test::InMemoryEventPublisher;
use crate::ingest_handler::IngestState;
use crate::raw_record_repository::raw_record_repository_test::{
    FailingRawRecordRepository, InMemoryRawRecordRepository,
};
use crate::raw_record_repository::RawRecordRepository;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use common::SourceType;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: IngestState) -> Router {
    Router::new().route("/v1/records", get(list_records)).with_state(state)
}

fn record_for_tenant_ingested_at(tenant_id: Uuid, ingested_at: DateTime<Utc>) -> RawRecord {
    let mut record =
        RawRecord::new("zendesk", SourceType::Ticket, tenant_id, serde_json::json!({}));
    record.ingested_at = ingested_at;
    record
}

#[tokio::test]
async fn returns_only_records_older_than_the_cutoff_for_the_requesting_tenant() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let old = record_for_tenant_ingested_at(tenant_id, now - chrono::Duration::days(10));
    let recent = record_for_tenant_ingested_at(tenant_id, now);
    let someone_elses_old =
        record_for_tenant_ingested_at(Uuid::new_v4(), now - chrono::Duration::days(10));
    repository.insert(&old).await.unwrap();
    repository.insert(&recent).await.unwrap();
    repository.insert(&someone_elses_old).await.unwrap();
    let state = IngestState { repository, publisher };

    let cutoff = now - chrono::Duration::days(5);
    let encoded_cutoff = cutoff.to_rfc3339().replace('+', "%2B");
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/records?older_than={encoded_cutoff}&limit=10"))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: Vec<RawRecord> = serde_json::from_slice(&body).unwrap();
    assert_eq!(found, vec![old]);
}

#[tokio::test]
async fn missing_tenant_header_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let encoded_cutoff = Utc::now().to_rfc3339().replace('+', "%2B");
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/records?older_than={encoded_cutoff}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn missing_older_than_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records")
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn repository_failure_returns_500() {
    let repository = Arc::new(FailingRawRecordRepository);
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let encoded_cutoff = Utc::now().to_rfc3339().replace('+', "%2B");
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/records?older_than={encoded_cutoff}"))
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
