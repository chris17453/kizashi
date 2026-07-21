use super::*;
use common::TriggerCondition;
use std::sync::Mutex;

/// True if `trigger` should be evaluated for an incoming `event_type` (ADR-0027): either its
/// `event_type_match` matches directly (the single-event-type shapes), or it's a
/// `CorrelatedOverWindow` trigger whose `conditions` list includes `event_type` — mirrors the
/// real `PostgresTriggerRepository`'s JSONB containment query, which this in-memory double
/// stands in for.
fn matches_event_type(trigger: &TriggerDefinition, event_type: &str) -> bool {
    if trigger.event_type_match == event_type {
        return true;
    }
    match &trigger.condition {
        TriggerCondition::CorrelatedOverWindow { conditions } => {
            conditions.iter().any(|c| c.event_type == event_type)
        }
        _ => false,
    }
}

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
            .filter(|t| t.enabled && t.tenant_id == tenant_id && matches_event_type(t, event_type))
            .cloned()
            .collect())
    }

    async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerRepositoryError> {
        Ok(self.triggers.lock().unwrap().iter().find(|t| t.id == id).cloned())
    }

    async fn upsert(&self, trigger: TriggerDefinition) -> Result<(), TriggerRepositoryError> {
        let mut triggers = self.triggers.lock().unwrap();
        match triggers.iter_mut().find(|t| t.id == trigger.id) {
            Some(existing) => *existing = trigger,
            None => triggers.push(trigger),
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), TriggerRepositoryError> {
        self.triggers.lock().unwrap().retain(|t| t.id != id);
        Ok(())
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

fn correlated_sample_trigger(tenant_id: Uuid) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "email-and-chat".to_string(),
        event_type_match: "sentiment_drop_email".to_string(),
        condition: common::TriggerCondition::CorrelatedOverWindow {
            conditions: vec![
                common::CorrelatedCondition {
                    event_type: "sentiment_drop_email".to_string(),
                    min_count: 1,
                },
                common::CorrelatedCondition {
                    event_type: "unresolved_chat".to_string(),
                    min_count: 1,
                },
            ],
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn a_correlated_trigger_is_found_by_any_of_its_listed_event_types() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTriggerRepository::with_trigger(correlated_sample_trigger(tenant_id));

    assert_eq!(repo.active_triggers_for(tenant_id, "sentiment_drop_email").await.unwrap().len(), 1);
    assert_eq!(repo.active_triggers_for(tenant_id, "unresolved_chat").await.unwrap().len(), 1);
    assert!(repo.active_triggers_for(tenant_id, "unrelated").await.unwrap().is_empty());
}

#[tokio::test]
async fn get_by_id_finds_a_trigger_regardless_of_enabled_state() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id, false);
    let repo = InMemoryTriggerRepository::with_trigger(trigger.clone());

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn get_by_id_returns_none_for_unknown_id() {
    let repo = InMemoryTriggerRepository::default();
    let found = repo.get_by_id(Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn upsert_inserts_a_new_trigger() {
    let repo = InMemoryTriggerRepository::default();
    let trigger = sample_trigger(Uuid::new_v4(), true);

    repo.upsert(trigger.clone()).await.unwrap();

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn upsert_replaces_an_existing_trigger_with_the_same_id() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id, true);
    let repo = InMemoryTriggerRepository::with_trigger(trigger.clone());

    let mut updated = trigger.clone();
    updated.enabled = false;
    updated.name = "renamed".to_string();
    repo.upsert(updated.clone()).await.unwrap();

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert_eq!(found, Some(updated));
}

#[tokio::test]
async fn delete_removes_the_trigger() {
    let trigger = sample_trigger(Uuid::new_v4(), true);
    let repo = InMemoryTriggerRepository::with_trigger(trigger.clone());

    repo.delete(trigger.id).await.unwrap();

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn delete_of_unknown_id_is_a_no_op() {
    let repo = InMemoryTriggerRepository::default();
    repo.delete(Uuid::new_v4()).await.unwrap();
}
