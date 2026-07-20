#[path = "api_test.rs"]
#[cfg(test)]
mod api_test;

use crate::internal_secret::require_internal_secret;
use crate::process_analyzed_record::evaluate_trigger;
use crate::signal_repository::SignalRepository;
use crate::trigger_repository::TriggerRepository;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub trigger_repository: Arc<dyn TriggerRepository>,
    pub signal_repository: Arc<dyn SignalRepository>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

/// GET /v1/triggers/:id — the API-mediated read path onto TriggerDefinition storage (spec §2
/// principle 1). Action Executor calls this to resolve which actions to run for a firing
/// event, instead of reading Trigger Engine's Postgres schema directly. Tenant-scoped the same
/// way `test_trigger` below already was: a caller's `X-Tenant-Id` must match the trigger's own
/// tenant_id, reported as 404 (not 403) on mismatch so a caller can't distinguish "wrong
/// tenant" from "doesn't exist" and enumerate other tenants' trigger ids.
async fn get_trigger(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.trigger_repository.get_by_id(id).await {
        Ok(Some(trigger)) if trigger.tenant_id == tenant_id => Json(trigger).into_response(),
        Ok(_) => {
            (StatusCode::NOT_FOUND, Json(ErrorBody { error: format!("no trigger with id {id}") }))
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorBody { error: e.to_string() }))
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
struct TestTriggerRequest {
    group_key: String,
}

#[derive(serde::Serialize)]
struct TestTriggerResponse {
    would_fire: bool,
    contributing_record_count: usize,
}

/// POST /v1/triggers/:id/test — a dry run (ADR-0030): "would this trigger fire right now for
/// this group_key," evaluated against real, already-recorded signal history via the exact same
/// `evaluate_trigger` the live `record.analyzed` path uses, so it can't drift from production
/// behavior. Never writes an `Event` or runs any action — read-only, no `require_operator`
/// gate, since checking whether a trigger *would* fire isn't a write path.
async fn test_trigger(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<TestTriggerRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    let trigger = match state.trigger_repository.get_by_id(id).await {
        Ok(Some(trigger)) => trigger,
        Ok(None) => {
            return error_response(StatusCode::NOT_FOUND, format!("no trigger with id {id}"))
        }
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if trigger.tenant_id != tenant_id {
        return error_response(StatusCode::NOT_FOUND, format!("no trigger with id {id}"));
    }

    match evaluate_trigger(
        &state.signal_repository,
        &trigger,
        tenant_id,
        &trigger.event_type_match,
        &req.group_key,
    )
    .await
    {
        Ok((would_fire, record_ids)) => {
            Json(TestTriggerResponse { would_fire, contributing_record_count: record_ids.len() })
                .into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// `internal_secret` gates every route on this router (ADR-0009 shared-secret pattern) via a
/// single router-wide middleware layer, so a future handler added here can't accidentally skip
/// the check the way a per-handler copy-paste could. `/healthz` is intentionally not part of
/// this router at all (see `health::build_router` / `main.rs`), so Docker's zero-header
/// healthcheck is unaffected.
pub fn build_router(state: ApiState, internal_secret: String) -> Router {
    Router::new()
        .route("/v1/triggers/:id", get(get_trigger))
        .route("/v1/triggers/:id/test", post(test_trigger))
        .layer(axum::middleware::from_fn_with_state(internal_secret, require_internal_secret))
        .with_state(state)
}
