#[path = "workflow_case_test.rs"]
#[cfg(test)]
mod workflow_case_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCaseStatus {
    PendingReview,
    Approved,
    Rejected,
    Exception,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCaseKind {
    ReconciliationFailure,
    Approval,
    Exception,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowCase {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub execution_id: Uuid,
    pub kind: WorkflowCaseKind,
    pub status: WorkflowCaseStatus,
    pub summary: String,
    pub due_at: Option<DateTime<Utc>>,
    pub decided_by: Option<String>,
    pub decision_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}
