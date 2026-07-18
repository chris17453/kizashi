#[path = "connector_stats_handler_test.rs"]
#[cfg(test)]
mod connector_stats_handler_test;

use crate::ingest_handler::{IngestError, IngestState};
use crate::list_records_handler::tenant_id_from_headers;
use crate::raw_record_repository::ConnectorStats;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Json;
use common::RawRecord;
use serde::Deserialize;

/// GET /v1/records/stats — per-connector aggregate (record count, last ingested time), tenant-
/// scoped, powering Agent status in the Console UI. There is no separate "agent run"
/// bookkeeping table: a connector's own ingested records are the ground truth for whether/when
/// it ran.
pub async fn get_connector_stats(
    State(state): State<IngestState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConnectorStats>>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let stats = state
        .repository
        .stats_by_connector(tenant_id)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    Ok(Json(stats))
}

#[derive(Debug, Deserialize)]
pub struct ListByConnectorQuery {
    pub connector_id: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// GET /v1/records/by-connector?connector_id=<id>&limit=<n> — the per-agent data drill-down:
/// the most recent raw records a given connector has ingested, tenant-scoped, newest first.
pub async fn list_records_by_connector(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<ListByConnectorQuery>,
) -> Result<Json<Vec<RawRecord>>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let records = state
        .repository
        .list_by_connector(tenant_id, &query.connector_id, query.limit)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    Ok(Json(records))
}
