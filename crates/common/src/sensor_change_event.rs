#[path = "sensor_change_event_test.rs"]
#[cfg(test)]
mod sensor_change_event_test;

use crate::Sensor;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Published on `sensor.changed` (ADR-0020, ADR-0036) so Agent Scheduler's own copy of the
/// Sensor registry — the one it actually walks to decide what's due to poll — stays in sync
/// with config-admin-service, same event-driven pattern as ADR-0018/ADR-0019. A tagged enum
/// rather than always publishing a full `Sensor` because deletion has no `Sensor` payload to
/// carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SensorChangeEvent {
    Upserted(Sensor),
    Deleted { id: Uuid, tenant_id: Uuid },
}
