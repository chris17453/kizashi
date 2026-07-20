use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::session::SessionStore;
use crate::session::{InMemorySessionStore, Session};
use crate::triggers_client::triggers_client_test::{FailingTriggersClient, InMemoryTriggersClient};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/triggers", get(get_triggers).post(post_trigger)).with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Admin).await;
    (state, session_id)
}

async fn state_with_session_and_role(role: common::Role) -> (AppState, String, Uuid) {
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
        sensors_client: Arc::new(crate::sensors_client::sensors_client_test::InMemorySensorsClient::default()),
        api_keys_client: Arc::new(crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default()),
        execution_client: std::sync::Arc::new(crate::execution_client::execution_client_test::InMemoryExecutionClient::default()),
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
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn shows_an_empty_state_with_no_triggers_configured() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No triggers configured yet"));
    assert!(!body.contains("<table>"));
}

#[tokio::test]
async fn renders_the_triggers_table_when_signed_in() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "high-volume-negative".to_string(),
        event_type_match: "sentiment".to_string(),
        enabled: true,
    });
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("high-volume-negative"));
    assert!(
        body.contains(r#"scope="col""#),
        "table headers should carry scope=\"col\" for screen readers"
    );
}

#[tokio::test]
async fn sorts_by_name_descending_when_dir_is_desc() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "alpha-trigger".to_string(),
        event_type_match: "sentiment".to_string(),
        enabled: true,
    });
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "zeta-trigger".to_string(),
        event_type_match: "ticket".to_string(),
        enabled: true,
    });
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers?sort=name&dir=desc")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let alpha_pos = body.find("alpha-trigger").unwrap();
    let zeta_pos = body.find("zeta-trigger").unwrap();
    assert!(zeta_pos < alpha_pos);
}

#[tokio::test]
async fn sorts_by_enabled_status() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "enabled-trigger".to_string(),
        event_type_match: "sentiment".to_string(),
        enabled: true,
    });
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "disabled-trigger".to_string(),
        event_type_match: "ticket".to_string(),
        enabled: false,
    });
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers?sort=enabled")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let enabled_pos = body.find("enabled-trigger").unwrap();
    let disabled_pos = body.find("disabled-trigger").unwrap();
    assert!(enabled_pos < disabled_pos, "enabled triggers should sort first");
}

#[tokio::test]
async fn filters_by_the_q_query_param_case_insensitively() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "high-volume-negative".to_string(),
        event_type_match: "sentiment".to_string(),
        enabled: true,
    });
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "urgent-ticket".to_string(),
        event_type_match: "ticket".to_string(),
        enabled: true,
    });
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers?q=URGENT")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("urgent-ticket"));
    assert!(!body.contains("high-volume-negative"));
}

#[tokio::test]
async fn shows_a_no_match_empty_state_for_an_unmatched_query() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "high-volume-negative".to_string(),
        event_type_match: "sentiment".to_string(),
        enabled: true,
    });
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers?q=nonexistent")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No triggers on this page match"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/triggers").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_a_next_link_when_there_are_more_triggers_but_no_previous_link_on_page_zero() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    *triggers_client.has_more.lock().unwrap() = true;
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Next"));
    assert!(!body.contains("Previous"));
}

#[tokio::test]
async fn shows_a_previous_link_on_page_two() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers?page=1")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Previous"));
    assert!(body.contains("Page 2"));
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.triggers_client = Arc::new(FailingTriggersClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("unreachable"));
}
