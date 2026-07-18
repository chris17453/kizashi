#[path = "event_type_definition_test.rs"]
#[cfg(test)]
mod event_type_definition_test;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Config-store record defining the shape of `Event.payload` for a given `event_type`
/// (spec §5.3). Versioned so operators can evolve event shapes without breaking existing
/// consumers reading older, already-written events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventTypeDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub field_schema: serde_json::Value,
    pub version: i32,
}

impl EventTypeDefinition {
    pub fn new(tenant_id: Uuid, name: impl Into<String>, field_schema: serde_json::Value) -> Self {
        Self { id: Uuid::new_v4(), tenant_id, name: name.into(), field_schema, version: 1 }
    }

    pub fn next_version(&self, field_schema: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id: self.tenant_id,
            name: self.name.clone(),
            field_schema,
            version: self.version + 1,
        }
    }
}
