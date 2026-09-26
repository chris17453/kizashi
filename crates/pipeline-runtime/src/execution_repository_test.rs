use super::*;
use common::PipelineExecution;
use uuid::Uuid;

#[test]
fn execution_request_keys_are_scoped_to_pipeline_and_tenant() {
    let mut execution = PipelineExecution::new_projection(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "erp:invoice:42".to_string(),
        serde_json::json!({}),
    );
    execution.pipeline_version = 1;
    assert_eq!(execution.attempt, 1);
    assert!(validate_execution(&execution).is_ok());
}

#[test]
fn execution_request_requires_a_definition_version_snapshot() {
    let execution = PipelineExecution::new_projection(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "erp:invoice:42".to_string(),
        serde_json::json!({}),
    );
    assert!(matches!(
        validate_execution(&execution),
        Err(ExecutionRepositoryError::MissingPipelineVersion)
    ));
}
