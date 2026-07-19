#[path = "process_event_test.rs"]
#[cfg(test)]
mod process_event_test;

use crate::action_dispatcher::ActionDispatcher;
use crate::execution_repository::ExecutionRepository;
use crate::trigger_client::TriggerClient;
use common::{ActionExecution, ActionExecutionStatus, Event};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to look up trigger: {0}")]
    TriggerLookup(String),
    #[error("event has no `triggered_by` trigger id in its payload")]
    MissingTriggerId,
    #[error("no trigger found for id {0}")]
    TriggerNotFound(Uuid),
    #[error("failed to write action execution audit row: {0}")]
    ExecutionWrite(String),
}

#[derive(Clone)]
pub struct ActionDeps {
    pub trigger_client: Arc<dyn TriggerClient>,
    pub dispatcher: Arc<dyn ActionDispatcher>,
    pub execution_repository: Arc<dyn ExecutionRepository>,
}

fn extract_trigger_id(event: &Event) -> Option<Uuid> {
    event.payload.get("triggered_by").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok())
}

/// One `event.created` message all the way through to executed actions and their append-only
/// audit rows (spec §6, service #7). Every action's outcome (success or failure) gets its own
/// `ActionExecution` row — a dispatch failure is recorded, not swallowed, since the audit trail
/// is the point (CLAUDE.md §5: "Action executions are append-only").
pub async fn process_event(deps: &ActionDeps, event: &Event) -> Result<usize, ProcessError> {
    let trigger_id = extract_trigger_id(event).ok_or(ProcessError::MissingTriggerId)?;

    let trigger = deps
        .trigger_client
        .get_trigger(trigger_id)
        .await
        .map_err(|e| ProcessError::TriggerLookup(e.to_string()))?
        .ok_or(ProcessError::TriggerNotFound(trigger_id))?;

    let mut executed = 0;
    for action in &trigger.actions {
        let (status, detail) = match deps.dispatcher.dispatch(action, event).await {
            Ok(detail) => (ActionExecutionStatus::Sent, detail),
            Err(e) => {
                tracing::error!(event_id = %event.id, action_type = ?action.action_type, error = %e, "action dispatch failed");
                (ActionExecutionStatus::Failed, serde_json::json!({"error": e.to_string()}))
            }
        };

        let execution = ActionExecution {
            id: Uuid::new_v4(),
            tenant_id: event.tenant_id,
            trigger_id,
            event_id: event.id,
            action_type: action.action_type,
            status,
            executed_at: chrono::Utc::now(),
            detail,
        };

        deps.execution_repository
            .insert(&execution)
            .await
            .map_err(|e| ProcessError::ExecutionWrite(e.to_string()))?;
        executed += 1;
    }

    Ok(executed)
}
