#[path = "execution_handlers_test.rs"]
#[cfg(test)]
mod execution_handlers_test;

use crate::execution_repository::ExecutionRepository;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ExecutionState {
    pub execution_repository: Arc<dyn ExecutionRepository>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Trusts `X-Tenant-Id` the same way config-admin-service/retention-service/dashboard-api do —
/// action-executor has no gateway in front of it either, and this is its first read/query
/// endpoint (previously a pure RabbitMQ consumer with only `/healthz`).
fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

#[derive(Debug, serde::Deserialize)]
pub struct ListExecutionsQuery {
    pub event_id: Uuid,
}

/// GET /v1/action-executions?event_id=X — every execution (including retries) for one Event,
/// tenant-scoped. The event→action hop of the platform's full data lineage (ADR-0017); what a
/// record-journey/link-analysis view in Console UI reads to show what happened after an Event
/// fired.
pub async fn list_executions(
    State(state): State<ExecutionState>,
    headers: HeaderMap,
    Query(query): Query<ListExecutionsQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.execution_repository.list_by_event(tenant_id, query.event_id).await {
        Ok(executions) => Json(executions).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub fn build_router(state: ExecutionState) -> axum::Router {
    axum::Router::new()
        .route("/v1/action-executions", axum::routing::get(list_executions))
        .with_state(state)
}
