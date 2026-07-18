use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAuditLogReader {
    pub entries: Mutex<Vec<AuditLogEntry>>,
}

#[async_trait]
impl AuditLogReader for InMemoryAuditLogReader {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError> {
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

pub struct FailingAuditLogReader;

#[async_trait]
impl AuditLogReader for FailingAuditLogReader {
    async fn list_for_entity(
        &self,
        _tenant_id: Uuid,
        _entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError> {
        Err(AuditLogError::Backend("simulated failure".to_string()))
    }
}

fn sample_entry(tenant_id: Uuid, entity_id: Uuid) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "retention_policy".to_string(),
        entity_id,
        change_type: ChangeType::Created,
        actor: tenant_id.to_string(),
        before: None,
        after: serde_json::json!({"ttl_days": 30}),
        changed_at: Utc::now(),
    }
}

#[tokio::test]
async fn list_for_entity_is_scoped_to_tenant_and_entity() {
    let tenant_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(sample_entry(tenant_id, entity_id));
    reader.entries.lock().unwrap().push(sample_entry(Uuid::new_v4(), entity_id));

    let found = reader.list_for_entity(tenant_id, entity_id).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn change_type_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&ChangeType::Created).unwrap(), "\"created\"");
    assert_eq!(serde_json::to_string(&ChangeType::Updated).unwrap(), "\"updated\"");
}
