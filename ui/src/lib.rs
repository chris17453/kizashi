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
mod backup_status_client;
mod branding_client;
mod branding_middleware;
mod connector_field_catalog;
mod cookie_security;
mod egress_allowlist_client;
mod events_client;
mod execution_client;
mod health_client;
mod incidents_client;
mod ingestion_stats_client;
mod login_attempts_client;
mod mfa_client;
mod normalization_mappings_client;
mod oidc_client;
mod ontology_client;
mod pending_oidc_flow;
mod retention_policies_client;
mod saved_search_queries_client;
mod sensors_client;
mod session;
mod session_context_handler;
mod session_guard;
mod topology;
mod triggers_client;
mod users_client;
mod work_handler;

mod actions_handler;
mod actions_library_handler;
mod analysis_config_handler;
mod api_keys_handler;
mod attention_summary_handler;
mod audit_log_handler;
mod backup_status_handler;
mod branding_handler;
mod compliance_report_handler;
mod configuration_handler;
mod data_compare_handler;
mod data_detail_handler;
mod data_handler;
mod egress_allowlist_handler;
mod event_detail_handler;
mod event_types_handler;
mod events_handler;
mod health_handler;
mod healthz;
mod incident_handlers;
mod login_attempts_handler;
mod login_handler;
mod logout_handler;
mod mfa_login_handler;
mod mfa_settings_handler;
mod normalization_mapping_delete_handler;
mod normalization_mappings_handler;
mod ontology_handler;
mod overview_handler;
mod password_change_handler;
mod permissions_reference_handler;
mod pipeline_handler;
mod recent_audit_log_handler;
mod record_journey_handler;
mod report_schedules_handler;
mod reports_handler;
mod retention_policies_handler;
mod root_handler;
mod search_handler;
mod security_overview_handler;
mod sensor_detail_handler;
mod sensor_script_handler;
mod sensors_handler;
mod sessions_handler;
mod sso_login_handler;
mod static_assets;
mod trigger_delete_handler;
mod trigger_detail_handler;
mod trigger_toggle_handler;
mod triggers_handler;
mod users_handler;
mod workspace_handler;

pub use analysis_config_client::{
    AnalysisConfigClient, AnalysisConfigClientError, AnalysisConfigView, HttpAnalysisConfigClient,
};
pub use api_keys_client::{ApiKeySummary, ApiKeysClient, ApiKeysClientError, HttpApiKeysClient};
pub use audit_log_client::{
    AuditLogClient, AuditLogClientError, AuditLogEntry, HttpAuditLogClient,
    IngestionGatewayApiKeyAuditLogClient,
};
pub use auth_client::{AuthClient, AuthClientError, HttpAuthClient, LocalLoginResult};
pub use backlog_client::{BacklogClient, BacklogClientError, HttpBacklogClient, QueueDepthSummary};
pub use backup_status_client::{
    BackupRun, BackupStatusClient, BackupStatusClientError, BackupTriggerResult,
    HttpBackupStatusClient,
};
pub use branding_client::{Branding, BrandingClient, BrandingClientError, HttpBrandingClient};
pub use branding_middleware::apply_branding;
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
pub use incidents_client::{
    HttpIncidentsClient, IncidentDetail, IncidentsClient, IncidentsClientError,
};
pub use ingestion_stats_client::{
    ConnectorStatSummary, HttpIngestionStatsClient, IngestionStatsClient,
    IngestionStatsClientError, RecordSearchFilter, RecordSummary,
};
pub use login_attempts_client::{
    HttpLoginAttemptsClient, LoginAttempt, LoginAttemptsClient, LoginAttemptsClientError,
};
pub use mfa_client::{HttpMfaClient, MfaClient, MfaClientError, MfaEnrollment};
pub use normalization_mappings_client::{
    HttpNormalizationMappingsClient, NormalizationMappingsClient, NormalizationMappingsClientError,
};
pub use oidc_client::{HttpOidcClient, OidcClient, OidcClientError};
pub use ontology_client::{
    initialize as initialize_ontology_client, CreateActionTypeRequest, CreateLinkRequest,
    CreateLinkTypeRequest, CreateObjectRequest, CreateObjectTypeRequest, HttpOntologyClient,
    InvokeActionRequest, OntologyClient, OntologyClientError,
};
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

