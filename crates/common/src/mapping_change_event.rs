#[path = "mapping_change_event_test.rs"]
#[cfg(test)]
mod mapping_change_event_test;

use crate::NormalizationMapping;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Published on `mapping.changed` (ADR-0110) so Normalization Service's own copy of the
/// mapping it actually applies stays in sync with config-admin-service, same event-driven
/// pattern as `SensorChangeEvent`/`TriggerChangeEvent`. A tagged enum rather than always
/// publishing a full `NormalizationMapping` because deletion has no mapping payload to carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingChangeEvent {
    Upserted(NormalizationMapping),
    Deleted { id: Uuid, tenant_id: Uuid },
}
