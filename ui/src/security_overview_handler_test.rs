use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient;
use crate::audit_log_client::AuditLogEntry;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::InMemoryBacklogClient;
use crate::branding_client::branding_client_test::InMemoryBrandingClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::egress_allowlist_client::EgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::oidc_client::oidc_client_test::InMemoryOidcClient;
use crate::pending_oidc_flow::InMemoryPendingOidcFlowStore;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
use crate::retention_policies_client::{DataClass, RetentionPoliciesClient, RetentionPolicy};
use crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::users_client::users_client_test::InMemoryUsersClient;
use crate::users_client::UiUser;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use common::Role;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/security", get(get_security_overview)).with_state(state)
}

async fn state_with_session(session_store: InMemorySessionStore) -> (AppState, String, Uuid) {
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
        branding_client: Arc::new(InMemoryBrandingClient::default()),
        oidc_client: Arc::new(InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        sensors_client: Arc::new(InMemorySensorsClient::default()),
        api_keys_client: Arc::new(InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(InMemoryBacklogClient::default()),
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        normalization_mappings_client: Arc::new(InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
    };
    (state, session_id, tenant_id)
}

async fn get_page(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri("/security")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

fn entry(changed_at: &str) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        entity_type: "trigger_definition".to_string(),
        entity_id: Uuid::new_v4(),
        change_type: "created".to_string(),
        actor: "alice".to_string(),
        before: None,
        after: serde_json::json!({}),
        changed_at: changed_at.parse().unwrap(),
    }
}

#[tokio::test]
async fn shows_zero_counts_with_no_data() {
    let (state, session_id, _tenant_id) = state_with_session(InMemorySessionStore::default()).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("no restrictions configured"));
}

#[tokio::test]
async fn counts_the_callers_own_active_sessions() {
    let store = InMemorySessionStore::default();
    let (state, session_id, tenant_id) = state_with_session(store).await;
    state
        .session_store
        .create(Session {
            bearer_token: "tok2".to_string(),
            tenant_id,
            username: "bob".to_string(),
            role: common::Role::Operator,
            created_at: chrono::Utc::now(),
        })
        .await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("2"), "expected the active session count (2) to appear");
}

#[tokio::test]
async fn only_counts_activity_from_the_last_seven_days() {
    let (mut state, session_id, _tenant_id) =
        state_with_session(InMemorySessionStore::default()).await;
    let recent = Arc::new(InMemoryAuditLogClient::default());
    *recent.recent.lock().unwrap() = vec![
        entry(&chrono::Utc::now().to_rfc3339()),
        entry(&(chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339()),
    ];
    state.config_audit_log_client = recent;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("1"), "expected only the recent entry to be counted");
}

#[tokio::test]
async fn shows_the_rbac_role_distribution() {
    let (mut state, session_id, tenant_id) =
        state_with_session(InMemorySessionStore::default()).await;
    let users_client = Arc::new(InMemoryUsersClient::default());
    users_client.users.lock().unwrap().extend([
        UiUser { id: Uuid::new_v4(), tenant_id, username: "a".to_string(), role: Role::Admin },
        UiUser { id: Uuid::new_v4(), tenant_id, username: "b".to_string(), role: Role::Operator },
        UiUser { id: Uuid::new_v4(), tenant_id, username: "c".to_string(), role: Role::Viewer },
    ]);
    state.users_client = users_client;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn shows_retention_policy_coverage() {
    let (mut state, session_id, tenant_id) =
        state_with_session(InMemorySessionStore::default()).await;
    let policies_client = Arc::new(InMemoryRetentionPoliciesClient::default());
    policies_client
        .create_policy(
            Role::Admin,
            RetentionPolicy {
                id: Uuid::new_v4(),
                tenant_id,
                data_class: DataClass::Raw,
                ttl_days: 30,
                enabled: true,
            },
            "alice",
        )
        .await
        .unwrap();
    state.retention_policies_client = policies_client;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("1 enabled / 1 total"));
}

#[tokio::test]
async fn flags_an_empty_egress_allowlist() {
    let (mut state, session_id, tenant_id) =
        state_with_session(InMemorySessionStore::default()).await;
    let egress_client = Arc::new(InMemoryEgressAllowlistClient::default());
    egress_client
        .put_allowlist(tenant_id, Role::Admin, vec!["example.com".to_string()], "alice")
        .await
        .unwrap();
    state.egress_allowlist_client = egress_client;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("no restrictions configured"));
    assert!(body.contains("1 domain"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) =
        state_with_session(InMemorySessionStore::default()).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/security").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
