use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryExecutionRepository {
    pub executions: Mutex<Vec<ActionExecution>>,
}

#[async_trait]
impl ExecutionRepository for InMemoryExecutionRepository {
    async fn insert(&self, execution: &ActionExecution) -> Result<(), ExecutionRepositoryError> {
        self.executions.lock().unwrap().push(execution.clone());
        Ok(())
    }

    async fn list_by_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecution>, ExecutionRepositoryError> {
        Ok(self
            .executions
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id && e.event_id == event_id)
            .cloned()
            .collect())
    }
}

pub struct FailingExecutionRepository;

#[async_trait]
impl ExecutionRepository for FailingExecutionRepository {
    async fn insert(&self, _execution: &ActionExecution) -> Result<(), ExecutionRepositoryError> {
        Err(ExecutionRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_by_event(
        &self,
        _tenant_id: Uuid,
        _event_id: Uuid,
    ) -> Result<Vec<ActionExecution>, ExecutionRepositoryError> {
        Err(ExecutionRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_execution(tenant_id: Uuid, event_id: Uuid) -> ActionExecution {
    ActionExecution::new(
        tenant_id,
        Uuid::new_v4(),
        event_id,
        common::ActionType::Webhook,
        serde_json::json!({}),
    )
}

#[tokio::test]
async fn in_memory_repository_records_inserted_executions() {
    let repo = InMemoryExecutionRepository::default();
    let execution = sample_execution(Uuid::new_v4(), Uuid::new_v4());

    repo.insert(&execution).await.unwrap();

    assert_eq!(repo.executions.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn list_by_event_is_scoped_to_tenant_and_event() {
    let repo = InMemoryExecutionRepository::default();
    let tenant_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    repo.insert(&sample_execution(tenant_id, event_id)).await.unwrap();
    repo.insert(&sample_execution(tenant_id, Uuid::new_v4())).await.unwrap();
    repo.insert(&sample_execution(Uuid::new_v4(), event_id)).await.unwrap();

    let found = repo.list_by_event(tenant_id, event_id).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn failing_repository_returns_backend_error() {
    let repo = FailingExecutionRepository;
    let execution = sample_execution(Uuid::new_v4(), Uuid::new_v4());

    let err = repo.insert(&execution).await.unwrap_err();
    assert!(matches!(err, ExecutionRepositoryError::Backend(_)));
}
