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

    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: u32,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError> {
        let mut entries: Vec<_> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id && before.map_or(true, |b| e.changed_at < b))
            .cloned()
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.changed_at));
        entries.truncate(limit as usize);
        Ok(entries)
    }
}

fn sample_entry(tenant_id: Uuid, entity_id: Uuid) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "incident".to_string(),
        entity_id,
        change_type: ChangeType::Created,
        actor: "test-actor".to_string(),
        before: None,
        after: serde_json::json!({"title": "test"}),
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
