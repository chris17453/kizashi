use super::*;
use uuid::Uuid;

#[test]
fn command_execution_carries_a_stable_idempotency_key() {
    let execution = PipelineExecution::new_command(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "invoice-42:approve".to_string(),
        serde_json::json!({"status": "approved"}),
    );

    assert_eq!(execution.status, PipelineExecutionStatus::Queued);
    assert_eq!(execution.idempotency_key, "invoice-42:approve");
    assert!(execution.command);
}
