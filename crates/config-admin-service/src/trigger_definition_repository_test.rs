use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTriggerDefinitionRepository {
    pub triggers: Mutex<Vec<TriggerDefinition>>,
}

impl InMemoryTriggerDefinitionRepository {
    pub fn with_trigger(trigger: TriggerDefinition) -> Self {
        Self { triggers: Mutex::new(vec![trigger]) }
    }
}

#[async_trait]
impl TriggerDefinitionRepository for InMemoryTriggerDefinitionRepository {
    async fn create(
        &self,
        trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError> {
        self.triggers.lock().unwrap().push(trigger.clone());
        Ok(trigger)
    }

    async fn update(
        &self,
        trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError> {
        let mut triggers = self.triggers.lock().unwrap();
        match triggers.iter_mut().find(|t| t.id == trigger.id && t.tenant_id == trigger.tenant_id) {
            Some(existing) => {
                *existing = trigger.clone();
                Ok(trigger)
            }
            None => Err(TriggerDefinitionRepositoryError::NotFound(trigger.id)),
        }
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerDefinitionRepositoryError> {
        Ok(self
            .triggers
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id && t.tenant_id == tenant_id)
            .cloned())
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TriggerDefinition>, TriggerDefinitionRepositoryError> {
        let mut triggers: Vec<TriggerDefinition> = self
            .triggers
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.tenant_id == tenant_id)
            .cloned()
            .collect();
        triggers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(triggers.into_iter().skip(offset as usize).take(limit as usize).collect())
    }
}

pub struct FailingTriggerDefinitionRepository;

#[async_trait]
impl TriggerDefinitionRepository for FailingTriggerDefinitionRepository {
    async fn create(
        &self,
        _trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError> {
        Err(TriggerDefinitionRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update(
        &self,
        _trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError> {
        Err(TriggerDefinitionRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerDefinitionRepositoryError> {
        Err(TriggerDefinitionRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<TriggerDefinition>, TriggerDefinitionRepositoryError> {
        Err(TriggerDefinitionRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_trigger(tenant_id: Uuid) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "test".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: common::TriggerCondition::CountOverWindow { count: 3 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let repo = InMemoryTriggerDefinitionRepository::default();
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);

    repo.create(trigger.clone()).await.unwrap();
    let found = repo.get(tenant_id, trigger.id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn update_of_unknown_trigger_returns_not_found() {
    let repo = InMemoryTriggerDefinitionRepository::default();
    let trigger = sample_trigger(Uuid::new_v4());

    let err = repo.update(trigger).await.unwrap_err();
    assert!(matches!(err, TriggerDefinitionRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTriggerDefinitionRepository::with_trigger(sample_trigger(tenant_id));
    repo.create(sample_trigger(Uuid::new_v4())).await.unwrap();

    let found = repo.list(tenant_id, 25, 0).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn list_respects_limit_and_offset() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTriggerDefinitionRepository::default();
    for name in ["a", "b", "c"] {
        let mut trigger = sample_trigger(tenant_id);
        trigger.name = name.to_string();
        repo.create(trigger).await.unwrap();
    }

    let found = repo.list(tenant_id, 1, 1).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "b");
}
