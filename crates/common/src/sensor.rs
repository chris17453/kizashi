#[path = "sensor_test.rs"]
#[cfg(test)]
mod sensor_test;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A registered instance of a connector for a tenant (spec §6 connectors) — the record that
/// previously didn't exist anywhere in the system. `connector_type` matches a
/// `crates/connectors/*` crate name (`zendesk`, `graph_mail`, `graph_teams`, `sql`, `fabric`,
/// `generic`); `config` is that connector's own JSON config shape, opaque to everything except
/// the connector binary itself, matching NormalizationMapping's config-over-code convention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sensor {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub connector_type: String,
    pub name: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

impl Sensor {
    pub fn new(
        tenant_id: Uuid,
        connector_type: impl Into<String>,
        name: impl Into<String>,
        config: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            connector_type: connector_type.into(),
            name: name.into(),
            config,
            enabled: true,
        }
    }
}
