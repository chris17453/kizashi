#[path = "trigger_change_event_test.rs"]
#[cfg(test)]
mod trigger_change_event_test;

use crate::TriggerDefinition;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Published on `trigger.changed` (ADR-0018, ADR-0109) so Trigger Engine's own copy of the
/// trigger definitions it actually evaluates stays in sync with config-admin-service, same
/// event-driven pattern as `SensorChangeEvent`/Agent Scheduler. A tagged enum rather than
/// always publishing a full `TriggerDefinition` because deletion has no `TriggerDefinition`
/// payload to carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerChangeEvent {
    Upserted(TriggerDefinition),
    Deleted { id: Uuid, tenant_id: Uuid },
}
