#[path = "handlers_test.rs"]
#[cfg(test)]
mod handlers_test;

use crate::backlog::BacklogReader;
use crate::platform_health::{check_platform_health, ServiceHealthChecker, Status};
use crate::service_registry::ServiceEndpoint;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub health_checker: Arc<dyn ServiceHealthChecker>,
    pub registry: Arc<Vec<ServiceEndpoint>>,
    pub backlog_reader: Arc<dyn BacklogReader>,
}

/// GET /v1/health — platform-wide health aggregation (ADR-0012). Returns 503 rather than 200
/// when any service is down, so this endpoint itself is usable as a single liveness check by
/// an external monitor, not just a JSON report a human has to read.
pub async fn get_platform_health(State(state): State<AppState>) -> Response {
    let health = check_platform_health(state.health_checker.as_ref(), &state.registry).await;
    let status_code =
        if health.status == Status::Up { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status_code, Json(health)).into_response()
}

/// GET /v1/backlog — pipeline backlog/lag visibility (ADR-0012).
pub async fn get_backlog(State(state): State<AppState>) -> Response {
    match state.backlog_reader.queue_depths().await {
        Ok(depths) => Json(depths).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
