use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTriggerRepository {
    pub triggers: Mutex<Vec<TriggerDefinition>>,
}

impl InMemoryTriggerRepository {
    pub fn with_trigger(trigger: TriggerDefinition) -> Self {
        Self { triggers: Mutex::new(vec![trigger]) }
    }
}

#[async_trait]
impl TriggerRepository for InMemoryTriggerRepository {
    async fn active_triggers_for(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Vec<TriggerDefinition>, TriggerRepositoryError> {
        Ok(self
            .triggers
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.enabled && t.tenant_id == tenant_id && t.event_type_match == event_type)
            .cloned()
            .collect())
    }
}

fn sample_trigger(tenant_id: Uuid, enabled: bool) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "test".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: common::TriggerCondition::CountOverWindow { count: 3 },
        window_seconds: 3600,
        actions: vec![],
        enabled,
    }
}

#[tokio::test]
async fn returns_enabled_triggers_matching_tenant_and_event_type() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTriggerRepository::with_trigger(sample_trigger(tenant_id, true));

    let found = repo.active_triggers_for(tenant_id, "sentiment").await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn excludes_disabled_triggers() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTriggerRepository::with_trigger(sample_trigger(tenant_id, false));

    let found = repo.active_triggers_for(tenant_id, "sentiment").await.unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn excludes_triggers_for_a_different_event_type() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTriggerRepository::with_trigger(sample_trigger(tenant_id, true));

    let found = repo.active_triggers_for(tenant_id, "urgency").await.unwrap();
    assert!(found.is_empty());
}