pub use actions_handler::{
    get_action_detail, get_actions, get_actions_export_csv, post_action_review,
    post_bulk_action_review, post_bulk_retry_actions, post_delete_action_view,
    post_replay_dead_letter, post_save_action_view,
};
pub use actions_library_handler::{
    get_action_library, post_create_action_library, post_delete_action_library,
    post_update_action_library,
};
pub use analysis_config_handler::{get_analysis_config_page, post_analysis_config};
pub use api_keys_handler::{
    get_api_keys, post_api_keys, post_bulk_revoke_api_keys, post_revoke_api_key,
};
pub use attention_summary_handler::get_attention_summary;
pub use audit_log_handler::get_audit_log as get_entity_audit_log;
pub use backup_status_handler::{get_backups, post_trigger_backup};
pub use branding_handler::{get_branding_page, post_branding};
pub use compliance_report_handler::get_compliance_report;
pub use configuration_handler::get_configuration;
pub use data_compare_handler::get_data_compare;
pub use data_detail_handler::{get_data_detail, post_model_record, post_reprocess_record};
pub use data_handler::{
    get_data, get_data_export_csv, post_delete_saved_search, post_model_selected, post_reprocess,
    post_reprocess_selected, post_save_search,
};
pub use egress_allowlist_handler::{get_egress_allowlist, post_egress_allowlist};
pub use event_detail_handler::{get_event_detail, post_event_status};
pub use events_handler::{
    get_events, get_events_export_csv, post_bulk_event_status, post_delete_event_view,
    post_save_event_view,
};
pub use health_handler::get_health;
pub use healthz::healthz;
pub use incident_handlers::{
    get_incident_detail, get_incident_export_csv, get_incidents, get_incidents_export_csv,
    post_add_incident_note, post_bulk_update_incidents, post_claim_incident,
    post_create_incident_from_event, post_create_incident_from_events, post_delete_incident_view,
    post_incident, post_incident_status_transition, post_link_event_to_incident,
    post_link_events_to_incident, post_save_incident_view, post_unlink_event, post_update_incident,
};
pub use login_attempts_handler::get_login_attempts as get_login_attempts_page;
pub use login_attempts_handler::get_login_attempts_export_csv;
pub use login_handler::{get_login, post_login};
pub use logout_handler::get_logout;
pub use mfa_login_handler::{get_mfa_challenge, post_mfa_challenge as post_mfa_login_challenge};
pub use mfa_settings_handler::{
    get_mfa_settings, post_mfa_disable as post_mfa_settings_disable,
    post_mfa_enroll as post_mfa_settings_enroll, post_mfa_verify as post_mfa_settings_verify,
};
pub use normalization_mapping_delete_handler::post_delete_normalization_mapping;
pub use normalization_mappings_handler::{
    get_normalization_mappings, post_edit_normalization_mapping, post_normalization_mapping,
};
pub use ontology_handler::invoke_ontology_action;
pub use ontology_handler::{
    create_bulk_ontology_link_instances, create_ontology_link_instance,
    delete_ontology_link_instance, update_ontology_link_instance,
};
pub use ontology_handler::{
    create_ontology_action, create_ontology_link, create_ontology_object, create_ontology_type,
    delete_ontology_action, delete_ontology_link, delete_ontology_object, delete_ontology_type,
    list_ontology, update_ontology_action, update_ontology_link, update_ontology_object,
    update_ontology_type,
};
pub use ontology_handler::{
    get_ontology_compare, get_ontology_export_csv, post_delete_ontology_view,
    post_save_ontology_view,
};
pub use overview_handler::{get_overview, post_dashboard_layout, post_reset_dashboard_layout};
pub use password_change_handler::{get_password_settings, post_password_settings};
pub use permissions_reference_handler::get_permissions_reference;
pub use pipeline_handler::get_pipeline;
pub use recent_audit_log_handler::{get_recent_audit_log, get_recent_audit_log_csv};
pub use record_journey_handler::get_record_journey;
pub use report_schedules_handler::{
    get_report_schedules, post_create_report_schedule, post_delete_report_schedule,
    post_run_report_schedule, post_toggle_report_schedule,
};
pub use reports_handler::{
    get_reports, get_reports_export_csv, get_reports_export_pdf, post_delete_report_view,
    post_save_report_view,
};
pub use retention_policies_handler::{
    get_retention_policies, post_bulk_delete_retention_policies, post_create_hold,
    post_delete_retention_policy, post_edit_retention_policy, post_reimport_archive,
    post_release_hold, post_retention_policies, post_toggle_retention_policy,
};
pub use root_handler::get_root;
pub use search_handler::{
    get_search, post_delete_global_search_view, post_save_global_search_view,
};
pub use security_overview_handler::get_security_overview;
pub use sensor_detail_handler::{get_sensor_detail, post_update_sensor};
pub use sensor_script_handler::{get_generate_form, get_generate_select, post_generate_script};
pub use sensors_handler::{
    get_sensors, post_bulk_delete_sensors, post_delete_sensor, post_sensors, post_toggle_sensor,
};
pub use session_context_handler::get_session_context;
pub use sessions_handler::{get_sessions, post_bulk_revoke_sessions, post_revoke_session};
pub use sso_login_handler::{get_sso_callback, get_sso_login};
pub use static_assets::{get_charts_js, get_command_palette_js, get_confirm_danger_js};
pub use trigger_delete_handler::post_delete_trigger;
pub use trigger_detail_handler::{get_trigger_detail, post_trigger_edit};
pub use trigger_toggle_handler::{post_bulk_toggle_triggers, post_toggle_trigger};
pub use triggers_handler::{get_triggers, post_trigger};
pub use users_handler::{
    get_export_user, get_user_detail, get_users, post_bulk_delete_users,
    post_bulk_update_user_role, post_delete_user, post_update_user_role, post_users,
};
pub use work_handler::{
    get_work, get_work_export_csv, post_bulk_claim_work, post_delete_work_view, post_save_work_view,
};
pub use workspace_handler::get_switch_workspace;

