//! Dashboard/Query API Service (spec §6, service #9): reads Events from ClickHouse for
//! dashboards, reports, and event browsing. Trusts `X-Tenant-Id` as set by Query Gateway —
//! never derives identity itself (spec §8).

mod event_query_repository;
mod handlers;
mod health;

pub use event_query_repository::{
    ClickHouseEventQueryRepository, DailyEventCount, EventFilter, EventQueryRepository, QueryError,
};
pub use handlers::{daily_event_counts, get_event, list_events, DashboardState};

use axum::routing::get as axum_get;
use axum::Router;

pub fn build_router(state: DashboardState) -> Router {
    Router::new()
        .route("/healthz", axum_get(health::healthz))
        .route("/v1/events", axum_get(list_events))
        .route("/v1/events/daily-counts", axum_get(daily_event_counts))
        .route("/v1/events/:id", axum_get(get_event))
        .with_state(state)
}
