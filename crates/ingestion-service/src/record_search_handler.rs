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
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// GET /v1/records/search — the Data Viewer's search: every filter is optional and AND-ed
/// (connector, source type, an ingested-at range, and a substring match against the raw
/// payload). Tenant-scoped like every other read path.
pub async fn search_records(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<SearchRecordsQuery>,
) -> Result<Json<Vec<RawRecord>>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let filter = RecordSearchFilter {
        connector_id: query.connector_id,
        source_type: query.source_type,
        from: query.from,
        to: query.to,
        query: query.q,
        limit: query.limit,
    };
    let records = state
        .repository
        .search(tenant_id, &filter)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    Ok(Json(records))
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
