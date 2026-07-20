use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::audit_log_client::audit_log_client_test::{
    FailingAuditLogClient, InMemoryAuditLogClient,
};
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/audit-log", get(get_recent_audit_log))
        .route("/audit-log/export.csv", get(get_recent_audit_log_csv))
        .with_state(state)
}

async fn state_with_session() -> (AppState, String, Uuid) {
    let session_store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "alice".to_string(),
            role: common::Role::Admin,
            created_at: chrono::Utc::now(),
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(
            crate::branding_client::branding_client_test::InMemoryBrandingClient::default(),
        ),
        oidc_client: Arc::new(crate::oidc_client::oidc_client_test::InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(
            crate::pending_oidc_flow::InMemoryPendingOidcFlowStore::default(),
        ),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        sensors_client: Arc::new(InMemorySensorsClient::default()),
        api_keys_client: Arc::new(
            crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default(),
        ),
        backlog_client: Arc::new(
            crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default(),
        ),
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        normalization_mappings_client: Arc::new(InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        ingestion_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        egress_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        users_client: Arc::new(
            crate::users_client::users_client_test::InMemoryUsersClient::default(),
        ),
        saved_search_queries_client: Arc::new(
            crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

fn entry(actor: &str, changed_at: &str) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        entity_type: "trigger_definition".to_string(),
        entity_id: Uuid::new_v4(),
        change_type: "created".to_string(),
        actor: actor.to_string(),
        before: None,
        after: serde_json::json!({}),
        changed_at: changed_at.parse().unwrap(),
    }
}

async fn get_page(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    get_page_at(state, session_id, "/audit-log").await
}

async fn get_page_at(state: AppState, session_id: &str, uri: &str) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn shows_an_empty_state_with_no_activity() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No audit activity"));
}

#[tokio::test]
async fn merges_and_sorts_entries_from_all_three_services_most_recent_first() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;

    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() = vec![entry("config-actor", "2026-07-18T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let retention_client = Arc::new(InMemoryAuditLogClient::default());
    *retention_client.recent.lock().unwrap() =
        vec![entry("retention-actor", "2026-07-20T00:00:00Z")];
    state.retention_audit_log_client = retention_client;

    let auth_client = Arc::new(InMemoryAuditLogClient::default());
    *auth_client.recent.lock().unwrap() = vec![entry("auth-actor", "2026-07-19T00:00:00Z")];
    state.auth_audit_log_client = auth_client;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let retention_pos = body.find("retention-actor").expect("retention-actor missing");
    let auth_pos = body.find("auth-actor").expect("auth-actor missing");
    let config_pos = body.find("config-actor").expect("config-actor missing");
    assert!(retention_pos < auth_pos, "most recent entry should render first");
    assert!(auth_pos < config_pos, "entries should be ordered most-recent-first");
}

#[tokio::test]
async fn filters_by_the_q_query_param_case_insensitively() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() =
        vec![entry("alice", "2026-07-18T00:00:00Z"), entry("bob", "2026-07-19T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let response = get_page_at(state, &session_id, "/audit-log?q=ALICE").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("alice"));
    assert!(!body.contains(">bob<"));
}

#[tokio::test]
async fn shows_a_no_match_empty_state_for_an_unmatched_query() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() = vec![entry("alice", "2026-07-18T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let response = get_page_at(state, &session_id, "/audit-log?q=nonexistent").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No audit activity on this page matches"));
}

#[tokio::test]
async fn shows_a_load_older_link_when_a_full_page_is_returned() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() = vec![entry("actor", "2026-07-18T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("/audit-log?q=&before="));
}

#[tokio::test]
async fn shows_an_error_when_a_backend_fails_but_still_renders_the_others() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    state.config_audit_log_client = Arc::new(FailingAuditLogClient);
    let auth_client = Arc::new(InMemoryAuditLogClient::default());
    *auth_client.recent.lock().unwrap() = vec![entry("auth-actor", "2026-07-19T00:00:00Z")];
    state.auth_audit_log_client = auth_client;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("unreachable"));
    assert!(body.contains("auth-actor"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/audit-log").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

async fn get_csv(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    get_csv_at(state, session_id, "/audit-log/export.csv").await
}

async fn get_csv_at(state: AppState, session_id: &str, uri: &str) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn csv_export_has_the_right_content_type_and_filename() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = get_csv(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/csv");
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("attachment"));
}

#[tokio::test]
async fn csv_export_includes_the_header_row_and_merged_entries() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() = vec![entry("config-actor", "2026-07-18T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let response = get_csv(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.starts_with("changed_at,service,entity_type,change_type,actor\n"));
    assert!(body.contains("config-actor"));
    assert!(body.contains("config-admin-service"));
}

#[tokio::test]
async fn csv_export_escapes_a_comma_in_a_field() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() =
        vec![entry("actor, with a comma", "2026-07-18T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let response = get_csv(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("\"actor, with a comma\""));
}

#[tokio::test]
async fn csv_export_honors_the_before_query_param_as_the_starting_cursor() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() = vec![
        entry("newer-actor", "2026-07-19T00:00:00Z"),
        entry("older-actor", "2026-07-17T00:00:00Z"),
    ];
    state.config_audit_log_client = config_client;

    let response =
        get_csv_at(state, &session_id, "/audit-log/export.csv?before=2026-07-18T00:00:00Z").await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("older-actor"));
    assert!(!body.contains("newer-actor"));
}

#[tokio::test]
async fn csv_export_sets_x_next_before_header_when_more_history_remains() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    // CSV_MAX_PAGES * CSV_PAGE_LIMIT rows guarantees every page fetched in the export loop is a
    // full page, so the loop runs out of iterations rather than hitting a natural empty page --
    // exactly the "there might be more" case the X-Next-Before header exists for.
    let base = "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    let seeded: Vec<AuditLogEntry> = (0..(CSV_MAX_PAGES * CSV_PAGE_LIMIT as usize))
        .map(|i| {
            let mut e = entry("bulk-actor", "2026-01-01T00:00:00Z");
            e.changed_at = base - chrono::Duration::seconds(i as i64);
            e
        })
        .collect();
    *config_client.recent.lock().unwrap() = seeded;
    state.config_audit_log_client = config_client;

    let response = get_csv(state, &session_id).await;

    assert!(response.headers().get("x-next-before").is_some());
}

#[tokio::test]
async fn csv_export_has_no_x_next_before_header_when_history_is_fully_exported() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    *config_client.recent.lock().unwrap() = vec![entry("only-actor", "2026-07-18T00:00:00Z")];
    state.config_audit_log_client = config_client;

    let response = get_csv(state, &session_id).await;

    assert!(response.headers().get("x-next-before").is_none());
}

#[tokio::test]
async fn csv_export_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/audit-log/export.csv").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
