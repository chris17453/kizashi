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
}

pub struct FailingExecutionRepository;

#[async_trait]
impl ExecutionRepository for FailingExecutionRepository {
    async fn insert(&self, _execution: &ActionExecution) -> Result<(), ExecutionRepositoryError> {
        Err(ExecutionRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_repository_records_inserted_executions() {
    let repo = InMemoryExecutionRepository::default();
    let execution = ActionExecution::new(
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        common::ActionType::Webhook,
        serde_json::json!({}),
    );

    repo.insert(&execution).await.unwrap();

    assert_eq!(repo.executions.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failing_repository_returns_backend_error() {
    let repo = FailingExecutionRepository;
    let execution = ActionExecution::new(
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        common::ActionType::Webhook,
        serde_json::json!({}),
    );

    let err = repo.insert(&execution).await.unwrap_err();
    assert!(matches!(err, ExecutionRepositoryError::Backend(_)));
}
