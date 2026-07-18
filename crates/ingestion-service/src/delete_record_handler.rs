#[path = "delete_record_handler_test.rs"]
#[cfg(test)]
mod delete_record_handler_test;

use crate::ingest_handler::{IngestError, IngestState};
use crate::list_records_handler::tenant_id_from_headers;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use uuid::Uuid;

/// DELETE /v1/records/:id — hard-deletes a RawRecord after Retention/Archival Service has
/// durably archived it (spec §9 disposal). Retention Service never touches Postgres directly
/// (spec §2 principle 1); this is the only path a record can be removed from the hot store.
/// Tenant-scoped via `X-Tenant-Id` so one tenant can never delete another tenant's record
/// (spec §8, CLAUDE.md §5).
pub async fn delete_record(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let deleted = state
        .repository
        .delete(tenant_id, id)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(IngestError::NotFound(format!("no record with id {id}")))
    }
}