use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

pub const SESSION_COOKIE_NAME: &str = "kizashi_session";
pub const WORKSPACE_COOKIE_NAME: &str = "kizashi_workspace";

#[derive(Clone)]
pub struct AppState {
    pub session_store: Arc<dyn SessionStore>,
    pub auth_client: Arc<dyn AuthClient>,
    pub mfa_client: Arc<dyn MfaClient>,
    pub branding_client: Arc<dyn BrandingClient>,
    pub oidc_client: Arc<dyn OidcClient>,
    pub pending_oidc_flow_store: Arc<dyn PendingOidcFlowStore>,
    pub events_client: Arc<dyn EventsClient>,
    pub triggers_client: Arc<dyn TriggersClient>,
    pub incidents_client: Arc<dyn IncidentsClient>,
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
    pub backup_status_client: Arc<dyn BackupStatusClient>,
    pub users_client: Arc<dyn UsersClient>,
    pub login_attempts_client: Arc<dyn LoginAttemptsClient>,
    pub saved_search_queries_client: Arc<dyn SavedSearchQueriesClient>,
    /// All three fields hold an `Arc<dyn AuditLogClient>` built from the *same*
    /// `HttpAuditLogClient` implementation, just constructed with a different backend base
    /// URL — `config-admin-service`, `retention-service`, and `auth-service` all expose an
    /// identically shaped `GET /v1/audit-log/:entity_id`, so one client type covers all three.
    pub config_audit_log_client: Arc<dyn AuditLogClient>,
    pub retention_audit_log_client: Arc<dyn AuditLogClient>,
    pub auth_audit_log_client: Arc<dyn AuditLogClient>,
    /// `ingestion-gateway`'s per-API-key audit trail -- a distinct URL shape from the other
    /// three (see `IngestionGatewayApiKeyAuditLogClient`'s doc comment), so it isn't just
    /// another `HttpAuditLogClient` pointed at a different base URL.
    pub ingestion_audit_log_client: Arc<dyn AuditLogClient>,
    /// `egress-gateway`'s per-tenant allowlist audit trail. Matches the shared
    /// `GET /v1/audit-log/:entity_id` shape (ADR-0097), so it reuses `HttpAuditLogClient` like
    /// the config/retention/auth trio above, just pointed at `egress-gateway`.
    pub egress_audit_log_client: Arc<dyn AuditLogClient>,
    /// The ingestion-gateway URL a *deployed connector* should point at — not necessarily
    /// reachable from inside this container (e.g. a customer-hosted connector polling in from
    /// outside the platform's own network), so it's a separate, operator-configurable value
    /// from `QUERY_GATEWAY_URL`/etc., which are all internal-network addresses.
    pub ingestion_gateway_public_url: String,
}

