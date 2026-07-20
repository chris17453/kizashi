//! Console UI (spec §7): a server-rendered Rust web app (ADR-0014) — axum + askama, no WASM
//! build step, tested the same way as every other service in this repo
//! (`tower::ServiceExt::oneshot` against an in-process router). Client-side JS is layered on
//! top for charts/components (ADR-0015, reversing ADR-0014's no-JS constraint) — every page
//! still server-renders its real data first, JS only progressively enhances it.

mod analysis_config_client;
mod api_keys_client;
mod audit_log_client;
mod auth_client;
mod backlog_client;
mod branding_client;
mod connector_field_catalog;
mod cookie_security;
mod egress_allowlist_client;
mod events_client;
mod execution_client;
mod health_client;
mod ingestion_stats_client;
mod normalization_mappings_client;
mod oidc_client;
mod pending_oidc_flow;
mod retention_policies_client;
mod saved_search_queries_client;
mod sensors_client;
mod session;
mod session_guard;
mod topology;
mod triggers_client;
mod users_client;

mod analysis_config_handler;
mod api_keys_handler;
mod audit_log_handler;
mod branding_handler;
mod data_detail_handler;
mod data_handler;
mod egress_allowlist_handler;
mod events_handler;
mod health_handler;
mod healthz;
mod login_handler;
mod logout_handler;
mod normalization_mappings_handler;
mod overview_handler;
mod permissions_reference_handler;
mod pipeline_handler;
mod recent_audit_log_handler;
mod record_journey_handler;
mod reports_handler;
mod retention_policies_handler;
mod root_handler;
mod security_overview_handler;
mod sensor_detail_handler;
mod sensor_script_handler;
mod sensors_handler;
mod sessions_handler;
mod sso_login_handler;
mod static_assets;
mod triggers_handler;
mod users_handler;

pub use analysis_config_client::{
    AnalysisConfigClient, AnalysisConfigClientError, AnalysisConfigView, HttpAnalysisConfigClient,
};
pub use api_keys_client::{ApiKeySummary, ApiKeysClient, ApiKeysClientError, HttpApiKeysClient};
pub use audit_log_client::{
    AuditLogClient, AuditLogClientError, AuditLogEntry, HttpAuditLogClient,
};
pub use auth_client::{AuthClient, AuthClientError, HttpAuthClient};
pub use backlog_client::{BacklogClient, BacklogClientError, HttpBacklogClient, QueueDepthSummary};
pub use branding_client::{Branding, BrandingClient, BrandingClientError, HttpBrandingClient};
pub use cookie_security::{cookie_secure, cookie_secure_suffix};
pub use egress_allowlist_client::{
    EgressAllowlistClient, EgressAllowlistClientError, HttpEgressAllowlistClient,
};
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
pub use oidc_client::{HttpOidcClient, OidcClient, OidcClientError};
pub use pending_oidc_flow::{InMemoryPendingOidcFlowStore, PendingOidcFlow, PendingOidcFlowStore};
pub use retention_policies_client::{
    DataClass, HttpRetentionPoliciesClient, RetentionPoliciesClient, RetentionPoliciesClientError,
    RetentionPolicy,
};
pub use saved_search_queries_client::{
    HttpSavedSearchQueriesClient, SavedSearchQueriesClient, SavedSearchQueriesClientError,
};
pub use sensors_client::{HttpSensorsClient, SensorsClient, SensorsClientError};
pub use session::{InMemorySessionStore, Session, SessionStore};
pub use triggers_client::{
    HttpTriggersClient, TriggerSummary, TriggerTestResult, TriggersClient, TriggersClientError,
    TriggersPage,
};
pub use users_client::{HttpUsersClient, UiUser, UsersClient, UsersClientError};

pub use analysis_config_handler::{get_analysis_config_page, post_analysis_config};
pub use api_keys_handler::{get_api_keys, post_api_keys, post_revoke_api_key};
pub use audit_log_handler::get_audit_log as get_entity_audit_log;
pub use branding_handler::{get_branding_page, post_branding};
pub use data_detail_handler::get_data_detail;
pub use data_handler::{get_data, post_delete_saved_search, post_reprocess, post_save_search};
pub use egress_allowlist_handler::{get_egress_allowlist, post_egress_allowlist};
pub use events_handler::get_events;
pub use health_handler::get_health;
pub use healthz::healthz;
pub use login_handler::{get_login, post_login};
pub use logout_handler::get_logout;
pub use normalization_mappings_handler::{get_normalization_mappings, post_normalization_mapping};
pub use overview_handler::get_overview;
pub use permissions_reference_handler::get_permissions_reference;
pub use pipeline_handler::get_pipeline;
pub use recent_audit_log_handler::{get_recent_audit_log, get_recent_audit_log_csv};
pub use record_journey_handler::get_record_journey;
pub use reports_handler::get_reports;
pub use retention_policies_handler::{
    get_retention_policies, post_delete_retention_policy, post_edit_retention_policy,
    post_retention_policies, post_toggle_retention_policy,
};
pub use root_handler::get_root;
pub use security_overview_handler::get_security_overview;
pub use sensor_detail_handler::get_sensor_detail;
pub use sensor_script_handler::{get_generate_form, get_generate_select, post_generate_script};
pub use sensors_handler::{get_sensors, post_delete_sensor, post_sensors, post_toggle_sensor};
pub use sessions_handler::{get_sessions, post_revoke_session};
pub use sso_login_handler::{get_sso_callback, get_sso_login};
pub use static_assets::get_charts_js;
pub use triggers_handler::{get_triggers, post_trigger};
pub use users_handler::{get_users, post_delete_user, post_update_user_role, post_users};

