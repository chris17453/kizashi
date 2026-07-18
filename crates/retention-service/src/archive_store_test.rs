use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryArchiveStore {
    pub batches: Mutex<HashMap<String, (ArchiveManifest, Vec<RawRecord>)>>,
}

#[async_trait]
impl ArchiveStore for InMemoryArchiveStore {
    async fn write_batch(
        &self,
        tenant_id: Uuid,
        data_class: &str,
        records: &[RawRecord],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<String, ArchiveStoreError> {
        let manifest =
            ArchiveManifest::new(tenant_id, data_class, records.len(), window_start, window_end);
        let key = archive_key(tenant_id, data_class, window_end, Uuid::new_v4());
        self.batches.lock().unwrap().insert(key.clone(), (manifest, records.to_vec()));
        Ok(key)
    }

    async fn read_batch(
        &self,
        key: &str,
    ) -> Result<(ArchiveManifest, Vec<RawRecord>), ArchiveStoreError> {
        self.batches
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| ArchiveStoreError::NotFound(key.to_string()))
    }
}

pub struct FailingArchiveStore;

#[async_trait]
impl ArchiveStore for FailingArchiveStore {
    async fn write_batch(
        &self,
        _tenant_id: Uuid,
        _data_class: &str,
        _records: &[RawRecord],
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
    ) -> Result<String, ArchiveStoreError> {
        Err(ArchiveStoreError::Backend("simulated failure".to_string()))
    }

    async fn read_batch(
        &self,
        key: &str,
    ) -> Result<(ArchiveManifest, Vec<RawRecord>), ArchiveStoreError> {
        Err(ArchiveStoreError::NotFound(key.to_string()))
    }
}

fn sample_record(tenant_id: Uuid) -> RawRecord {
    RawRecord::new("zendesk", common::SourceType::Ticket, tenant_id, serde_json::json!({"a": 1}))
}

#[test]
fn encode_then_decode_round_trips_manifest_and_records() {
    let tenant_id = Uuid::new_v4();
    let records = vec![sample_record(tenant_id), sample_record(tenant_id)];
    let manifest = ArchiveManifest::new(tenant_id, "raw", records.len(), Utc::now(), Utc::now());

    let gzipped = encode_batch(&manifest, &records).unwrap();
    let (decoded_manifest, decoded_records) = decode_batch("test-key", &gzipped).unwrap();

    assert_eq!(decoded_manifest, manifest);
    assert_eq!(decoded_records, records);
}

#[test]
fn decode_batch_rejects_corrupt_gzip() {
    let err = decode_batch("bad-key", b"not gzip data").unwrap_err();
    assert!(matches!(err, ArchiveStoreError::Corrupt(_, _)));
}

#[tokio::test]
async fn in_memory_store_round_trips_a_batch() {
    let store = InMemoryArchiveStore::default();
    let tenant_id = Uuid::new_v4();
    let records = vec![sample_record(tenant_id)];

    let key = store.write_batch(tenant_id, "raw", &records, Utc::now(), Utc::now()).await.unwrap();
    let (manifest, found) = store.read_batch(&key).await.unwrap();

    assert_eq!(found, records);
    assert_eq!(manifest.record_count, 1);
}

#[tokio::test]
async fn in_memory_store_returns_not_found_for_unknown_key() {
    let store = InMemoryArchiveStore::default();
    let err = store.read_batch("missing").await.unwrap_err();
    assert!(matches!(err, ArchiveStoreError::NotFound(_)));
}
