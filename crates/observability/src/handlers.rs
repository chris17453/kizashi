#[path = "handlers_test.rs"]
#[cfg(test)]
mod handlers_test;

use crate::backlog::BacklogReader;
use crate::platform_health::{
    check_platform_health, collect_service_metrics, ServiceHealthChecker, Status,
};
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

/// GET /v1/service-metrics — optional request/latency evidence from registered services.
/// Services that have not adopted the shared instrumentation are omitted rather than treated as
/// unhealthy; availability remains the responsibility of `/v1/health`.
pub async fn get_service_metrics(State(state): State<AppState>) -> Response {
    Json(collect_service_metrics(state.health_checker.as_ref(), &state.registry).await)
        .into_response()
}

/// GET /v1/backlog — pipeline backlog/lag visibility (ADR-0012).
pub async fn get_backlog(State(state): State<AppState>) -> Response {
    match state.backlog_reader.queue_depths().await {
        Ok(depths) => Json(depths).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /metrics — a Prometheus-compatible platform snapshot. This intentionally exports
/// bounded gauges derived from the same health/backlog reads as the operator console, rather
/// than pretending to provide per-request instrumentation that the v1 services do not yet own.
pub async fn get_metrics(State(state): State<AppState>) -> Response {
    let health = check_platform_health(state.health_checker.as_ref(), &state.registry).await;
    let backlog = state.backlog_reader.queue_depths().await.unwrap_or_default();
    let mut body = String::from(
        "# HELP kizashi_platform_up Whether every registered service is healthy.\n# TYPE kizashi_platform_up gauge\n",
    );
    body.push_str(&format!(
        "kizashi_platform_up {}\n",
        if health.status == Status::Up { 1 } else { 0 }
    ));
    body.push_str(
        "# HELP kizashi_service_up Whether a registered service health check is passing.\n# TYPE kizashi_service_up gauge\n",
    );
    for service in health.services {
        body.push_str(&format!(
            "kizashi_service_up{{service=\"{}\"}} {}\n",
            prometheus_label(&service.name),
            if service.status == Status::Up { 1 } else { 0 }
        ));
    }
    body.push_str(
        "# HELP kizashi_queue_messages Messages currently waiting at a pipeline boundary.\n# TYPE kizashi_queue_messages gauge\n",
    );
    for queue in backlog {
        body.push_str(&format!(
            "kizashi_queue_messages{{stage=\"{}\",queue=\"{}\"}} {}\n",
            prometheus_label(&queue.stage),
            prometheus_label(&queue.queue_name),
            queue.messages
        ));
    }
    ([(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], body)
        .into_response()
}

fn prometheus_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}
