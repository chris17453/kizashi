#[path = "raw_record_test.rs"]
#[cfg(test)]
mod raw_record_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The stable, generic envelope every connector writes to, regardless of source type.
/// This schema must not change as new source types are added (spec §5.1) — structure is
/// imposed downstream by the Normalization Service, never at ingest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawRecord {
    pub id: Uuid,
    pub connector_id: String,
    pub source_type: SourceType,
    pub ingested_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<DateTime<Utc>>,
    pub raw_payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_payload: Option<serde_json::Value>,
    pub tenant_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Message,
    Ticket,
    Log,
    SqlRow,
    FabricRecord,
    Generic,
}

impl RawRecord {
    pub fn new(
        connector_id: impl Into<String>,
        source_type: SourceType,
        tenant_id: Uuid,
        raw_payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            connector_id: connector_id.into(),
            source_type,
            ingested_at: Utc::now(),
            occurred_at: None,
            raw_payload,
            normalized_payload: None,
            tenant_id,
        }
    }

    pub fn is_normalized(&self) -> bool {
        self.normalized_payload.is_some()
    }
}
