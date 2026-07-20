use super::*;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::{FailingBacklogClient, InMemoryBacklogClient};
use crate::backlog_client::QueueDepthSummary;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::{FailingHealthClient, InMemoryHealthClient};
use crate::health_client::{PlatformHealthSummary, ServiceHealthSummary};
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
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
    Router::new().route("/pipeline", get(get_pipeline)).with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
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
            summary: PlatformHealthSummary {
                status: "up".to_string(),
                services: vec![
                    ServiceHealthSummary {
                        name: "ingestion-service".to_string(),
                        status: "up".to_string(),
                    },
                    ServiceHealthSummary {
                        name: "normalization-service".to_string(),
                        status: "up".to_string(),
                    },
                    ServiceHealthSummary {
                        name: "analysis-service".to_string(),
                        status: "down".to_string(),
                    },
                ],
            },
        }),
        sensors_client: Arc::new(InMemorySensorsClient::default()),
        api_keys_client: Arc::new(InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(InMemoryBacklogClient::default()),
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
    (state, session_id)
}

#[tokio::test]
async fn renders_all_five_pipeline_stages_with_their_health_status() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Ingestion"));
    assert!(body.contains("Trigger Engine"));
    assert!(body.contains("Action Executor"));
    assert!(body.contains("node-up"));
    assert!(body.contains("node-down"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/pipeline").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_an_error_when_health_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.health_client = Arc::new(FailingHealthClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
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

#[tokio::test]
async fn degrades_gracefully_showing_stages_with_no_backlog_numbers_when_backlog_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.backlog_client = Arc::new(FailingBacklogClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Ingestion"));
    assert!(body.contains("n/a"));
}

#[tokio::test]
async fn marks_a_heavily_backlogged_queue_as_critical() {
    let (state, session_id) = state_with_session().await;
    let backlog = InMemoryBacklogClient::default();
    backlog.depths.lock().unwrap().push(QueueDepthSummary {
        stage: "ingest_to_normalize".to_string(),
        queue_name: "normalization-service.record.ingested".to_string(),
        messages: 500,
    });
    let state = AppState { backlog_client: Arc::new(backlog), ..state };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("edge-critical"));
    assert!(body.contains("500 queued"));
}
