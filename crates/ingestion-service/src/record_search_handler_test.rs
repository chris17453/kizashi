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
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: IngestState) -> Router {
    Router::new()
        .route("/v1/records/search", get(search_records))
        .route("/v1/records/:id", get(get_record))
        .with_state(state)
}

fn record(tenant_id: Uuid, connector_id: &str, payload: serde_json::Value) -> RawRecord {
    RawRecord::new(connector_id, SourceType::Ticket, tenant_id, payload)
}

#[tokio::test]
async fn search_records_returns_matches_for_the_requesting_tenant() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    repository
        .insert(&record(tenant_id, "zendesk", serde_json::json!({"subject": "printer on fire"})))
        .await
        .unwrap();
    repository.insert(&record(Uuid::new_v4(), "zendesk", serde_json::json!({}))).await.unwrap();
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/search?q=printer")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: SearchRecordsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(found.records.len(), 1);
    assert!(!found.has_more);
}

#[tokio::test]
async fn search_records_missing_tenant_header_is_rejected_with_400() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/records/search").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn search_records_repository_failure_returns_500() {
    let repository = Arc::new(FailingRawRecordRepository);
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/search")
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn get_record_returns_the_record_for_the_requesting_tenant() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    let inserted = record(tenant_id, "zendesk", serde_json::json!({"subject": "hi"}));
    repository.insert(&inserted).await.unwrap();
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/records/{}", inserted.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: RawRecord = serde_json::from_slice(&body).unwrap();
    assert_eq!(found, inserted);
}

#[tokio::test]
async fn get_record_returns_404_for_unknown_id() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
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
async fn search_records_filters_by_subject_query_param() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    repository
        .insert(&record(tenant_id, "graph-mail", serde_json::json!({"subject": "printer on fire"})))
        .await
        .unwrap();
    repository
        .insert(&record(tenant_id, "graph-mail", serde_json::json!({"subject": "password reset"})))
        .await
        .unwrap();
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/search?subject=printer")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: SearchRecordsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(found.records.len(), 1);
}

#[tokio::test]
async fn search_records_reports_has_more_when_results_exceed_the_page_size() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    for i in 0..3 {
        repository
            .insert(&record(tenant_id, "zendesk", serde_json::json!({"i": i})))
            .await
            .unwrap();
    }
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/search?limit=2")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: SearchRecordsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(found.records.len(), 2);
    assert!(found.has_more);
}

#[tokio::test]
async fn search_records_offset_skips_earlier_pages() {
    let repository = Arc::new(InMemoryRawRecordRepository::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let tenant_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    for days_ago in 0..3 {
        let mut r = record(tenant_id, "zendesk", serde_json::json!({"i": days_ago}));
        r.ingested_at = now - chrono::Duration::days(days_ago);
        repository.insert(&r).await.unwrap();
    }
    let state = IngestState { repository, publisher };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/records/search?limit=1&offset=1")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: SearchRecordsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(found.records.len(), 1);
    assert_eq!(found.records[0].raw_payload["i"], 1);
}
