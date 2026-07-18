#[path = "update_normalized_handler_test.rs"]
#[cfg(test)]
mod update_normalized_handler_test;

use crate::ingest_handler::{IngestError, IngestState};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct UpdateNormalizedPayloadRequest {
    pub normalized_payload: serde_json::Value,
}

/// PATCH /v1/records/:id/normalized — the only write path Normalization Service uses onto a
/// RawRecord it doesn't own storage for (spec §2 principle 1, "API-mediated everything": no
/// component reads or writes another component's database directly).
pub async fn update_normalized_payload(
    State(state): State<IngestState>,
    Path(record_id): Path<Uuid>,
    Json(req): Json<UpdateNormalizedPayloadRequest>,
) -> Result<StatusCode, IngestError> {
    let updated = state
        .repository
        .update_normalized_payload(record_id, &req.normalized_payload)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;

    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(IngestError::NotFound(format!("no record with id {record_id}")))
    }
}
