//! Console UI (spec §7): a server-rendered Rust web app (ADR-0014) — axum + askama, no WASM
//! build step, tested the same way as every other service in this repo
//! (`tower::ServiceExt::oneshot` against an in-process router). Client-side JS is layered on
//! top for charts/components (ADR-0015, reversing ADR-0014's no-JS constraint) — every page
//! still server-renders its real data first, JS only progressively enhances it.

mod agents_client;
mod analysis_config_client;
mod api_keys_client;
mod auth_client;
mod backlog_client;
mod connector_field_catalog;
mod events_client;
mod execution_client;
mod health_client;
mod ingestion_stats_client;
mod normalization_mappings_client;
mod retention_policies_client;
mod session;
mod session_guard;
mod topology;
mod triggers_client;

mod agent_detail_handler;
mod agent_script_handler;
mod agents_handler;
mod analysis_config_handler;
mod api_keys_handler;
mod data_detail_handler;
mod data_handler;
mod events_handler;
mod health_handler;
mod healthz;
mod login_handler;
mod logout_handler;
mod normalization_mappings_handler;
mod overview_handler;
mod pipeline_handler;
mod record_journey_handler;
mod reports_handler;
mod retention_policies_handler;
mod root_handler;
mod static_assets;
mod triggers_handler;

pub use agents_client::{AgentsClient, AgentsClientError, HttpAgentsClient};
pub use analysis_config_client::{
    AnalysisConfigClient, AnalysisConfigClientError, AnalysisConfigView, HttpAnalysisConfigClient,
};
pub use api_keys_client::{ApiKeySummary, ApiKeysClient, ApiKeysClientError, HttpApiKeysClient};
pub use auth_client::{AuthClient, AuthClientError, HttpAuthClient};
pub use backlog_client::{BacklogClient, BacklogClientError, HttpBacklogClient, QueueDepthSummary};
pub use events_client::{EventSummary, EventsClient, EventsClientError, HttpEventsClient};
pub use execution_client::{
    ActionExecutionSummary, ExecutionClient, ExecutionClientError, HttpExecutionClient,
};
pub use health_client::{
    HealthClient, HealthClientError, HttpHealthClient, PlatformHealthSummary, ServiceHealthSummary,
};
pub use ingestion_stats_client::{
    ConnectorStatSummary, HttpIngestionStatsClient, IngestionStatsClient,
    IngestionStatsClientError, RecordSearchFilter, RecordSummary,
};
pub use normalization_mappings_client::{
    HttpNormalizationMappingsClient, NormalizationMappingsClient, NormalizationMappingsClientError,
};
pub use retention_policies_client::{
    DataClass, HttpRetentionPoliciesClient, RetentionPoliciesClient, RetentionPoliciesClientError,
    RetentionPolicy,
};
pub use session::{InMemorySessionStore, Session, SessionStore};
pub use triggers_client::{
    HttpTriggersClient, TriggerSummary, TriggersClient, TriggersClientError, TriggersPage,
};

pub use agent_detail_handler::get_agent_detail;
pub use agent_script_handler::{get_generate_form, get_generate_select, post_generate_script};
pub use agents_handler::{get_agents, post_agents, post_delete_agent, post_toggle_agent};
pub use analysis_config_handler::{get_analysis_config_page, post_analysis_config};
pub use api_keys_handler::{get_api_keys, post_api_keys, post_revoke_api_key};
pub use data_detail_handler::get_data_detail;
pub use data_handler::get_data;
pub use events_handler::get_events;
pub use health_handler::get_health;
pub use healthz::healthz;
pub use login_handler::{get_login, post_login};
pub use logout_handler::get_logout;
pub use normalization_mappings_handler::{get_normalization_mappings, post_normalization_mapping};
pub use overview_handler::get_overview;
pub use pipeline_handler::get_pipeline;
pub use record_journey_handler::get_record_journey;
pub use reports_handler::get_reports;
pub use retention_policies_handler::{
    get_retention_policies, post_delete_retention_policy, post_edit_retention_policy,
    post_retention_policies, post_toggle_retention_policy,
};
pub use root_handler::get_root;
pub use static_assets::get_charts_js;
pub use triggers_handler::{get_triggers, post_trigger};

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
    pub api_keys_client: Arc<dyn ApiKeysClient>,
    pub backlog_client: Arc<dyn BacklogClient>,
    pub execution_client: Arc<dyn ExecutionClient>,
    pub stats_client: Arc<dyn IngestionStatsClient>,
    pub analysis_config_client: Arc<dyn AnalysisConfigClient>,
    pub normalization_mappings_client: Arc<dyn NormalizationMappingsClient>,
    pub retention_policies_client: Arc<dyn RetentionPoliciesClient>,
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
        .route("/triggers", get(get_triggers).post(post_trigger))
        .route("/health", get(get_health))
        .route("/pipeline", get(get_pipeline))
        .route("/overview", get(get_overview))
        .route("/agents", get(get_agents).post(post_agents))
        .route("/agents/generate", get(get_generate_select))
        .route("/agents/generate/form", get(get_generate_form))
        .route("/agents/generate/script", axum::routing::post(post_generate_script))
        .route("/agents/:id", get(get_agent_detail))
        .route("/agents/:id/delete", axum::routing::post(post_delete_agent))
        .route("/agents/:id/toggle", axum::routing::post(post_toggle_agent))
        .route("/api-keys", get(get_api_keys).post(post_api_keys))
        .route("/api-keys/:id/revoke", axum::routing::post(post_revoke_api_key))
        .route("/analysis-config", get(get_analysis_config_page).post(post_analysis_config))
        .route(
            "/normalization-mappings",
            get(get_normalization_mappings).post(post_normalization_mapping),
        )
        .route("/retention-policies", get(get_retention_policies).post(post_retention_policies))
        .route("/retention-policies/:id/toggle", axum::routing::post(post_toggle_retention_policy))
        .route("/retention-policies/:id/edit", axum::routing::post(post_edit_retention_policy))
        .route("/retention-policies/:id/delete", axum::routing::post(post_delete_retention_policy))
        .route("/reports", get(get_reports))
        .route("/data", get(get_data))
        .route("/data/:id", get(get_data_detail))
        .route("/data/:id/journey", get(get_record_journey))
        .route("/static/charts.js", get(get_charts_js))
        .with_state(state)
}
