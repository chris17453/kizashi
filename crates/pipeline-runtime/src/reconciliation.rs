#[cfg(test)]
#[path = "reconciliation_test.rs"]
mod reconciliation_test;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Authoritative state to project only after the source system has confirmed a command.
/// Evidence remains a reference/descriptor, never an embedded source-system document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationProjection {
    pub object_id: Uuid,
    pub object_type_id: Uuid,
    pub source_version: String,
    pub properties: serde_json::Value,
    pub evidence: serde_json::Value,
}

pub fn parse_projection(
    value: &serde_json::Value,
) -> Result<ReconciliationProjection, &'static str> {
    let projection: ReconciliationProjection = serde_json::from_value(
        value.get("reconciliation").cloned().ok_or("confirmation requires reconciliation")?,
    )
    .map_err(|_| "reconciliation payload is invalid")?;
    if projection.source_version.trim().is_empty() {
        return Err("reconciliation source_version is required");
    }
    if !projection.properties.is_object() {
        return Err("reconciliation properties must be an object");
    }
    if !projection.evidence.is_object() {
        return Err("reconciliation evidence must be an object");
    }
    Ok(projection)
}
