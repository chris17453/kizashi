use super::*;
use crate::archive_store::archive_store_test::{FailingArchiveStore, InMemoryArchiveStore};
use crate::raw_record_client::raw_record_client_test::InMemoryRawRecordClient;
use crate::retention_policy::retention_policy_test::InMemoryRetentionPolicyRepository;
use crate::retention_policy::RetentionPolicy;
use common::{RawRecord, SourceType};
use uuid::Uuid;

fn record_ingested_at(tenant_id: Uuid, ingested_at: DateTime<Utc>) -> RawRecord {
    let mut record =
        RawRecord::new("zendesk", SourceType::Ticket, tenant_id, serde_json::json!({}));
    record.ingested_at = ingested_at;
    record
}

fn raw_policy(tenant_id: Uuid, ttl_days: i32) -> RetentionPolicy {
    RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days,
        enabled: true,
    }
}

#[tokio::test]
async fn archives_and_deletes_records_older_than_the_policy_ttl_but_leaves_recent_ones() {
    let now = Utc::now();
    let tenant_id = Uuid::new_v4();
    let old = record_ingested_at(tenant_id, now - chrono::Duration::days(100));
    let recent = record_ingested_at(tenant_id, now - chrono::Duration::days(1));

    let record_client = Arc::new(InMemoryRawRecordClient::default());
    record_client.records.lock().unwrap().push(old.clone());
    record_client.records.lock().unwrap().push(recent.clone());

    let policy_repository =
        Arc::new(InMemoryRetentionPolicyRepository::with_policy(raw_policy(tenant_id, 90)));
    let archive_store = Arc::new(InMemoryArchiveStore::default());

    let state = SweepState {
        policy_repository,
        record_client: record_client.clone(),
        archive_store: archive_store.clone(),
    };

    let summary = sweep(&state, now, 100).await.unwrap();

    assert_eq!(summary.records_archived, 1);
    assert_eq!(summary.batches_written.len(), 1);
    assert_eq!(*record_client.deleted.lock().unwrap(), vec![old.id]);
    assert_eq!(record_client.records.lock().unwrap().clone(), vec![recent]);

    let (_, archived) = archive_store.read_batch(&summary.batches_written[0]).await.unwrap();
    assert_eq!(archived, vec![old]);
}

#[tokio::test]
async fn disabled_policies_are_not_swept() {
    let now = Utc::now();
    let tenant_id = Uuid::new_v4();
    let mut policy = raw_policy(tenant_id, 90);
    policy.enabled = false;

    let record_client = Arc::new(InMemoryRawRecordClient::default());
    record_client
        .records
        .lock()
        .unwrap()
        .push(record_ingested_at(tenant_id, now - chrono::Duration::days(200)));

    let policy_repository = Arc::new(InMemoryRetentionPolicyRepository::with_policy(policy));
    let archive_store = Arc::new(InMemoryArchiveStore::default());
    let state =
        SweepState { policy_repository, record_client: record_client.clone(), archive_store };

    let summary = sweep(&state, now, 100).await.unwrap();

    assert_eq!(summary.records_archived, 0);
    assert_eq!(record_client.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn non_raw_data_classes_are_not_swept_in_v1() {
    let now = Utc::now();
    let tenant_id = Uuid::new_v4();
    let mut policy = raw_policy(tenant_id, 90);
    policy.data_class = DataClass::Event;

    let record_client = Arc::new(InMemoryRawRecordClient::default());
    record_client
        .records
        .lock()
        .unwrap()
        .push(record_ingested_at(tenant_id, now - chrono::Duration::days(200)));

    let policy_repository = Arc::new(InMemoryRetentionPolicyRepository::with_policy(policy));
    let archive_store = Arc::new(InMemoryArchiveStore::default());
    let state =
        SweepState { policy_repository, record_client: record_client.clone(), archive_store };

    let summary = sweep(&state, now, 100).await.unwrap();

    assert_eq!(summary.records_archived, 0);
    assert_eq!(record_client.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn pages_through_more_records_than_the_batch_limit() {
    let now = Utc::now();
    let tenant_id = Uuid::new_v4();
    let record_client = Arc::new(InMemoryRawRecordClient::default());
    for days_ago in 100..105 {
        record_client
            .records
            .lock()
            .unwrap()
            .push(record_ingested_at(tenant_id, now - chrono::Duration::days(days_ago)));
    }

    let policy_repository =
        Arc::new(InMemoryRetentionPolicyRepository::with_policy(raw_policy(tenant_id, 90)));
    let archive_store = Arc::new(InMemoryArchiveStore::default());
    let state =
        SweepState { policy_repository, record_client: record_client.clone(), archive_store };

    let summary = sweep(&state, now, 2).await.unwrap();

    assert_eq!(summary.records_archived, 5);
    assert_eq!(summary.batches_written.len(), 3);
    assert!(record_client.records.lock().unwrap().is_empty());
}

#[tokio::test]
async fn archive_failure_stops_that_policys_sweep_without_deleting() {
    let now = Utc::now();
    let tenant_id = Uuid::new_v4();
    let record_client = Arc::new(InMemoryRawRecordClient::default());
    record_client
        .records
        .lock()
        .unwrap()
        .push(record_ingested_at(tenant_id, now - chrono::Duration::days(200)));

    let policy_repository =
        Arc::new(InMemoryRetentionPolicyRepository::with_policy(raw_policy(tenant_id, 90)));
    let archive_store = Arc::new(FailingArchiveStore);
    let state =
        SweepState { policy_repository, record_client: record_client.clone(), archive_store };

    let summary = sweep(&state, now, 100).await.unwrap();

    assert_eq!(summary.records_archived, 0);
    assert_eq!(record_client.records.lock().unwrap().len(), 1);
}
