use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAuditLogRepository {
    pub entries: Mutex<Vec<AuditLogEntry>>,
}

#[async_trait]
impl AuditLogRepository for InMemoryAuditLogRepository {
    async fn record(&self, entry: AuditLogEntry) -> Result<(), AuditLogError> {
        self.entries.lock().unwrap().push(entry);
        Ok(())
    }
}

pub struct FailingAuditLogRepository;

#[async_trait]
impl AuditLogRepository for FailingAuditLogRepository {
    async fn record(&self, _entry: AuditLogEntry) -> Result<(), AuditLogError> {
        Err(AuditLogError::Backend("simulated failure".to_string()))
    }
}

fn sample_entry() -> AuditLogEntry {
    AuditLogEntry {
        tenant_id: "tenant-a".to_string(),
        connector_id: "zendesk-connector".to_string(),
        destination_host: "api.zendesk.com".to_string(),
        destination_port: 443,
        allowed: true,
        occurred_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn in_memory_repository_records_entries() {
    let repo = InMemoryAuditLogRepository::default();
    repo.record(sample_entry()).await.unwrap();

    let entries = repo.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].destination_host, "api.zendesk.com");
}

#[tokio::test]
async fn failing_repository_returns_backend_error() {
    let repo = FailingAuditLogRepository;
    let err = repo.record(sample_entry()).await.unwrap_err();
    assert!(matches!(err, AuditLogError::Backend(_)));
}
