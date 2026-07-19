#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

use crate::allowlist::AllowlistRepository;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct AdminState {
    pub allowlist_repository: Arc<dyn AllowlistRepository>,
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))
}

/// GET /v1/allowlist — the calling tenant's configured domain allowlist, `[]` meaning "no
/// restriction configured" (ADR-0021).
async fn get_allowlist(State(state): State<AdminState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.allowlist_repository.get_domains(&tenant_id).await {
        Ok(domains) => Json(domains).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(serde::Deserialize)]
struct PutAllowlistBody {
    domains: Vec<String>,
}

/// PUT /v1/allowlist — replaces the calling tenant's domain allowlist wholesale (not a
/// per-domain add/remove API — the list is small and operator-managed, replace-the-whole-thing
/// matches how `AnalysisConfig`'s single prompt is edited).
async fn put_allowlist(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<PutAllowlistBody>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.allowlist_repository.set_domains(&tenant_id, body.domains.clone()).await {
        Ok(()) => Json(body.domains).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub fn build_router(state: AdminState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/allowlist", get(get_allowlist).put(put_allowlist))
        .with_state(state)
}
