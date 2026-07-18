//! Platform Observability (spec §6, service #13): platform-wide health aggregation and
//! pipeline backlog/lag visibility. See ADR-0012 for v1 scope (per-service `/metrics`
//! request/latency instrumentation is deliberately deferred, not silently missing).

mod backlog;
mod handlers;
mod health;
mod pipeline_queues;
mod platform_health;
mod service_registry;

pub use backlog::{BacklogError, BacklogReader, QueueDepth, RabbitMqManagementBacklogReader};
pub use handlers::{get_backlog, get_platform_health, AppState};
pub use health::healthz;
pub use pipeline_queues::PIPELINE_QUEUES;
pub use platform_health::{
    check_platform_health, HttpServiceHealthChecker, PlatformHealth, ServiceHealth,
    ServiceHealthChecker, Status,
};
pub use service_registry::{parse_registry, ServiceEndpoint};

use axum::routing::get;
use axum::Router;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/health", get(get_platform_health))
        .route("/v1/backlog", get(get_backlog))
        .with_state(state)
}
