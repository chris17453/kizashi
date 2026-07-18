#[path = "analyzed_record_test.rs"]
#[cfg(test)]
mod analyzed_record_test;

use crate::raw_record::RawRecord;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Wire shape of the `record.analyzed` bus message (spec §3): the RawRecord as normalized,
/// plus whatever Analysis Service's AI/ML call produced for it. Analysis results are not
/// persisted to their own table in v1 — they travel forward on the bus for the
/// Aggregation/Trigger Engine to evaluate directly, consistent with spec §2 principle 4
/// (decoupled, asynchronously connected stages) rather than adding a stage that must read
/// back through another service's API just to pass the result one hop further.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzedRecord {
    pub record: RawRecord,
    pub analysis: serde_json::Value,
    pub analyzed_at: DateTime<Utc>,
}

impl AnalyzedRecord {
    pub fn new(record: RawRecord, analysis: serde_json::Value) -> Self {
        Self { record, analysis, analyzed_at: Utc::now() }
    }
}
