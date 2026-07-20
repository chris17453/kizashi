use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
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
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/retention-policies", get(get_retention_policies).post(post_retention_policies))
        .route("/retention-policies/:id/toggle", post(post_toggle_retention_policy))
        .route("/retention-policies/:id/edit", post(post_edit_retention_policy))
        .route("/retention-policies/:id/delete", post(post_delete_retention_policy))
        .route("/retention-policies/bulk-delete", post(post_bulk_delete_retention_policies))
        .with_state(state)
}

async fn state_with_session(role: common::Role) -> (AppState, String, Uuid) {
    let session_store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "alice".to_string(),
            role,
            created_at: chrono::Utc::now(),
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(crate::branding_client::branding_client_test::InMemoryBrandingClient::default()),
        oidc_client: Arc::new(crate::oidc_client::oidc_client_test::InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(crate::pending_oidc_flow::InMemoryPendingOidcFlowStore::default()),
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
        egress_allowlist_client: Arc::new(
            crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient::default(),
        ),
        config_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        retention_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        auth_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        ingestion_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        egress_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
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

#[tokio::test]
async fn post_bulk_delete_removes_every_selected_policy() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let first = crate::retention_policies_client::RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: crate::retention_policies_client::DataClass::Raw,
        ttl_days: 30,
        enabled: true,
    };
    let second = crate::retention_policies_client::RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: crate::retention_policies_client::DataClass::Normalized,
        ttl_days: 60,
        enabled: true,
    };
    let untouched = crate::retention_policies_client::RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: crate::retention_policies_client::DataClass::Event,
        ttl_days: 90,
        enabled: true,
    };
    let policies_client = Arc::new(InMemoryRetentionPoliciesClient {
        policies: std::sync::Mutex::new(vec![first.clone(), second.clone(), untouched.clone()]),
    });
    let mut state = state;
    state.retention_policies_client = policies_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/retention-policies/bulk-delete")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={}&ids={}", first.id, second.id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let remaining = policies_client.policies.lock().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, untouched.id);
}

#[tokio::test]
async fn post_bulk_delete_with_no_selection_is_a_no_op() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let policy = crate::retention_policies_client::RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: crate::retention_policies_client::DataClass::Raw,
        ttl_days: 30,
        enabled: true,
    };
    let policies_client =
        Arc::new(InMemoryRetentionPoliciesClient { policies: std::sync::Mutex::new(vec![policy]) });
    let mut state = state;
    state.retention_policies_client = policies_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/retention-policies/bulk-delete")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(policies_client.policies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn post_bulk_delete_rejects_a_viewer_role() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Viewer).await;
    let policy_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/retention-policies/bulk-delete")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={policy_id}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
