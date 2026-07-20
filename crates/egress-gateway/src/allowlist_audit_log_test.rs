use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAllowlistAuditLogReader {
    pub entries: Mutex<Vec<AllowlistAuditLogEntry>>,
}

#[async_trait]
impl AllowlistAuditLogReader for InMemoryAllowlistAuditLogReader {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AllowlistAuditLogEntry>, AllowlistAuditLogError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id && e.entity_id == entity_id)
            .cloned()
            .collect())
    }
}

pub struct FailingAllowlistAuditLogReader;

#[async_trait]
impl AllowlistAuditLogReader for FailingAllowlistAuditLogReader {
    async fn list_for_entity(
        &self,
        _tenant_id: Uuid,
        _entity_id: Uuid,
    ) -> Result<Vec<AllowlistAuditLogEntry>, AllowlistAuditLogError> {
        Err(AllowlistAuditLogError::Backend("simulated failure".to_string()))
    }
}

fn sample_entry(tenant_id: Uuid, entity_id: Uuid) -> AllowlistAuditLogEntry {
    AllowlistAuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "egress_allowlist".to_string(),
        entity_id,
        change_type: AllowlistChangeType::Updated,
        actor: "operator@example.com".to_string(),
        before: None,
        after: serde_json::json!({"domains": ["zendesk.com"]}),
        changed_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn list_for_entity_is_scoped_to_tenant_and_entity() {
    let reader = InMemoryAllowlistAuditLogReader::default();
    let tenant_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    reader.entries.lock().unwrap().push(sample_entry(tenant_id, entity_id));
    reader.entries.lock().unwrap().push(sample_entry(Uuid::new_v4(), entity_id));

    let found = reader.list_for_entity(tenant_id, entity_id).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].tenant_id, tenant_id);
}

#[tokio::test]
async fn failing_reader_returns_backend_error() {
    let reader = FailingAllowlistAuditLogReader;
    let err = reader.list_for_entity(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, AllowlistAuditLogError::Backend(_)));
}
