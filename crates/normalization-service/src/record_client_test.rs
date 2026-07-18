use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryRecordClient {
    pub updates: Mutex<Vec<(Uuid, serde_json::Value)>>,
}

#[async_trait]
impl RecordClient for InMemoryRecordClient {
    async fn update_normalized_payload(
        &self,
        record_id: Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<(), RecordClientError> {
        self.updates.lock().unwrap().push((record_id, normalized_payload.clone()));
        Ok(())
    }
}

pub struct FailingRecordClient;

#[async_trait]
impl RecordClient for FailingRecordClient {
    async fn update_normalized_payload(
        &self,
        _record_id: Uuid,
        _normalized_payload: &serde_json::Value,
    ) -> Result<(), RecordClientError> {
        Err(RecordClientError::Unreachable("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_client_records_updates() {
    let client = InMemoryRecordClient::default();
    let record_id = Uuid::new_v4();
    let payload = serde_json::json!({"text": "hi"});

    client.update_normalized_payload(record_id, &payload).await.unwrap();

    let updates = client.updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0], (record_id, payload));
}

#[tokio::test]
async fn failing_client_returns_unreachable_error() {
    let client = FailingRecordClient;
    let err =
        client.update_normalized_payload(Uuid::new_v4(), &serde_json::json!({})).await.unwrap_err();
    assert!(matches!(err, RecordClientError::Unreachable(_)));
}
