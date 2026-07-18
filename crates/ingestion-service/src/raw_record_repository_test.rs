use super::*;
use std::sync::Mutex;

/// In-memory test double shared by this module's tests and by ingest_handler's unit tests, so
/// handler logic can be verified without a live Postgres instance (CLAUDE.md §2).
#[derive(Default)]
pub struct InMemoryRawRecordRepository {
    pub records: Mutex<Vec<RawRecord>>,
}

#[async_trait]
impl RawRecordRepository for InMemoryRawRecordRepository {
    async fn insert(&self, record: &RawRecord) -> Result<(), RepositoryError> {
        self.records.lock().unwrap().push(record.clone());
        Ok(())
    }

    async fn update_normalized_payload(
        &self,
        record_id: uuid::Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<bool, RepositoryError> {
        let mut records = self.records.lock().unwrap();
        match records.iter_mut().find(|r| r.id == record_id) {
            Some(record) => {
                record.normalized_payload = Some(normalized_payload.clone());
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

pub struct FailingRawRecordRepository;

#[async_trait]
impl RawRecordRepository for FailingRawRecordRepository {
    async fn insert(&self, _record: &RawRecord) -> Result<(), RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update_normalized_payload(
        &self,
        _record_id: uuid::Uuid,
        _normalized_payload: &serde_json::Value,
    ) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_repository_stores_inserted_records() {
    let repo = InMemoryRawRecordRepository::default();
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );

    repo.insert(&record).await.unwrap();

    let stored = repo.records.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0], record);
}

#[tokio::test]
async fn failing_repository_returns_backend_error() {
    let repo = FailingRawRecordRepository;
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );

    let err = repo.insert(&record).await.unwrap_err();
    assert!(matches!(err, RepositoryError::Backend(_)));
}

#[tokio::test]
async fn update_normalized_payload_sets_it_on_a_known_record_and_returns_true() {
    let repo = InMemoryRawRecordRepository::default();
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );
    repo.insert(&record).await.unwrap();

    let normalized = serde_json::json!({"text": "hi"});
    let updated = repo.update_normalized_payload(record.id, &normalized).await.unwrap();

    assert!(updated);
    assert_eq!(repo.records.lock().unwrap()[0].normalized_payload, Some(normalized));
}

#[tokio::test]
async fn update_normalized_payload_returns_false_for_unknown_record() {
    let repo = InMemoryRawRecordRepository::default();
    let updated =
        repo.update_normalized_payload(uuid::Uuid::new_v4(), &serde_json::json!({})).await.unwrap();
    assert!(!updated);
}
