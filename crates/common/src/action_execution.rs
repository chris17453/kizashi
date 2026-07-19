#[path = "action_execution_test.rs"]
#[cfg(test)]
mod action_execution_test;

use crate::trigger_definition::ActionType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Append-only audit row for an action run (spec §5.5, CLAUDE.md §5). Never update-in-place —
/// a retry or correction is a *new* row referencing `event_id`/`trigger_id`, so the audit
/// trail always shows every attempt, not just the latest state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionExecution {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub trigger_id: Uuid,
    pub event_id: Uuid,
    pub action_type: ActionType,
    pub status: ActionExecutionStatus,
    pub executed_at: DateTime<Utc>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionExecutionStatus {
    Pending,
    Sent,
    Failed,
    Retried,
}

impl ActionExecution {
    pub fn new(
        tenant_id: Uuid,
        trigger_id: Uuid,
        event_id: Uuid,
        action_type: ActionType,
        detail: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            trigger_id,
            event_id,
            action_type,
            status: ActionExecutionStatus::Pending,
            executed_at: Utc::now(),
            detail,
        }
    }

    /// Produces a new append-only row recording a retry attempt, referencing this execution's
    /// trigger/event rather than mutating this row.
    pub fn retry(&self, detail: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id: self.tenant_id,
            trigger_id: self.trigger_id,
            event_id: self.event_id,
            action_type: self.action_type,
            status: ActionExecutionStatus::Retried,
            executed_at: Utc::now(),
            detail,
        }
    }
}
