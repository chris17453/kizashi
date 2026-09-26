#[path = "dedup_handlers_test.rs"]
#[cfg(test)]
mod dedup_handlers_test;

use crate::FingerprintRepository;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct DedupState {
    pub fingerprint_repository: Arc<dyn FingerprintRepository>,
    pub internal_secret: String,
}

fn authorized(state: &DedupState, headers: &HeaderMap) -> bool {
    headers.get("x-internal-secret").and_then(|value| value.to_str().ok())
        == Some(state.internal_secret.as_str())
}

pub fn build_router(state: DedupState) -> Router {
    Router::new().route("/v1/dedup/summary", get(get_dedup_summary)).with_state(state)
}

pub async fn get_dedup_summary(State(state): State<DedupState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }
    let Some(tenant_id) = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Uuid>().ok())
    else {
        return (StatusCode::BAD_REQUEST, "missing or invalid tenant id").into_response();
    };
    match state.fingerprint_repository.summary(tenant_id).await {
        Ok(summary) => Json(summary).into_response(),
        Err(error) => {
            tracing::error!(%error, %tenant_id, "dedup summary failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "dedup summary unavailable").into_response()
        }
    }
}