pub fn build_router(state: AppState) -> Router {
    let branding_state = state.clone();
    Router::new()
        .route("/", get(get_root))
        .route("/healthz", get(healthz))
        .route("/login", get(get_login).post(post_login))
        .route("/login/mfa", get(get_mfa_challenge).post(post_mfa_login_challenge))
        .route("/login/sso", get(get_sso_login))
        .route("/login/sso/callback", get(get_sso_callback))
        .route("/logout", get(get_logout))
        .route("/workspace/switch", get(get_switch_workspace))
        .route("/session/context", get(get_session_context))
        .route("/work/summary", get(get_attention_summary))
        .route("/events", get(get_events))
        .route("/event-types", get(event_types_handler::get_event_types))
        .route("/event-types", axum::routing::post(event_types_handler::post_create_event_type))
        .route(
            "/event-types/:id/versions",
            axum::routing::post(event_types_handler::post_event_type_version),
        )
        .route("/events/saved-views", post(post_save_event_view))
        .route("/events/saved-views/:id/delete", post(post_delete_event_view))
        .route("/actions", get(get_actions))
        .route("/actions/:id", get(get_action_detail))
        .route("/actions/:id/review", axum::routing::post(post_action_review))
        .route("/actions/library", get(get_action_library).post(post_create_action_library))
        .route("/actions/library/:id/edit", post(post_update_action_library))
        .route("/actions/library/:id/delete", post(post_delete_action_library))
        .route("/actions/export.csv", get(get_actions_export_csv))
        .route("/actions/saved-views", post(post_save_action_view))
        .route("/actions/saved-views/:id/delete", post(post_delete_action_view))
        .route("/actions/dead-letter/replay", post(post_replay_dead_letter))
        .route("/actions/bulk-retry", post(post_bulk_retry_actions))
        .route("/actions/bulk-review", post(post_bulk_action_review))
        .route("/events/:id", get(get_event_detail))
        .route("/events/:id/status", axum::routing::post(post_event_status))
        .route("/events/:id/create-incident", axum::routing::post(post_create_incident_from_event))
        .route("/events/:id/link-incident", axum::routing::post(post_link_event_to_incident))
        .route("/events/export.csv", get(get_events_export_csv))
        .route("/events/create-incident", axum::routing::post(post_create_incident_from_events))
        .route("/events/link-incident", axum::routing::post(post_link_events_to_incident))
        .route("/events/bulk-status", axum::routing::post(post_bulk_event_status))
        .route("/incidents", get(get_incidents).post(post_incident))
        .route("/incidents/export.csv", get(get_incidents_export_csv))
        .route("/incidents/saved-views", post(post_save_incident_view))
        .route("/incidents/saved-views/:id/delete", post(post_delete_incident_view))
        .route("/incidents/bulk-update", axum::routing::post(post_bulk_update_incidents))
        .route("/incidents/:id/export.csv", get(get_incident_export_csv))
        .route("/incidents/:id", get(get_incident_detail).post(post_update_incident))
        .route("/incidents/:id/claim", axum::routing::post(post_claim_incident))
        .route("/incidents/:id/status", axum::routing::post(post_incident_status_transition))
        .route("/incidents/:id/notes", axum::routing::post(post_add_incident_note))
        .route("/incidents/:id/events/:event_id/unlink", axum::routing::post(post_unlink_event))
        .route("/triggers", get(get_triggers).post(post_trigger))
        .route("/triggers/:id", get(get_trigger_detail))
        .route("/triggers/:id/edit", axum::routing::post(post_trigger_edit))
        .route("/triggers/bulk-toggle", post(post_bulk_toggle_triggers))
        .route("/triggers/:id/toggle", post(post_toggle_trigger))
        .route("/triggers/:id/delete", post(post_delete_trigger))
        .route("/health", get(get_health))
        .route("/configuration", get(get_configuration))
        .route("/branding", get(get_branding_page).post(post_branding))
        .route("/pipeline", get(get_pipeline))
        .route("/ontology", get(list_ontology))
        .route("/ontology/compare", get(get_ontology_compare))
        .route("/ontology/export.csv", get(get_ontology_export_csv))
        .route("/ontology/saved-views", axum::routing::post(post_save_ontology_view))
        .route("/ontology/saved-views/:id/delete", axum::routing::post(post_delete_ontology_view))
        .route("/ontology/objects", axum::routing::post(create_ontology_object))
        .route("/ontology/objects/:id/edit", axum::routing::post(update_ontology_object))
        .route("/ontology/objects/:id/delete", axum::routing::post(delete_ontology_object))
        .route("/ontology/types", axum::routing::post(create_ontology_type))
        .route("/ontology/types/:id/edit", axum::routing::post(update_ontology_type))
        .route("/ontology/types/:id/delete", axum::routing::post(delete_ontology_type))
        .route("/ontology/links", axum::routing::post(create_ontology_link))
        .route("/ontology/links/instances", axum::routing::post(create_ontology_link_instance))
        .route(
            "/ontology/links/instances/bulk",
            axum::routing::post(create_bulk_ontology_link_instances),
        )
        .route(
            "/ontology/links/instances/:id/edit",
            axum::routing::post(update_ontology_link_instance),
        )
        .route(
            "/ontology/links/instances/:id/delete",
            axum::routing::post(delete_ontology_link_instance),
        )
        .route("/ontology/links/:id/edit", axum::routing::post(update_ontology_link))
        .route("/ontology/links/:id/delete", axum::routing::post(delete_ontology_link))
        .route("/ontology/actions", axum::routing::post(create_ontology_action))
        .route("/ontology/actions/invoke", axum::routing::post(invoke_ontology_action))
        .route("/ontology/actions/:id/edit", axum::routing::post(update_ontology_action))
        .route("/ontology/actions/:id/delete", axum::routing::post(delete_ontology_action))
        .route("/overview", get(get_overview))
        .route("/overview/layout", axum::routing::post(post_dashboard_layout))
        .route("/overview/layout/reset", axum::routing::post(post_reset_dashboard_layout))
        .route("/work", get(get_work))
        .route("/work/export.csv", get(get_work_export_csv))
        .route("/work/saved-views", post(post_save_work_view))
        .route("/work/saved-views/:id/delete", post(post_delete_work_view))
        .route("/work/bulk-claim", axum::routing::post(post_bulk_claim_work))
        .route("/sensors", get(get_sensors).post(post_sensors))
        .route("/sensors/generate", get(get_generate_select))
        .route("/sensors/generate/form", get(get_generate_form))
        .route("/sensors/generate/script", axum::routing::post(post_generate_script))
        .route("/sensors/:id", get(get_sensor_detail))
        .route("/sensors/:id/edit", axum::routing::post(post_update_sensor))
        .route("/sensors/:id/delete", axum::routing::post(post_delete_sensor))
        .route("/sensors/:id/toggle", axum::routing::post(post_toggle_sensor))
        .route("/sensors/bulk-delete", axum::routing::post(post_bulk_delete_sensors))
        .route("/api-keys", get(get_api_keys).post(post_api_keys))
        .route("/api-keys/:id/revoke", axum::routing::post(post_revoke_api_key))
        .route("/api-keys/bulk-revoke", axum::routing::post(post_bulk_revoke_api_keys))
        .route("/analysis-config", get(get_analysis_config_page).post(post_analysis_config))
        .route(
            "/normalization-mappings",
            get(get_normalization_mappings).post(post_normalization_mapping),
        )
        .route(
            "/normalization-mappings/:id/edit",
            axum::routing::post(post_edit_normalization_mapping),
        )
        .route(
            "/normalization-mappings/:id/delete",
            axum::routing::post(post_delete_normalization_mapping),
        )
        .route("/retention-policies", get(get_retention_policies).post(post_retention_policies))
        .route("/retention-policies/reimport", axum::routing::post(post_reimport_archive))
        .route("/retention-policies/holds", axum::routing::post(post_create_hold))
        .route("/retention-policies/holds/:id/release", axum::routing::post(post_release_hold))
        .route("/retention-policies/:id/toggle", axum::routing::post(post_toggle_retention_policy))
        .route("/retention-policies/:id/edit", axum::routing::post(post_edit_retention_policy))
        .route("/retention-policies/:id/delete", axum::routing::post(post_delete_retention_policy))
        .route(
            "/retention-policies/bulk-delete",
            axum::routing::post(post_bulk_delete_retention_policies),
        )
        .route("/egress-allowlist", get(get_egress_allowlist).post(post_egress_allowlist))
        .route("/users", get(get_users).post(post_users))
        .route("/users/:id", get(get_user_detail))
        .route("/users/:id/role", axum::routing::post(post_update_user_role))
        .route("/users/:id/delete", axum::routing::post(post_delete_user))
        .route("/users/bulk-delete", axum::routing::post(post_bulk_delete_users))
        .route("/users/bulk-role", axum::routing::post(post_bulk_update_user_role))
        .route("/users/:id/export", get(get_export_user))
        .route("/audit-log", get(get_recent_audit_log))
        .route("/audit-log/export.csv", get(get_recent_audit_log_csv))
        .route("/audit-log/:service/:entity_id", get(get_entity_audit_log))
        .route("/security", get(get_security_overview))
        .route("/security/permissions", get(get_permissions_reference))
        .route("/security/mfa", get(get_mfa_settings))
        .route("/security/mfa/enroll", axum::routing::post(post_mfa_settings_enroll))
        .route("/security/mfa/verify", axum::routing::post(post_mfa_settings_verify))
        .route("/security/mfa/disable", axum::routing::post(post_mfa_settings_disable))
        .route("/security/password", get(get_password_settings).post(post_password_settings))
        .route("/security/sessions", get(get_sessions))
        .route("/security/login-attempts", get(get_login_attempts_page))
        .route("/security/login-attempts/export.csv", get(get_login_attempts_export_csv))
        .route("/security/backups", get(get_backups))
        .route("/security/backups/run", axum::routing::post(post_trigger_backup))
        .route("/security/compliance-report", get(get_compliance_report))
        .route("/security/sessions/:id/revoke", axum::routing::post(post_revoke_session))
        .route("/security/sessions/bulk-revoke", axum::routing::post(post_bulk_revoke_sessions))
        .route("/reports", get(get_reports))
        .route("/reports/schedules", get(get_report_schedules).post(post_create_report_schedule))
        .route("/reports/schedules/:id/toggle", axum::routing::post(post_toggle_report_schedule))
        .route("/reports/schedules/:id/run", axum::routing::post(post_run_report_schedule))
        .route("/reports/schedules/:id/delete", axum::routing::post(post_delete_report_schedule))
        .route("/reports/saved-views", post(post_save_report_view))
        .route("/reports/saved-views/:id/delete", post(post_delete_report_view))
        .route("/reports/export.csv", get(get_reports_export_csv))
        .route("/reports/export.pdf", get(get_reports_export_pdf))
        .route("/search", get(get_search))
        .route("/search/saved-views", post(post_save_global_search_view))
        .route("/search/saved-views/:id/delete", post(post_delete_global_search_view))
        .route("/data", get(get_data))
        .route("/data/compare", get(get_data_compare))
        .route("/data/export.csv", get(get_data_export_csv))
        .route("/data/reprocess", axum::routing::post(post_reprocess))
        .route("/data/reprocess-selected", axum::routing::post(post_reprocess_selected))
        .route("/data/model-selected", axum::routing::post(post_model_selected))
        .route("/data/saved-searches", axum::routing::post(post_save_search))
        .route("/data/saved-searches/:id/delete", axum::routing::post(post_delete_saved_search))
        .route("/data/:id/reprocess", axum::routing::post(post_reprocess_record))
        .route("/data/:id/model", axum::routing::post(post_model_record))
        .route("/data/:id", get(get_data_detail))
        .route("/data/:id/journey", get(get_record_journey))
        .route("/static/charts.js", get(get_charts_js))
        .route("/static/confirm-danger.js", get(get_confirm_danger_js))
        .route("/static/command-palette.js", get(get_command_palette_js))
        .with_state(state)
        // Applies tenant branding (ADR-0059) to every already-rendered authenticated page by
        // rewriting the response body, not per-handler template fields -- see
        // `branding_middleware.rs`'s doc comment for why. Layered last so it wraps every route
        // above uniformly, including ones added in the future.
        .layer(axum::middleware::from_fn_with_state(branding_state, apply_branding))
}
