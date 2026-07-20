use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use common::Role;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/sensors", get(get_sensors).post(post_sensors))
        .route("/sensors/:id/delete", post(post_delete_sensor))
        .route("/sensors/:id/toggle", post(post_toggle_sensor))
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
        execution_client: std::sync::Arc::new(
            crate::execution_client::execution_client_test::InMemoryExecutionClient::default(),
        ),
        analysis_config_client: std::sync::Arc::new(crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient::default()),
        normalization_mappings_client: std::sync::Arc::new(crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: std::sync::Arc::new(crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: std::sync::Arc::new(crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        retention_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        auth_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        ingestion_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        egress_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn viewer_role_does_not_see_register_form_or_write_buttons() {
    let (state, _admin_session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    let viewer_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "viewer-alice".to_string(),
            role: common::Role::Viewer,
            created_at: chrono::Utc::now(),
        })
        .await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={viewer_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("support-poller"));
    assert!(!body.contains("Register an already-deployed sensor"));
    assert!(!body.contains("Generate a deploy script"));
    assert!(!body.contains(">Remove<"));
    assert!(!body.contains(">Disable<"));
    assert!(!body.contains(">Enable<"));
    assert!(!body.contains("Remove selected"));
    assert!(!body.contains(r#"id="bulk-delete-form""#));
}

#[tokio::test]
async fn admin_only_nav_links_hidden_for_viewer_and_operator_shown_for_admin() {
    let (state, admin_session_id, tenant_id) = state_with_session().await;
    let viewer_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "viewer-bob".to_string(),
            role: common::Role::Viewer,
            created_at: chrono::Utc::now(),
        })
        .await;
    let operator_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "operator-carol".to_string(),
            role: common::Role::Operator,
            created_at: chrono::Utc::now(),
        })
        .await;

    for session_id in [&viewer_session_id, &operator_session_id] {
        let response = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/sensors")
                    .header("cookie", format!("kizashi_session={session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(
            !body.contains("href=\"/users\""),
            "non-admin session should not see the admin-only Users nav link"
        );
    }

    let admin_response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={admin_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::OK);
    let admin_bytes = axum::body::to_bytes(admin_response.into_body(), usize::MAX).await.unwrap();
    let admin_body = String::from_utf8(admin_bytes.to_vec()).unwrap();
    assert!(
        admin_body.contains("href=\"/users\""),
        "an Admin session should see the admin-only Users nav link"
    );
}

#[tokio::test]
async fn operator_role_sees_register_form_and_write_buttons() {
    let (state, _admin_session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    let operator_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "operator-alice".to_string(),
            role: common::Role::Operator,
            created_at: chrono::Utc::now(),
        })
        .await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={operator_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Register an already-deployed sensor"));
    assert!(body.contains(">Disable<"));
    assert!(body.contains("Remove selected"));
    assert!(body.contains(r#"id="bulk-delete-form""#));
}

#[tokio::test]
async fn get_sensors_renders_the_sensors_table_when_signed_in() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("support-poller"));
}

#[tokio::test]
async fn get_sensors_remove_button_asks_for_confirmation_before_submitting() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("onsubmit=\"return confirm("));
}

#[tokio::test]
async fn get_sensors_filters_by_the_q_query_param_case_insensitively() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "sql",
            "billing-sync",
            serde_json::json!({}),
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors?q=SUPPORT")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("support-poller"));
    assert!(!body.contains("billing-sync"));
}

#[tokio::test]
async fn get_sensors_shows_a_no_match_empty_state_for_an_unmatched_query() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors?q=nonexistent")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No sensors on this page match"));
}

#[tokio::test]
async fn get_sensors_sorts_by_name_descending_when_dir_is_desc() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "alpha-sensor",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "sql",
            "zeta-sensor",
            serde_json::json!({}),
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors?sort=name&dir=desc")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let alpha_pos = body.find("alpha-sensor").unwrap();
    let zeta_pos = body.find("zeta-sensor").unwrap();
    assert!(zeta_pos < alpha_pos);
}

#[tokio::test]
async fn get_sensors_shows_an_empty_state_with_no_sensors_registered() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No sensors registered yet"));
    assert!(!body.contains("<table>"));
}

#[tokio::test]
async fn get_sensors_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/sensors").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
