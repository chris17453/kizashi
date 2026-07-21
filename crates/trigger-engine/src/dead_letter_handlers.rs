#[path = "dead_letter_handlers_test.rs"]
#[cfg(test)]
mod dead_letter_handlers_test;

use crate::dead_letter::DeadLetterManager;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct DeadLetterState {
    pub dead_letter_manager: Arc<dyn DeadLetterManager>,
    pub internal_secret: String,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Both endpoints here are service-to-service operational actions with no end user or session
/// behind the call, same shape and rationale as retention-service's `has_valid_internal_secret`
/// (ADR-0011/§5 finding: an operational trigger with no caller-identity check at all is a real
/// gap, not a stopgap worth deferring).
fn has_valid_internal_secret(state: &DeadLetterState, headers: &HeaderMap) -> bool {
    let provided = headers.get("x-internal-secret").and_then(|v| v.to_str().ok());
    provided == Some(state.internal_secret.as_str())
}

pub fn build_router(state: DeadLetterState) -> Router {
    Router::new()
        .route("/v1/dead-letter", get(get_dead_letter_count))
        .route("/v1/dead-letter/replay", post(post_dead_letter_replay))
        .with_state(state)
}

#[derive(serde::Serialize)]
struct DeadLetterCountResponse {
    count: u32,
}

/// GET /v1/dead-letter — how many messages are currently sitting in this service's dead-letter
/// queue (populated by `retry.rs` once a message exceeds `MAX_RETRIES`), so an operator has any
/// visibility into it at all -- previously none existed.
pub async fn get_dead_letter_count(
    State(state): State<DeadLetterState>,
    headers: HeaderMap,
) -> Response {
    if !has_valid_internal_secret(&state, &headers) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid internal secret");
    }
    match state.dead_letter_manager.count().await {
        Ok(count) => Json(DeadLetterCountResponse { count }).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dead letter count failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

#[derive(serde::Serialize)]
struct DeadLetterReplayResponse {
    replayed: bool,
}

/// POST /v1/dead-letter/replay — moves the oldest dead-lettered message back onto the main
/// queue with a fresh retry budget. One message per call, deliberately: replaying an entire
/// backlog automatically the moment an operator asks about it risks re-dead-lettering an
/// unbounded batch right back if the underlying cause isn't actually fixed yet. Repeated calls
/// (or a small client-side loop) replay more than one.
pub async fn post_dead_letter_replay(
    State(state): State<DeadLetterState>,
    headers: HeaderMap,
) -> Response {
    if !has_valid_internal_secret(&state, &headers) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid internal secret");
    }
    match state.dead_letter_manager.replay_oldest().await {
        Ok(replayed) => Json(DeadLetterReplayResponse { replayed }).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dead letter replay failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}
