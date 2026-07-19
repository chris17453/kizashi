#[path = "agent_change_event_test.rs"]
#[cfg(test)]
mod agent_change_event_test;

use crate::Agent;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Published on `agent.changed` (ADR-0020) so Agent Scheduler's own copy of the Agent
/// registry — the one it actually walks to decide what's due to poll — stays in sync with
/// config-admin-service, same event-driven pattern as ADR-0018/ADR-0019. A tagged enum rather
/// than always publishing a full `Agent` because deletion has no `Agent` payload to carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentChangeEvent {
    Upserted(Agent),
    Deleted { id: Uuid, tenant_id: Uuid },
}