use axum::routing::get;
use axum::Router;
use std::sync::Arc;

pub const SESSION_COOKIE_NAME: &str = "kizashi_session";

#[derive(Clone)]
pub struct AppState {
    pub session_store: Arc<dyn SessionStore>,
    pub auth_client: Arc<dyn AuthClient>,
    pub branding_client: Arc<dyn BrandingClient>,
    pub oidc_client: Arc<dyn OidcClient>,
    pub pending_oidc_flow_store: Arc<dyn PendingOidcFlowStore>,
    pub events_client: Arc<dyn EventsClient>,
    pub triggers_client: Arc<dyn TriggersClient>,
    pub health_client: Arc<dyn HealthClient>,
    pub sensors_client: Arc<dyn SensorsClient>,
    pub api_keys_client: Arc<dyn ApiKeysClient>,
    pub backlog_client: Arc<dyn BacklogClient>,
    pub execution_client: Arc<dyn ExecutionClient>,
    pub stats_client: Arc<dyn IngestionStatsClient>,
    pub analysis_config_client: Arc<dyn AnalysisConfigClient>,
    pub normalization_mappings_client: Arc<dyn NormalizationMappingsClient>,
    pub retention_policies_client: Arc<dyn RetentionPoliciesClient>,
    pub egress_allowlist_client: Arc<dyn EgressAllowlistClient>,
    pub users_client: Arc<dyn UsersClient>,
    pub saved_search_queries_client: Arc<dyn SavedSearchQueriesClient>,
    /// All three fields hold an `Arc<dyn AuditLogClient>` built from the *same*
    /// `HttpAuditLogClient` implementation, just constructed with a different backend base
    /// URL — `config-admin-service`, `retention-service`, and `auth-service` all expose an
    /// identically shaped `GET /v1/audit-log/:entity_id`, so one client type covers all three.
    pub config_audit_log_client: Arc<dyn AuditLogClient>,
    pub retention_audit_log_client: Arc<dyn AuditLogClient>,
    pub auth_audit_log_client: Arc<dyn AuditLogClient>,
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
        .route("/login/sso", get(get_sso_login))
        .route("/login/sso/callback", get(get_sso_callback))
        .route("/logout", get(get_logout))
        .route("/events", get(get_events))
        .route("/triggers", get(get_triggers).post(post_trigger))
        .route("/health", get(get_health))
        .route("/branding", get(get_branding_page).post(post_branding))
        .route("/pipeline", get(get_pipeline))
        .route("/overview", get(get_overview))
        .route("/sensors", get(get_sensors).post(post_sensors))
        .route("/sensors/generate", get(get_generate_select))
        .route("/sensors/generate/form", get(get_generate_form))
        .route("/sensors/generate/script", axum::routing::post(post_generate_script))
        .route("/sensors/:id", get(get_sensor_detail))
        .route("/sensors/:id/delete", axum::routing::post(post_delete_sensor))
        .route("/sensors/:id/toggle", axum::routing::post(post_toggle_sensor))
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
        .route("/egress-allowlist", get(get_egress_allowlist).post(post_egress_allowlist))
        .route("/users", get(get_users).post(post_users))
        .route("/users/:id/role", axum::routing::post(post_update_user_role))
        .route("/users/:id/delete", axum::routing::post(post_delete_user))
        .route("/audit-log", get(get_recent_audit_log))
        .route("/audit-log/export.csv", get(get_recent_audit_log_csv))
        .route("/audit-log/:service/:entity_id", get(get_entity_audit_log))
        .route("/security", get(get_security_overview))
        .route("/security/permissions", get(get_permissions_reference))
        .route("/security/sessions", get(get_sessions))
        .route("/security/sessions/:id/revoke", axum::routing::post(post_revoke_session))
        .route("/reports", get(get_reports))
        .route("/data", get(get_data))
        .route("/data/reprocess", axum::routing::post(post_reprocess))
        .route("/data/saved-searches", axum::routing::post(post_save_search))
        .route("/data/saved-searches/:id/delete", axum::routing::post(post_delete_saved_search))
        .route("/data/:id", get(get_data_detail))
        .route("/data/:id/journey", get(get_record_journey))
        .route("/static/charts.js", get(get_charts_js))
        .with_state(state)
}
