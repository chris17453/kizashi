#[path = "saved_search_query_test.rs"]
#[cfg(test)]
mod saved_search_query_test;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tenant-wide bookmark of a `/data` page search filter (spec §7 "saved queries/views",
/// ADR-0029). `filter` is opaque JSON matching the Console UI's own `RecordSearchFilter` shape
/// — this crate doesn't need to know its fields, only store/return them, the same config-as-
/// data convention `Sensor.config`/`NormalizationMapping.field_map` already use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSearchQuery {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub filter: serde_json::Value,
}

impl SavedSearchQuery {
    pub fn new(tenant_id: Uuid, name: impl Into<String>, filter: serde_json::Value) -> Self {
        Self { id: Uuid::new_v4(), tenant_id, name: name.into(), filter }
    }
}
