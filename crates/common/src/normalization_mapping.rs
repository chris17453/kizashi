#[path = "normalization_mapping_test.rs"]
#[cfg(test)]
mod normalization_mapping_test;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Config-store record: maps raw source fields (JSONPath-style strings) onto normalized
/// field names for a given `source_type` (spec §5.6). Config-over-code (spec §2 principle 5) —
/// this is operator-editable data, never a code change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizationMapping {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub source_type: String,
    pub field_map: BTreeMap<String, String>,
    pub version: i32,
}

impl NormalizationMapping {
    pub fn new(
        tenant_id: Uuid,
        source_type: impl Into<String>,
        field_map: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            source_type: source_type.into(),
            field_map,
            version: 1,
        }
    }

    /// Applies `field_map` to `raw_payload` using JSONPath-lite lookups (`$.foo.bar`), never
    /// panicking on missing paths or malformed operator config — a missing source field just
    /// produces `null` at the mapped destination.
    pub fn apply(&self, raw_payload: &serde_json::Value) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        for (dest_field, source_path) in &self.field_map {
            let value = resolve_json_path(raw_payload, source_path)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            out.insert(dest_field.clone(), value);
        }
        serde_json::Value::Object(out)
    }
}

fn resolve_json_path<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let path = path.strip_prefix("$.").unwrap_or(path.strip_prefix('$').unwrap_or(path));
    if path.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        current = current.get(segment)?;
    }
    Some(current)
}
