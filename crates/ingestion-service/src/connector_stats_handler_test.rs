use super::*;
use crate::event_publisher::event_publisher_test::InMemoryEventPublisher;
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
use uuid::Uuid;

fn router(state: IngestState) -> Router {
    Router::new()
        .route("/v1/records/stats", get(get_connector_stats))
        .route("/v1/records/by-connector", get(list_records_by_connector))
        .with_state(state)
}

fn record(tenant_id: Uuid, connector_id: &str) -> RawRecord {
    RawRecord::new(connector_id, SourceType::Ticket, tenant_id, serde_json::json!({}))
}

#[tokio::test]
async fn get_connector_stats_returns_aggregates_for_the_requesting_tenant() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    repository.insert(&record(tenant_id, "zendesk")).await.unwrap();
    repository.insert(&record(tenant_id, "zendesk")).await.unwrap();
    repository.insert(&record(Uuid::new_v4(), "zendesk")).await.unwrap();
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/stats")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let stats: Vec<ConnectorStats> = serde_json::from_slice(&body).unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].connector_id, "zendesk");
    assert_eq!(stats[0].record_count, 2);
}

#[tokio::test]
async fn get_connector_stats_missing_tenant_header_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/records/stats").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_connector_stats_repository_failure_returns_500() {
    let repository = Arc::new(FailingRawRecordRepository);
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/stats")
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn list_records_by_connector_returns_only_matching_connector_for_the_tenant() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    let zendesk_record = record(tenant_id, "zendesk");
    repository.insert(&zendesk_record).await.unwrap();
    repository.insert(&record(tenant_id, "sql")).await.unwrap();
    repository.insert(&record(Uuid::new_v4(), "zendesk")).await.unwrap();
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/by-connector?connector_id=zendesk")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: Vec<RawRecord> = serde_json::from_slice(&body).unwrap();
    assert_eq!(found, vec![zendesk_record]);
}

#[tokio::test]
async fn list_records_by_connector_missing_connector_id_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/by-connector")
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
