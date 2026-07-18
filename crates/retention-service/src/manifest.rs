#[path = "manifest_test.rs"]
#[cfg(test)]
mod manifest_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The one-line JSON header every archive file starts with (ADR-0005) — distinct from a
/// `RawRecord`-shaped envelope so readers can always tell manifest from data on line 1.
/// `format_version` lets the envelope evolve later without breaking readers of already-archived
/// files; reimport dispatches on it (currently only `1` exists).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub format_version: u32,
    pub tenant_id: Uuid,
    pub data_class: String,
    pub record_count: usize,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub schema_version: String,
}

pub const CURRENT_FORMAT_VERSION: u32 = 1;
/// The `common` crate schema version archived records are serialized with (ADR-0005) — bumped
/// by hand alongside any future breaking change to `RawRecord`'s shape.
pub const COMMON_SCHEMA_VERSION: &str = "1";

impl ArchiveManifest {
    pub fn new(
        tenant_id: Uuid,
        data_class: impl Into<String>,
        record_count: usize,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            tenant_id,
            data_class: data_class.into(),
            record_count,
            window_start,
            window_end,
            schema_version: COMMON_SCHEMA_VERSION.to_string(),
        }
    }
}

/// Builds the `archive/<tenant_id>/<data_class>/<yyyy>/<mm>/<dd>/<batch_id>.ndjson.gz` object
/// key ADR-0005 specifies.
pub fn archive_key(
    tenant_id: Uuid,
    data_class: &str,
    window_end: DateTime<Utc>,
    batch_id: Uuid,
) -> String {
    format!(
        "archive/{tenant_id}/{data_class}/{}/{batch_id}.ndjson.gz",
        window_end.format("%Y/%m/%d"),
    )
}
