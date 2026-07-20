#[path = "ops_handlers_test.rs"]
#[cfg(test)]
mod ops_handlers_test;

use crate::reimport::{reimport, ReimportState};
use crate::sweep::{sweep, SweepState};
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
#[cfg(test)]
use std::sync::Arc;

const DEFAULT_SWEEP_BATCH_LIMIT: i64 = 500;

/// Both endpoints in this file are service-to-service operational triggers (an external
/// CronJob-equivalent, ADR-0011 point 5) with no end user or session behind the call, so there's
/// no `X-Role` to check — only whether the caller knows the shared secret, same v1 stopgap as
/// query-gateway's `/internal/tokens` (ADR-0009). Previously these had *no* check at all: any
/// caller able to reach retention-service could trigger a tenant-wide sweep or force a reimport
/// of an arbitrary archive.
///
/// `pub(crate)` so `policy_handlers.rs` can apply the same gate to the retention-policy CRUD
/// routes (security audit finding: those routes trusted `X-Role`/`X-Tenant-Id`/`X-Username` at
/// face value with no verification the caller was actually the Console UI, since
/// docker-compose publishes this service's port directly).
pub(crate) fn has_valid_internal_secret(state: &AppState, headers: &HeaderMap) -> bool {
    let provided = headers.get("x-internal-secret").and_then(|v| v.to_str().ok());
    provided == Some(state.internal_secret.as_str())
}

/// POST /v1/sweep — triggers one retention sweep pass across every enabled policy. Sweeping is
/// externally scheduled (a Kubernetes CronJob or equivalent, ADR-0011 point 5) rather than an
/// in-process timer, so this is the only way a sweep runs — `retention-sweep-scheduler` in
/// `docker-compose.yml` is that "or equivalent" for the docker-compose deployment target.
pub async fn trigger_sweep(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !has_valid_internal_secret(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }

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
    headers: HeaderMap,
    Json(req): Json<ReimportRequest>,
) -> Response {
    if !has_valid_internal_secret(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }

    let reimport_state = ReimportState {
        archive_store: state.archive_store.clone(),
        record_client: state.record_client.clone(),
    };
    match reimport(&reimport_state, &req.archive_key).await {
        Ok(summary) => Json(summary).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}
