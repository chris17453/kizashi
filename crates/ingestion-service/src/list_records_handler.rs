#[path = "list_records_handler_test.rs"]
#[cfg(test)]
mod list_records_handler_test;

use crate::ingest_handler::{IngestError, IngestState};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Json;
use chrono::{DateTime, Utc};
use common::RawRecord;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ListRecordsQuery {
    pub older_than: DateTime<Utc>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// GET /v1/records?older_than=<rfc3339>&limit=<n> — the read side of Retention/Archival
/// Service's sweep (spec §6, service #12). Retention Service never touches Postgres directly
/// (spec §2 principle 1); this is the HTTP-mediated path it archives through. Tenant-scoped via
/// `X-Tenant-Id`, same trust-boundary convention as dashboard-api/config-admin-service — one
/// tenant's sweep must never see or delete another tenant's records (spec §8, CLAUDE.md §5).
pub async fn list_records(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<ListRecordsQuery>,
) -> Result<Json<Vec<RawRecord>>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let records = state
        .repository
        .list_older_than(tenant_id, query.older_than, query.limit)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    Ok(Json(records))
}

pub(crate) fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, IngestError> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| IngestError::Validation("missing X-Tenant-Id header".to_string()))?;
    Uuid::parse_str(raw)
        .map_err(|_| IngestError::Validation("X-Tenant-Id is not a valid UUID".to_string()))
}
