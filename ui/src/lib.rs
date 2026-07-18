//! Console UI (spec §7): a server-rendered Rust web app (ADR-0014) — axum + askama, no WASM
//! build step, tested the same way as every other service in this repo
//! (`tower::ServiceExt::oneshot` against an in-process router).

mod agents_client;
mod auth_client;
mod connector_field_catalog;
mod events_client;
mod health_client;
mod ingestion_stats_client;
mod session;
mod session_guard;
mod triggers_client;

mod agent_detail_handler;
mod agent_script_handler;
mod agents_handler;
mod data_detail_handler;
mod data_handler;
mod events_handler;
mod health_handler;
mod healthz;
mod login_handler;
mod logout_handler;
mod reports_handler;
mod root_handler;
mod triggers_handler;

pub use agents_client::{AgentsClient, AgentsClientError, HttpAgentsClient};
pub use auth_client::{AuthClient, AuthClientError, HttpAuthClient};
pub use events_client::{EventSummary, EventsClient, EventsClientError, HttpEventsClient};
pub use health_client::{
    HealthClient, HealthClientError, HttpHealthClient, PlatformHealthSummary, ServiceHealthSummary,
};
pub use ingestion_stats_client::{
    ConnectorStatSummary, HttpIngestionStatsClient, IngestionStatsClient,
    IngestionStatsClientError, RecordSearchFilter, RecordSummary,
};
pub use session::{InMemorySessionStore, Session, SessionStore};
pub use triggers_client::{
    HttpTriggersClient, TriggerSummary, TriggersClient, TriggersClientError,
};

pub use agent_detail_handler::get_agent_detail;
pub use agent_script_handler::{get_generate_form, get_generate_select, post_generate_script};
pub use agents_handler::{get_agents, post_agents, post_delete_agent};
pub use data_detail_handler::get_data_detail;
pub use data_handler::get_data;
pub use events_handler::get_events;
pub use health_handler::get_health;
pub use healthz::healthz;
pub use login_handler::{get_login, post_login};
pub use logout_handler::get_logout;
pub use reports_handler::get_reports;
pub use root_handler::get_root;
pub use triggers_handler::get_triggers;

use axum::routing::get;
use axum::Router;
use std::sync::Arc;

pub const SESSION_COOKIE_NAME: &str = "kizashi_session";

#[derive(Clone)]
pub struct AppState {
    pub session_store: Arc<dyn SessionStore>,
    pub auth_client: Arc<dyn AuthClient>,
    pub events_client: Arc<dyn EventsClient>,
    pub triggers_client: Arc<dyn TriggersClient>,
    pub health_client: Arc<dyn HealthClient>,
    pub agents_client: Arc<dyn AgentsClient>,
    pub stats_client: Arc<dyn IngestionStatsClient>,
    /// The ingestion-gateway URL a *deployed connector* should point at — not necessarily
    /// reachable from inside this container (e.g. a customer-hosted connector polling in from
    /// outside the platform's own network), so it's a separate, operator-configurable value
    /// from `QUERY_GATEWAY_URL`/etc., which are all internal-network addresses.
    pub ingestion_gateway_public_url: String,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(get_root))
        .route("/healthz", get(healthz))
        .route("/login", get(get_login).post(post_login))
        .route("/logout", get(get_logout))
        .route("/events", get(get_events))
        .route("/triggers", get(get_triggers))
        .route("/health", get(get_health))
        .route("/agents", get(get_agents).post(post_agents))
        .route("/agents/generate", get(get_generate_select))
        .route("/agents/generate/form", get(get_generate_form))
        .route("/agents/generate/script", axum::routing::post(post_generate_script))
        .route("/agents/:id", get(get_agent_detail))
        .route("/agents/:id/delete", axum::routing::post(post_delete_agent))
        .route("/reports", get(get_reports))
        .route("/data", get(get_data))
        .route("/data/:id", get(get_data_detail))
        .with_state(state)
}
