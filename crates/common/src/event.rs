#[path = "event_test.rs"]
#[cfg(test)]
mod event_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Aggregate-tier record (ClickHouse), written by the Aggregation/Trigger Engine when a
/// TriggerDefinition's condition is satisfied (spec §5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub source_connector_ids: Vec<String>,
    pub entity_ref: String,
    pub group_key: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub status: EventStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    New,
    Triggered,
    Actioned,
    Dismissed,
}

impl Event {
    pub fn new(
        tenant_id: Uuid,
        event_type: impl Into<String>,
        entity_ref: impl Into<String>,
        group_key: impl Into<String>,
        payload: serde_json::Value,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: event_type.into(),
            source_connector_ids: Vec::new(),
            entity_ref: entity_ref.into(),
            group_key: group_key.into(),
            payload,
            occurred_at,
            created_at: Utc::now(),
            status: EventStatus::New,
        }
    }

    pub fn is_actionable(&self) -> bool {
        matches!(self.status, EventStatus::New | EventStatus::Triggered)
    }
}
