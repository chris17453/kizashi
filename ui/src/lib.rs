//! Console UI (spec §7): a server-rendered Rust web app (ADR-0014) — axum + askama, no WASM
//! build step, tested the same way as every other service in this repo
//! (`tower::ServiceExt::oneshot` against an in-process router). Client-side JS is layered on
//! top for charts/components (ADR-0015, reversing ADR-0014's no-JS constraint) — every page
//! still server-renders its real data first, JS only progressively enhances it.

#![allow(
    clippy::cmp_owned,
    clippy::into_iter_on_ref,
    clippy::manual_checked_ops,
    clippy::manual_div_ceil,
    clippy::manual_pattern_char_comparison,
    clippy::result_large_err,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::unnecessary_map_or,
    clippy::unnecessary_sort_by,
    clippy::useless_vec
)]

mod action_templates_client;
mod analysis_config_client;
mod api_keys_client;
mod api_v1_handler;
mod apps_handler;
mod audit_log_client;
mod auth_client;
mod backlog_client;
mod backup_status_client;
mod branding_client;
mod branding_middleware;
mod build_studio_client;
mod build_studio_handler;
mod connector_field_catalog;
mod cookie_security;
mod egress_allowlist_client;
mod events_client;
mod execution_client;
mod health_client;
mod incident_brief_client;
mod incidents_client;
mod ingestion_stats_client;
mod login_attempts_client;
mod mfa_client;
mod normalization_mappings_client;
mod normalization_telemetry_client;
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
mod workflow_cases_handler;
mod workflow_client;

#[cfg(test)]
mod layout_design_system_test;

mod action_templates_handler;
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
mod incident_correlation_handler;
mod incident_handlers;
mod login_attempts_handler;
mod login_handler;
mod logout_handler;
mod mfa_login_handler;
mod mfa_settings_handler;
mod normalization_mapping_delete_handler;
mod normalization_mappings_handler;
mod ontology_handler;
mod ontology_object_360_handler;
mod overview_handler;
mod password_change_handler;
mod permissions_reference_handler;
mod pipeline_definitions_handler;
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
pub(crate) mod sensor_script_handler;
mod sensors_handler;
mod service_accounts_handler;
mod sessions_handler;
mod sso_login_handler;
mod static_assets;
mod trigger_delete_handler;
mod trigger_detail_handler;
mod trigger_toggle_handler;
mod triggers_handler;
mod users_handler;
mod workspace_handler;

