#[path = "record_search_handler_test.rs"]
#[cfg(test)]
mod record_search_handler_test;

use crate::ingest_handler::{IngestError, IngestState};
use crate::list_records_handler::tenant_id_from_headers;
use crate::raw_record_repository::RecordSearchFilter;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Json;
use chrono::{DateTime, Utc};
use common::{RawRecord, SourceType};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SearchRecordsQuery {
    pub connector_id: Option<String>,
    pub source_type: Option<SourceType>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub q: Option<String>,
    /// Substring match against an email-shaped record's `subject` (`common::EmailPayload`).
    pub subject: Option<String>,
    /// Substring match against an email-shaped record's `from` address.
    pub email_from: Option<String>,
    /// Substring match against any attachment's filename.
    pub attachment_filename: Option<String>,
    /// `false` finds records with no `normalized_payload` yet — the reprocess endpoint's
    /// "what's left to normalize" query.
    pub normalized: Option<bool>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchRecordsResponse {
    pub records: Vec<RawRecord>,
    /// True if there are more matching records beyond this page — computed by fetching one
    /// extra row rather than a separate `COUNT(*)` query, which would scan the same rows twice
    /// at exactly the scale (thousands of inboxes) pagination exists to handle.
    pub has_more: bool,
}

/// GET /v1/records/search — the Data Viewer's search: every filter is optional and AND-ed
/// (connector, source type, an ingested-at range, a substring match against the raw payload,
/// and, for email-shaped records, subject/from/attachment-filename matches). Tenant-scoped
/// like every other read path.
pub async fn search_records(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<SearchRecordsQuery>,
) -> Result<Json<SearchRecordsResponse>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let filter = RecordSearchFilter {
        connector_id: query.connector_id,
        source_type: query.source_type,
        from: query.from,
        to: query.to,
        query: query.q,
        subject: query.subject,
        email_from: query.email_from,
        attachment_filename: query.attachment_filename,
        normalized: query.normalized,
        limit: query.limit + 1,
        offset: query.offset,
    };
    let mut records = state
        .repository
        .search(tenant_id, &filter)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    let has_more = records.len() as i64 > query.limit;
    records.truncate(query.limit as usize);
    Ok(Json(SearchRecordsResponse { records, has_more }))
}

/// GET /v1/records/:id — the Data Viewer's record detail view (full raw + normalized payload).
pub async fn get_record(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<RawRecord>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let record = state
        .repository
        .get_by_id(tenant_id, id)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    record.map(Json).ok_or_else(|| IngestError::NotFound(format!("no record with id {id}")))
}
