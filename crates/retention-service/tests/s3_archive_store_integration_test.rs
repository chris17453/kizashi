//! Integration test against a real S3-compatible backend (MinIO in docker-compose, CLAUDE.md
//! §2, ADR-0011). Requires S3_ENDPOINT_URL, AWS_S3_BUCKET, AWS_REGION,
//! AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY (all present in .env.example).

use common::{RawRecord, SourceType};
use retention_service::{ArchiveStore, ArchiveStoreError, S3ArchiveStore};
use uuid::Uuid;

async fn test_store() -> S3ArchiveStore {
    let region = std::env::var("AWS_REGION").expect("AWS_REGION must be set to run this test");
    let endpoint_url =
        std::env::var("S3_ENDPOINT_URL").expect("S3_ENDPOINT_URL must be set to run this test");
    let bucket =
        std::env::var("AWS_S3_BUCKET").expect("AWS_S3_BUCKET must be set to run this test");

    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region))
        .endpoint_url(endpoint_url)
        .load()
        .await;
    let s3_config =
        aws_sdk_s3::config::Builder::from(&shared_config).force_path_style(true).build();
    let store = S3ArchiveStore::new(aws_sdk_s3::Client::from_conf(s3_config), bucket);
    store.ensure_bucket().await.expect("failed to ensure bucket exists");
    store
}

fn sample_record(tenant_id: Uuid) -> RawRecord {
    RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        serde_json::json!({"subject": "help me"}),
    )
}

#[tokio::test]
async fn write_batch_then_read_batch_round_trips_against_a_real_s3_compatible_backend() {
    let store = test_store().await;
    let tenant_id = Uuid::new_v4();
    let records = vec![sample_record(tenant_id), sample_record(tenant_id)];
    let now = chrono::Utc::now();

    let key = store.write_batch(tenant_id, "raw", &records, now, now).await.unwrap();
    let (manifest, found) = store.read_batch(&key).await.unwrap();

    assert_eq!(found, records);
    assert_eq!(manifest.record_count, 2);
    assert_eq!(manifest.tenant_id, tenant_id);
    assert_eq!(manifest.data_class, "raw");
}

#[tokio::test]
async fn read_batch_returns_not_found_for_an_unknown_key() {
    let store = test_store().await;
    let err = store.read_batch("archive/does/not/exist.ndjson.gz").await.unwrap_err();
    assert!(matches!(err, ArchiveStoreError::NotFound(_)));
}

#[tokio::test]
async fn ensure_bucket_is_idempotent() {
    let store = test_store().await;
    store.ensure_bucket().await.expect("second call should not fail");
}
