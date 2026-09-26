#[path = "pipeline_execution_test.rs"]
#[cfg(test)]
mod pipeline_execution_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable runtime state for one immutable invocation of a versioned Pipeline Definition.
/// Command executions remain pending until an authoritative source confirmation is recorded;
/// they do not imply that a local projection has already been reconciled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineExecution {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub pipeline_definition_id: Uuid,
    pub pipeline_version: i32,
    pub command: bool,
    pub idempotency_key: String,
    pub status: PipelineExecutionStatus,
    pub attempt: i32,
    pub input: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub requested_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineExecutionStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    DeadLettered,
    AwaitingConfirmation,
    Confirmed,
}

impl PipelineExecution {
    pub fn new_projection(
        tenant_id: Uuid,
        pipeline_definition_id: Uuid,
        idempotency_key: String,
        input: serde_json::Value,
    ) -> Self {
        Self::new(tenant_id, pipeline_definition_id, false, idempotency_key, input)
    }

    pub fn new_command(
        tenant_id: Uuid,
        pipeline_definition_id: Uuid,
        idempotency_key: String,
        input: serde_json::Value,
    ) -> Self {
        Self::new(tenant_id, pipeline_definition_id, true, idempotency_key, input)
    }

    fn new(
        tenant_id: Uuid,
        pipeline_definition_id: Uuid,
        command: bool,
        idempotency_key: String,
        input: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            pipeline_definition_id,
            pipeline_version: 0,
            command,
            idempotency_key,
            status: PipelineExecutionStatus::Queued,
            attempt: 1,
            input,
            result: None,
            requested_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }
}
