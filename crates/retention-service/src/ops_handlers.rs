#[path = "ops_handlers_test.rs"]
#[cfg(test)]
mod ops_handlers_test;

use crate::reimport::{reimport, ReimportState};
use crate::sweep::{sweep, SweepState};
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
#[cfg(test)]
use std::sync::Arc;

const DEFAULT_SWEEP_BATCH_LIMIT: i64 = 500;

/// POST /v1/sweep — triggers one retention sweep pass across every enabled policy. Sweeping is
/// externally scheduled (a Kubernetes CronJob or equivalent, ADR-0011 point 5) rather than an
/// in-process timer, so this is the only way a sweep runs.
pub async fn trigger_sweep(State(state): State<AppState>) -> Response {
    let sweep_state = SweepState {
        policy_repository: state.policy_repository.clone(),
        record_client: state.record_client.clone(),
        archive_store: state.archive_store.clone(),
    };
    match sweep(&sweep_state, chrono::Utc::now(), DEFAULT_SWEEP_BATCH_LIMIT).await {
        Ok(summary) => Json(summary).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ReimportRequest {
    pub archive_key: String,
}

/// POST /v1/reimport — replays one archived batch back through Ingestion Service (spec §9).
pub async fn trigger_reimport(
    State(state): State<AppState>,
    Json(req): Json<ReimportRequest>,
) -> Response {
    let reimport_state = ReimportState {
        archive_store: state.archive_store.clone(),
        record_client: state.record_client.clone(),
    };
    match reimport(&reimport_state, &req.archive_key).await {
        Ok(summary) => Json(summary).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}
