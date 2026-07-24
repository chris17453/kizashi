#[path = "ingest_handler_test.rs"]
#[cfg(test)]
mod ingest_handler_test;

use crate::event_publisher::EventPublisher;
use crate::list_records_handler::tenant_id_from_headers;
use crate::raw_record_repository::{RawRecordRepository, RecordSearchFilter};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
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
    #[serde(default)]
    pub external_id: Option<String>,
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
    NotFound(String),
}

impl IntoResponse for IngestError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            IngestError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
            IngestError::Storage(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            IngestError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
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
    record.external_id = req.external_id;

    let inserted =
        state.repository.insert(&record).await.map_err(|e| IngestError::Storage(e.to_string()))?;

    if inserted {
        if let Err(e) = state.publisher.publish_record_ingested(&record).await {
            tracing::error!(record_id = %record.id, error = %e, "failed to publish record.ingested");
        }
    } else {
        tracing::debug!(
            connector_id = %record.connector_id,
            external_id = ?record.external_id,
            "duplicate external_id, skipping insert and publish"
        );
    }

    Ok((StatusCode::CREATED, Json(IngestResponse { id: record.id })))
}

#[derive(Debug, Deserialize)]
pub struct ReprocessQuery {
    pub connector_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReprocessResponse {
    pub republished: usize,
}

/// POST /v1/records/reprocess — republishes `record.ingested` for every one of the caller's
/// tenant's records that has no `normalized_payload` yet (optionally scoped to one connector).
/// Closes a real gap: a record ingested before a `NormalizationMapping` existed for its source
/// type is silently skipped by Normalization Service (`ProcessOutcome::NoMappingConfigured`,
/// by design — not an error) and never gets a second chance without this. Re-publishing
/// (rather than calling normalization logic directly here) means this handler needs no
/// knowledge of mappings/normalization at all — the existing `record.ingested` consumer
/// (Normalization Service) picks these up exactly like a fresh poll would, so the rest of the
/// pipeline (analysis, triggers) is exercised unchanged. Bounded to `BATCH_LIMIT` records per
/// call so a huge backlog is swept in successive calls, not one unbounded request.
const REPROCESS_BATCH_LIMIT: i64 = 500;

pub async fn reprocess_records(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<ReprocessQuery>,
) -> Result<Json<ReprocessResponse>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let filter = RecordSearchFilter {
        connector_id: query.connector_id,
        normalized: Some(false),
        limit: REPROCESS_BATCH_LIMIT,
        ..Default::default()
    };
    let records = state
        .repository
        .search(tenant_id, &filter)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;

    let mut republished = 0;
    for record in &records {
        match state.publisher.publish_record_ingested(record).await {
            Ok(()) => republished += 1,
            Err(e) => {
                tracing::error!(record_id = %record.id, error = %e, "failed to republish record.ingested during reprocess");
            }
        }
    }

    Ok(Json(ReprocessResponse { republished }))
}

/// POST /v1/records/:id/reprocess — republishes one unnormalized record for targeted recovery.
/// The tenant header is part of the lookup, so a record id alone can never cross a tenant
/// boundary. Normalized records are left untouched and return `republished: 0`.
pub async fn reprocess_record(
    State(state): State<IngestState>,
    headers: HeaderMap,
    axum::extract::Path(record_id): axum::extract::Path<Uuid>,
) -> Result<Json<ReprocessResponse>, IngestError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let Some(record) = state
        .repository
        .get_by_id(tenant_id, record_id)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?
    else {
        return Err(IngestError::NotFound("record not found".to_string()));
    };
    if record.normalized_payload.is_some() {
        return Ok(Json(ReprocessResponse { republished: 0 }));
    }
    state
        .publisher
        .publish_record_ingested(&record)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    Ok(Json(ReprocessResponse { republished: 1 }))
}
