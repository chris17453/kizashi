use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::InMemoryBacklogClient;
use crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient;
use crate::backup_status_client::BackupRun;
use crate::branding_client::branding_client_test::InMemoryBrandingClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient;
use crate::login_attempts_client::LoginAttempt;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::oidc_client::oidc_client_test::InMemoryOidcClient;
use crate::pending_oidc_flow::InMemoryPendingOidcFlowStore;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
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
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/security/compliance-report", get(get_compliance_report)).with_state(state)
}

fn sample_session(tenant_id: Uuid, role: Role) -> Session {
    Session {
        bearer_token: "tok".to_string(),
        tenant_id,
        username: "alice".to_string(),
        role,
        created_at: chrono::Utc::now(),
    }
}

fn base_state(
    session_store: InMemorySessionStore,
    users_client: InMemoryUsersClient,
    login_attempts_client: InMemoryLoginAttemptsClient,
    backup_status_client: InMemoryBackupStatusClient,
) -> AppState {
    AppState {
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
        users_client: Arc::new(users_client),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
        login_attempts_client: Arc::new(login_attempts_client),
        backup_status_client: Arc::new(backup_status_client),
    }
}

async fn get_page(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri("/security/compliance-report")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_sees_a_full_snapshot() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin)).await;

    let users_client = InMemoryUsersClient::default();
    users_client.users.lock().unwrap().extend([
        UiUser {
            id: Uuid::new_v4(),
            tenant_id,
            username: "alice".to_string(),
            role: Role::Admin,
            mfa_enabled: true,
        },
        UiUser {
            id: Uuid::new_v4(),
            tenant_id,
            username: "bob".to_string(),
            role: Role::Viewer,
            mfa_enabled: false,
        },
    ]);
    let login_attempts_client = InMemoryLoginAttemptsClient::default();
    login_attempts_client.attempts.lock().unwrap().push(LoginAttempt {
        username: "bob".to_string(),
        success: false,
        reason: "wrong_password".to_string(),
        attempted_at: chrono::Utc::now(),
    });
    let backup_status_client = InMemoryBackupStatusClient::default();
    backup_status_client.runs.lock().unwrap().push(BackupRun {
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        status: "success".to_string(),
        target: "postgres/test.dump".to_string(),
        size_bytes: Some(1024),
        error: None,
    });

    let state = base_state(store, users_client, login_attempts_client, backup_status_client);
    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("2 local user account"));
    assert!(body.contains("1 of 2 account"));
    assert!(body.contains("Minimum 12 characters"));
    assert!(body.contains("1 failed local-login attempt"));
    assert!(body.contains("success"));
}

#[tokio::test]
async fn non_admin_gets_forbidden() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Operator)).await;
    let state = base_state(
        store,
        InMemoryUsersClient::default(),
        InMemoryLoginAttemptsClient::default(),
        InMemoryBackupStatusClient::default(),
    );

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let state = base_state(
        InMemorySessionStore::default(),
        InMemoryUsersClient::default(),
        InMemoryLoginAttemptsClient::default(),
        InMemoryBackupStatusClient::default(),
    );

    let response = router(state)
        .oneshot(Request::builder().uri("/security/compliance-report").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