pub use action_templates_client::{
    initialize as initialize_action_templates_client, ActionTemplatesClient,
    ActionTemplatesClientError, HttpActionTemplatesClient,
};
pub use analysis_config_client::{
    AnalysisConfigClient, AnalysisConfigClientError, AnalysisConfigView, HttpAnalysisConfigClient,
};
pub use api_keys_client::{ApiKeySummary, ApiKeysClient, ApiKeysClientError, HttpApiKeysClient};
pub use audit_log_client::{
    AuditLogClient, AuditLogClientError, AuditLogEntry, HttpAuditLogClient,
    IngestionGatewayApiKeyAuditLogClient,
};
pub use auth_client::{
    AuthClient, AuthClientError, HttpAuthClient, LocalLoginResult, ServiceAccountPrincipal,
    ServiceAccountSummary, TenantOidcProviderSummary,
};
pub use backlog_client::{BacklogClient, BacklogClientError, HttpBacklogClient, QueueDepthSummary};
pub use backup_status_client::{
    BackupRun, BackupStatusClient, BackupStatusClientError, BackupTriggerResult,
    HttpBackupStatusClient,
};
pub use branding_client::{Branding, BrandingClient, BrandingClientError, HttpBrandingClient};
pub use branding_middleware::apply_branding;
pub use build_studio_client::{
    global as build_studio_client, initialize as initialize_build_studio_client, BuildStudioClient,
    HttpBuildStudioClient,
};
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
    ServiceMetricsSummary,
};
pub use incident_brief_client::{
    initialize as initialize_incident_brief_client, HttpIncidentBriefClient, IncidentBriefClient,
    IncidentBriefClientError,
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
pub use normalization_telemetry_client::{
    initialize as initialize_normalization_telemetry_client, DedupSummary,
    HttpNormalizationTelemetryClient, NormalizationTelemetryClient,
    NormalizationTelemetryClientError,
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
pub use workflow_client::{
    initialize as initialize_workflow_client, HttpWorkflowClient, WorkflowClient,
};

pub use action_templates_handler::{
    get_action_templates, post_action_template, post_delete_action_template,
    post_update_action_template,
};
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
    post_generate_incident_brief, post_incident, post_incident_status_transition,
    post_link_event_to_incident, post_link_events_to_incident, post_save_incident_view,
    post_unlink_event, post_update_incident,
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
    bulk_update_ontology_objects, create_ontology_action, create_ontology_link,
    create_ontology_object, create_ontology_object_annotation, create_ontology_type,
    delete_ontology_action, delete_ontology_link, delete_ontology_object, delete_ontology_type,
    list_ontology, update_ontology_action, update_ontology_link, update_ontology_object,
    update_ontology_type,
};
pub use ontology_handler::{
    create_bulk_ontology_link_instances, create_ontology_link_instance,
    delete_ontology_link_instance, update_ontology_link_instance,
};
pub use ontology_handler::{
    get_ontology_compare, get_ontology_export_csv, post_delete_ontology_view,
    post_save_ontology_view,
};
pub use ontology_object_360_handler::get_ontology_object_360;
pub use overview_handler::{
    get_overview, post_dashboard_layout, post_reset_dashboard_layout, post_update_saved_view_name,
};
pub use password_change_handler::{get_password_settings, post_password_settings};
pub use permissions_reference_handler::get_permissions_reference;
pub use pipeline_handler::get_pipeline;
pub use recent_audit_log_handler::{get_recent_audit_log, get_recent_audit_log_csv};
pub use record_journey_handler::get_record_journey;
pub use report_schedules_handler::{
    get_report_schedules, post_create_report_schedule, post_delete_report_schedule,
    post_run_report_schedule, post_toggle_report_schedule, post_update_report_schedule,
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
pub use security_overview_handler::{
    get_security_overview, post_delete_tenant_oidc_config, post_mfa_policy,
    post_oidc_provider_policy, post_tenant_oidc_config,
};
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

use axum::routing::{get, post, put};
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
        .route("/action-templates", get(get_action_templates).post(post_action_template))
        .route("/action-templates/:id", post(post_delete_action_template))
        .route("/action-templates/:id/edit", post(post_update_action_template))
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
        .route(
            "/incidents/correlation-sweep",
            get(incident_correlation_handler::get_correlation_sweep)
                .post(incident_correlation_handler::post_correlation_sweep),
        )
        .route(
            "/incidents/correlation-sweep/auto",
            axum::routing::post(incident_correlation_handler::post_correlation_sweep_auto),
        )
        .route(
            "/incidents/correlation-sweep/resolve",
            axum::routing::post(incident_correlation_handler::post_correlation_sweep_resolve),
        )
        .route(
            "/api/v1/incidents/correlation-sweep",
            get(incident_correlation_handler::get_correlation_sweep_api)
                .post(incident_correlation_handler::post_correlation_sweep_api),
        )
        .route(
            "/api/v1/incidents/correlation-sweep/auto",
            axum::routing::post(incident_correlation_handler::post_correlation_sweep_auto_api),
        )
        .route("/api/v1/events", get(api_v1_handler::list_events))
        .route("/api/v1/events/export.csv", get(api_v1_handler::export_events_csv_api))
        .route("/api/v1/health", get(api_v1_handler::get_health_api))
        .route("/api/v1/overview", get(api_v1_handler::get_overview_api))
        .route("/api/v1/attention", get(api_v1_handler::get_attention_api))
        .route("/api/v1/security/overview", get(api_v1_handler::get_security_overview_api))
        .route("/api/v1/security/permissions", get(api_v1_handler::get_permissions_reference_api))
        .route("/api/v1/configuration", get(api_v1_handler::get_configuration_api))
        .route("/api/v1/session/context", get(api_v1_handler::get_session_context_api))
        .route("/api/v1/pipeline", get(api_v1_handler::get_pipeline_api))
        .route("/api/v1/audit-log", get(api_v1_handler::list_audit_log_api))
        .route(
            "/api/v1/audit-log/:service/:entity_id",
            get(api_v1_handler::get_entity_audit_log_api),
        )
        .route("/api/v1/audit-log/export.csv", get(api_v1_handler::export_audit_log_csv_api))
        .route(
            "/api/v1/branding",
            get(api_v1_handler::get_branding_api).put(api_v1_handler::put_branding_api),
        )
        .route(
            "/api/v1/analysis-config",
            get(api_v1_handler::get_analysis_config_api)
                .put(api_v1_handler::put_analysis_config_api),
        )
        .route(
            "/api/v1/egress-allowlist",
            get(api_v1_handler::get_egress_allowlist_api)
                .put(api_v1_handler::put_egress_allowlist_api),
        )
        .route("/api/v1/security/sessions", get(api_v1_handler::list_sessions_api))
        .route(
            "/api/v1/security/sessions/:id",
            axum::routing::delete(api_v1_handler::revoke_session_api),
        )
        .route(
            "/api/v1/security/sessions/bulk-revoke",
            post(api_v1_handler::bulk_revoke_sessions_api),
        )
        .route("/api/v1/security/login-attempts", get(api_v1_handler::list_login_attempts_api))
        .route(
            "/api/v1/security/login-attempts/export.csv",
            get(api_v1_handler::export_login_attempts_csv_api),
        )
        .route("/api/v1/security/backups", get(api_v1_handler::list_backups_api))
        .route("/api/v1/security/backups/run", post(api_v1_handler::trigger_backup_api))
        .route("/api/v1/security/compliance-report", get(api_v1_handler::get_compliance_report_api))
        .route("/api/v1/security/mfa", get(api_v1_handler::get_mfa_api))
        .route(
            "/api/v1/security/mfa-policy",
            get(api_v1_handler::get_mfa_policy_api).put(api_v1_handler::update_mfa_policy_api),
        )
        .route(
            "/api/v1/security/oidc-provider",
            get(api_v1_handler::get_oidc_provider_policy_api)
                .put(api_v1_handler::update_oidc_provider_policy_api),
        )
        .route("/api/v1/security/oidc-config", get(api_v1_handler::get_tenant_oidc_configs_api))
        .route(
            "/api/v1/security/oidc-config/:provider",
            axum::routing::put(api_v1_handler::put_tenant_oidc_config_api)
                .delete(api_v1_handler::delete_tenant_oidc_config_api),
        )
        .route("/api/v1/security/mfa/enroll", post(api_v1_handler::enroll_mfa_api))
        .route("/api/v1/security/mfa/verify", post(api_v1_handler::verify_mfa_api))
        .route("/api/v1/security/mfa/disable", post(api_v1_handler::disable_mfa_api))
        .route("/api/v1/security/password", post(api_v1_handler::change_password_api))
        .route("/api/v1/reports/export.csv", get(api_v1_handler::export_report_csv_api))
        .route("/api/v1/reports/export.pdf", get(api_v1_handler::export_report_pdf_api))
        .route("/api/v1/reports/summary", get(api_v1_handler::get_reports_summary_api))
        .route(
            "/api/v1/events/:id",
            get(api_v1_handler::get_event).post(api_v1_handler::update_event_status),
        )
        .route(
            "/api/v1/events/:id/status-history",
            get(api_v1_handler::list_event_status_history_api),
        )
        .route("/api/v1/events/:id/360", get(api_v1_handler::get_event_360))
        .route("/api/v1/events/bulk-status", post(api_v1_handler::bulk_update_event_status))
        .route(
            "/api/v1/incidents",
            get(api_v1_handler::list_incidents).post(api_v1_handler::create_incident),
        )
        .route("/api/v1/incidents/export.csv", get(api_v1_handler::export_incidents_csv_api))
        .route(
            "/api/v1/incidents/:id",
            get(api_v1_handler::get_incident).put(api_v1_handler::update_incident_api),
        )
        .route("/api/v1/incidents/:id/brief", post(api_v1_handler::generate_incident_brief_api))
        .route("/api/v1/incidents/:id/360", get(api_v1_handler::get_incident_360))
        .route("/api/v1/incidents/:id/status", post(api_v1_handler::update_incident_status_api))
        .route("/api/v1/incidents/:id/claim", post(api_v1_handler::claim_incident_api))
        .route("/api/v1/incidents/:id/notes", post(api_v1_handler::add_incident_note_api))
        .route("/api/v1/incidents/bulk", post(api_v1_handler::bulk_update_incidents_api))
        .route(
            "/api/v1/incidents/:incident_id/events/:event_id",
            post(api_v1_handler::link_incident_event).delete(api_v1_handler::unlink_incident_event),
        )
        .route(
            "/api/v1/incidents/:incident_id/events",
            post(api_v1_handler::link_events_to_incident_api),
        )
        .route(
            "/api/v1/triggers",
            get(api_v1_handler::list_triggers).post(api_v1_handler::create_trigger),
        )
        .route(
            "/api/v1/triggers/:id",
            get(api_v1_handler::get_trigger)
                .put(api_v1_handler::update_trigger)
                .delete(api_v1_handler::delete_trigger),
        )
        .route("/api/v1/triggers/:id/test", post(api_v1_handler::test_trigger))
        .route("/api/v1/triggers/bulk-toggle", post(api_v1_handler::bulk_toggle_triggers))
        .route(
            "/api/v1/event-types",
            get(api_v1_handler::list_event_types).post(api_v1_handler::create_event_type),
        )
        .route("/api/v1/event-types/:id", get(api_v1_handler::get_event_type))
        .route("/api/v1/event-types/:id/versions", post(api_v1_handler::create_event_type_version))
        .route(
            "/api/v1/retention-policies",
            get(api_v1_handler::list_retention_policies)
                .post(api_v1_handler::create_retention_policy),
        )
        .route(
            "/api/v1/retention-policies/:id",
            get(api_v1_handler::get_retention_policy)
                .put(api_v1_handler::update_retention_policy)
                .delete(api_v1_handler::delete_retention_policy),
        )
        .route(
            "/api/v1/retention-policies/bulk-delete",
            post(api_v1_handler::bulk_delete_retention_policies_api),
        )
        .route(
            "/api/v1/retention-policies/holds",
            get(api_v1_handler::list_compliance_holds).post(api_v1_handler::create_compliance_hold),
        )
        .route(
            "/api/v1/retention-policies/holds/:id/release",
            post(api_v1_handler::release_compliance_hold),
        )
        .route("/api/v1/retention-policies/holds/:id", get(api_v1_handler::get_compliance_hold))
        .route("/api/v1/retention-policies/reimport", post(api_v1_handler::reimport_archive))
        .route(
            "/api/v1/report-schedules",
            get(api_v1_handler::list_report_schedules).post(api_v1_handler::create_report_schedule),
        )
        .route(
            "/api/v1/report-schedules/:id",
            put(api_v1_handler::update_report_schedule)
                .delete(api_v1_handler::delete_report_schedule),
        )
        .route("/api/v1/report-schedules/:id/toggle", post(api_v1_handler::toggle_report_schedule))
        .route("/api/v1/report-schedules/:id/run", post(api_v1_handler::run_report_schedule))
        .route(
            "/api/v1/sensors",
            get(api_v1_handler::list_sensors).post(api_v1_handler::register_sensor),
        )
        .route("/api/v1/sensors/catalog", get(api_v1_handler::list_sensor_catalog_api))
        .route("/api/v1/sensors/generate", post(api_v1_handler::generate_sensor_script_api))
        .route(
            "/api/v1/sensors/:id",
            get(api_v1_handler::get_sensor)
                .put(api_v1_handler::update_sensor)
                .delete(api_v1_handler::delete_sensor),
        )
        .route("/api/v1/sensors/bulk-delete", post(api_v1_handler::bulk_delete_sensors_api))
        .route(
            "/api/v1/normalization-mappings",
            get(api_v1_handler::list_normalization_mappings)
                .post(api_v1_handler::create_normalization_mapping),
        )
        .route(
            "/api/v1/normalization-mappings/:id",
            put(api_v1_handler::update_normalization_mapping)
                .delete(api_v1_handler::delete_normalization_mapping),
        )
        .route("/api/v1/ontology/objects", get(api_v1_handler::list_ontology_objects))
        .route("/api/v1/ontology/graph", get(api_v1_handler::get_ontology_graph_api))
        .route("/api/v1/ontology/compare", get(api_v1_handler::get_ontology_compare_api))
        .route("/api/v1/ontology/export.csv", get(api_v1_handler::export_ontology_csv_api))
        .route(
            "/api/v1/ontology/object-types",
            get(api_v1_handler::list_ontology_object_types)
                .post(api_v1_handler::create_ontology_object_type),
        )
        .route(
            "/api/v1/ontology/object-types/history",
            get(api_v1_handler::list_ontology_object_type_history_all),
        )
        .route(
            "/api/v1/ontology/object-types/:id/history",
            get(api_v1_handler::list_ontology_object_type_history),
        )
        .route(
            "/api/v1/ontology/object-types/:id",
            get(api_v1_handler::get_ontology_object_type)
                .put(api_v1_handler::update_ontology_object_type)
                .delete(api_v1_handler::delete_ontology_object_type),
        )
        .route(
            "/api/v1/ontology/link-types",
            get(api_v1_handler::list_ontology_link_types)
                .post(api_v1_handler::create_ontology_link_type),
        )
        .route(
            "/api/v1/ontology/link-types/:id",
            get(api_v1_handler::get_ontology_link_type)
                .put(api_v1_handler::update_ontology_link_type)
                .delete(api_v1_handler::delete_ontology_link_type),
        )
        .route(
            "/api/v1/ontology/link-types/history",
            get(api_v1_handler::list_ontology_link_type_history_all),
        )
        .route(
            "/api/v1/ontology/link-types/:id/history",
            get(api_v1_handler::list_ontology_link_type_history),
        )
        .route(
            "/api/v1/ontology/links",
            get(api_v1_handler::list_ontology_links).post(api_v1_handler::create_ontology_link),
        )
        .route(
            "/api/v1/ontology/links/instances/bulk",
            post(api_v1_handler::create_bulk_ontology_link_instances_api),
        )
        .route(
            "/api/v1/ontology/links/:id",
            get(api_v1_handler::get_ontology_link)
                .put(api_v1_handler::update_ontology_link)
                .delete(api_v1_handler::delete_ontology_link),
        )
        .route(
            "/api/v1/ontology/links/history",
            get(api_v1_handler::list_ontology_link_history_all),
        )
        .route(
            "/api/v1/ontology/links/:id/history",
            get(api_v1_handler::list_ontology_link_history),
        )
        .route("/api/v1/ontology/objects", post(api_v1_handler::create_ontology_object))
        .route(
            "/api/v1/ontology/objects/bulk-update",
            post(api_v1_handler::bulk_update_ontology_objects),
        )
        .route(
            "/api/v1/ontology/objects/:id",
            get(api_v1_handler::get_ontology_object)
                .put(api_v1_handler::update_ontology_object)
                .delete(api_v1_handler::delete_ontology_object),
        )
        .route("/api/v1/ontology/objects/:id/360", get(api_v1_handler::get_ontology_object_360_api))
        .route(
            "/api/v1/ontology/objects/history",
            get(api_v1_handler::list_ontology_object_history_all),
        )
        .route(
            "/api/v1/ontology/objects/:id/history",
            get(api_v1_handler::list_ontology_object_history),
        )
        .route(
            "/api/v1/ontology/objects/:id/annotations",
            get(api_v1_handler::list_ontology_object_annotations)
                .post(api_v1_handler::create_ontology_object_annotation),
        )
        .route(
            "/api/v1/ontology/objects/:object_id/links/:link_type_id",
            get(api_v1_handler::traverse_ontology_links),
        )
        .route("/api/v1/ontology/actions", get(api_v1_handler::list_ontology_actions))
        .route("/api/v1/actions", get(api_v1_handler::list_actions_api))
        .route("/api/v1/actions/library", get(api_v1_handler::get_action_library_api))
        .route("/api/v1/actions/:id", get(api_v1_handler::get_action_api))
        .route("/api/v1/actions/:id/360", get(api_v1_handler::get_action_360))
        .route("/api/v1/actions/:id/review", post(api_v1_handler::upsert_action_review_api))
        .route("/api/v1/actions/bulk-review", post(api_v1_handler::bulk_action_review_api))
        .route("/api/v1/actions/bulk-retry", post(api_v1_handler::bulk_retry_actions_api))
        .route("/api/v1/actions/dead-letter/peek", post(api_v1_handler::peek_dead_letter_api))
        .route("/api/v1/actions/dead-letter/replay", post(api_v1_handler::replay_dead_letter_api))
        .route("/api/v1/actions/export.csv", get(api_v1_handler::export_actions_csv_api))
        .route(
            "/api/v1/ontology/action-types",
            get(api_v1_handler::list_ontology_action_types)
                .post(api_v1_handler::create_ontology_action_type),
        )
        .route(
            "/api/v1/ontology/action-types/:id",
            get(api_v1_handler::get_ontology_action_type)
                .put(api_v1_handler::update_ontology_action_type)
                .delete(api_v1_handler::delete_ontology_action_type),
        )
        .route(
            "/api/v1/ontology/action-types/history",
            get(api_v1_handler::list_ontology_action_type_history_all),
        )
        .route(
            "/api/v1/ontology/action-types/:id/history",
            get(api_v1_handler::list_ontology_action_type_history),
        )
        .route(
            "/api/v1/ontology/action-reviews",
            get(api_v1_handler::list_ontology_action_reviews)
                .post(api_v1_handler::upsert_ontology_action_review),
        )
        .route("/api/v1/ontology/actions/invoke", post(api_v1_handler::invoke_ontology_action))
        .route(
            "/api/v1/service-accounts",
            get(api_v1_handler::list_service_accounts).post(api_v1_handler::create_service_account),
        )
        .route(
            "/api/v1/service-accounts/:id",
            axum::routing::delete(api_v1_handler::revoke_service_account),
        )
        .route(
            "/api/v1/dashboard/layout",
            get(api_v1_handler::get_dashboard_layout)
                .put(api_v1_handler::put_dashboard_layout)
                .delete(api_v1_handler::delete_dashboard_layout),
        )
        .route("/api/v1/data/records", get(api_v1_handler::list_data_records))
        .route("/api/v1/data/compare", get(api_v1_handler::get_data_compare_api))
        .route("/api/v1/data/export.csv", get(api_v1_handler::export_data_csv_api))
        .route("/api/v1/data/records/:id/journey", get(api_v1_handler::get_record_journey_api))
        .route("/api/v1/data/records/:id", get(api_v1_handler::get_data_record))
        .route("/api/v1/data/records/:id/reprocess", post(api_v1_handler::reprocess_data_record))
        .route("/api/v1/data/records/:id/model", post(api_v1_handler::model_data_record_api))
        .route("/api/v1/data/reprocess", post(api_v1_handler::reprocess_data_records))
        .route("/api/v1/data/model", post(api_v1_handler::model_data_records_api))
        .route("/api/v1/work", get(api_v1_handler::get_work_api))
        .route("/api/v1/work/export.csv", get(api_v1_handler::export_work_csv_api))
        .route("/api/v1/work/claim", post(api_v1_handler::claim_work_api))
        .route("/api/v1/search", get(api_v1_handler::global_search_api))
        .route(
            "/api/v1/saved-views",
            get(api_v1_handler::list_saved_views_api).post(api_v1_handler::create_saved_view_api),
        )
        .route(
            "/api/v1/saved-views/:id",
            put(api_v1_handler::update_saved_view_api)
                .delete(api_v1_handler::delete_saved_view_api),
        )
        .route(
            "/api/v1/action-templates",
            get(api_v1_handler::list_action_templates_api)
                .post(api_v1_handler::create_action_template_api),
        )
        .route(
            "/api/v1/action-templates/:id",
            put(api_v1_handler::update_action_template_api)
                .delete(api_v1_handler::delete_action_template_api),
        )
        .route(
            "/api/v1/api-keys",
            get(api_v1_handler::list_api_keys_api).post(api_v1_handler::create_api_key_api),
        )
        .route("/api/v1/api-keys/:id", axum::routing::delete(api_v1_handler::revoke_api_key_api))
        .route("/api/v1/api-keys/bulk-revoke", post(api_v1_handler::bulk_revoke_api_keys_api))
        .route("/api/v1/users", get(api_v1_handler::list_users).post(api_v1_handler::create_user))
        .route(
            "/api/v1/users/:id",
            get(api_v1_handler::get_user_api)
                .put(api_v1_handler::update_user_role)
                .delete(api_v1_handler::delete_user),
        )
        .route("/api/v1/users/bulk-delete", post(api_v1_handler::bulk_delete_users_api))
        .route("/api/v1/users/bulk-role", post(api_v1_handler::bulk_update_user_role_api))
        .route("/api/v1/users/:id/data-subject-export", get(api_v1_handler::export_user_data))
        .route("/api/v1/users/password-policy", get(api_v1_handler::get_password_policy))
        .route("/incidents/export.csv", get(get_incidents_export_csv))
        .route("/incidents/saved-views", post(post_save_incident_view))
        .route("/incidents/saved-views/:id/delete", post(post_delete_incident_view))
        .route("/incidents/bulk-update", axum::routing::post(post_bulk_update_incidents))
        .route("/incidents/:id/export.csv", get(get_incident_export_csv))
        .route("/incidents/:id", get(get_incident_detail).post(post_update_incident))
        .route("/incidents/:id/brief", post(post_generate_incident_brief))
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
        .route("/build", get(build_studio_handler::get_build_studio))
        .route(
            "/build/data-sources",
            get(build_studio_handler::get_data_sources)
                .post(build_studio_handler::post_data_source),
        )
        .route("/build/data-sources/new", get(build_studio_handler::get_new_data_source))
        .route(
            "/build/data-sources/:id",
            get(build_studio_handler::get_data_source_detail)
                .post(build_studio_handler::post_update_data_source),
        )
        .route(
            "/build/pipelines",
            get(pipeline_definitions_handler::get_pipelines)
                .post(pipeline_definitions_handler::post_pipeline),
        )
        .route("/build/pipelines/new", get(pipeline_definitions_handler::get_new_pipeline))
        .route(
            "/build/pipelines/:id",
            get(pipeline_definitions_handler::get_pipeline_detail)
                .post(pipeline_definitions_handler::post_update_pipeline),
        )
        .route("/branding", get(get_branding_page).post(post_branding))
        .route("/pipeline", get(get_pipeline))
        .route("/ontology", get(list_ontology))
        .route("/ontology/objects/:id/360", get(get_ontology_object_360))
        .route("/ontology/compare", get(get_ontology_compare))
        .route("/ontology/export.csv", get(get_ontology_export_csv))
        .route("/ontology/saved-views", axum::routing::post(post_save_ontology_view))
        .route("/ontology/saved-views/:id/delete", axum::routing::post(post_delete_ontology_view))
        .route("/ontology/objects", axum::routing::post(create_ontology_object))
        .route("/ontology/objects/:id/edit", axum::routing::post(update_ontology_object))
        .route(
            "/ontology/objects/:id/annotations",
            axum::routing::post(create_ontology_object_annotation),
        )
        .route("/ontology/objects/:id/delete", axum::routing::post(delete_ontology_object))
        .route("/ontology/objects/bulk-update", axum::routing::post(bulk_update_ontology_objects))
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
        .route("/overview/saved-views/:id/edit", axum::routing::post(post_update_saved_view_name))
        .route("/work", get(get_work))
        .route("/workflows", get(workflow_cases_handler::get_workflow_cases))
        .route("/workflows/:id/decide", post(workflow_cases_handler::post_workflow_decision))
        .route("/apps", get(apps_handler::get_apps).post(apps_handler::post_app))
        .route("/apps/:id", get(apps_handler::get_app_detail).post(apps_handler::post_update_app))
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
        .route("/security/mfa-policy", axum::routing::post(post_mfa_policy))
        .route("/security/oidc-provider", axum::routing::post(post_oidc_provider_policy))
        .route("/security/oidc-config", axum::routing::post(post_tenant_oidc_config))
        .route("/security/oidc-config/delete", axum::routing::post(post_delete_tenant_oidc_config))
        .route(
            "/service-accounts",
            get(service_accounts_handler::get_service_accounts)
                .post(service_accounts_handler::post_service_account),
        )
        .route(
            "/service-accounts/:id/revoke",
            post(service_accounts_handler::post_revoke_service_account),
        )
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
        .route("/reports/schedules/:id/edit", axum::routing::post(post_update_report_schedule))
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
