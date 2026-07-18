use super::*;
use crate::archive_store::archive_store_test::InMemoryArchiveStore;
use crate::archive_store::ArchiveStore;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::raw_record_client::raw_record_client_test::InMemoryRawRecordClient;
use crate::retention_policy::retention_policy_test::InMemoryRetentionPolicyRepository;
use crate::retention_policy::{DataClass, RetentionPolicy};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use common::{RawRecord, SourceType};
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/sweep", post(trigger_sweep))
        .route("/v1/reimport", post(trigger_reimport))
        .with_state(state)
}

#[tokio::test]
async fn trigger_sweep_archives_records_past_their_policy_ttl() {
    let tenant_id = Uuid::new_v4();
    let record_client = InMemoryRawRecordClient::default();
    let mut record =
        RawRecord::new("zendesk", SourceType::Ticket, tenant_id, serde_json::json!({}));
    record.ingested_at = chrono::Utc::now() - chrono::Duration::days(200);
    record_client.records.lock().unwrap().push(record);

    let policy_repository = InMemoryRetentionPolicyRepository::with_policy(RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days: 90,
        enabled: true,
    });

    let state = AppState {
        policy_repository: Arc::new(policy_repository),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        record_client: Arc::new(record_client),
        archive_store: Arc::new(InMemoryArchiveStore::default()),
    };

    let response = router(state)
        .oneshot(Request::builder().method("POST").uri("/v1/sweep").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["records_archived"], 1);
}

#[tokio::test]
async fn trigger_reimport_replays_an_archived_batch() {
    let tenant_id = Uuid::new_v4();
    let archive_store = InMemoryArchiveStore::default();
    let record = RawRecord::new("zendesk", SourceType::Ticket, tenant_id, serde_json::json!({}));
    let key = archive_store
        .write_batch(tenant_id, "raw", &[record], chrono::Utc::now(), chrono::Utc::now())
        .await
        .unwrap();

    let state = AppState {
        policy_repository: Arc::new(InMemoryRetentionPolicyRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        record_client: Arc::new(InMemoryRawRecordClient::default()),
        archive_store: Arc::new(archive_store),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reimport")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"archive_key": key}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(summary["records_reimported"], 1);
}

#[tokio::test]
async fn trigger_reimport_returns_404_for_unknown_archive_key() {
    let state = AppState {
        policy_repository: Arc::new(InMemoryRetentionPolicyRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        record_client: Arc::new(InMemoryRawRecordClient::default()),
        archive_store: Arc::new(InMemoryArchiveStore::default()),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reimport")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"archive_key": "missing"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
