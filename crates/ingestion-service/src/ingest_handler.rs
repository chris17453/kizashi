#[path = "ingest_handler_test.rs"]
#[cfg(test)]
mod ingest_handler_test;

use crate::event_publisher::EventPublisher;
use crate::raw_record_repository::RawRecordRepository;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use common::{RawRecord, SourceType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct IngestState {
    pub repository: Arc<dyn RawRecordRepository>,
    pub publisher: Arc<dyn EventPublisher>,
}

#[derive(Debug, Deserialize)]
pub struct NewRawRecordRequest {
    pub connector_id: String,
    pub source_type: SourceType,
    pub tenant_id: Uuid,
    pub raw_payload: serde_json::Value,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct IngestErrorBody {
    pub error: String,
}

pub enum IngestError {
    Validation(String),
    Storage(String),
}

impl IntoResponse for IngestError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            IngestError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
            IngestError::Storage(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(IngestErrorBody { error: message })).into_response()
    }
}

fn validate(req: &NewRawRecordRequest) -> Result<(), IngestError> {
    if req.connector_id.trim().is_empty() {
        return Err(IngestError::Validation("connector_id must not be empty".to_string()));
    }
    if req.tenant_id.is_nil() {
        return Err(IngestError::Validation("tenant_id must not be nil".to_string()));
    }
    if req.raw_payload.is_null() {
        return Err(IngestError::Validation("raw_payload must not be null".to_string()));
    }
    Ok(())
}

/// POST /v1/records — validates and persists a RawRecord, then publishes `record.ingested`.
/// The record is durably stored before publish is attempted; a publish failure is logged and
/// does not roll back the write, since the raw store (not the bus) is the source of truth
/// (spec §2 principle 6, "everything replayable") and downstream backlog/lag is Platform
/// Observability's job (spec §6, service #13) to surface, not this handler's to retry inline.
pub async fn ingest_record(
    State(state): State<IngestState>,
    Json(req): Json<NewRawRecordRequest>,
) -> Result<(StatusCode, Json<IngestResponse>), IngestError> {
    validate(&req)?;

    let mut record =
        RawRecord::new(req.connector_id, req.source_type, req.tenant_id, req.raw_payload);
    record.occurred_at = req.occurred_at;

    state.repository.insert(&record).await.map_err(|e| IngestError::Storage(e.to_string()))?;

    if let Err(e) = state.publisher.publish_record_ingested(&record).await {
        tracing::error!(record_id = %record.id, error = %e, "failed to publish record.ingested");
    }

    Ok((StatusCode::CREATED, Json(IngestResponse { id: record.id })))
}
