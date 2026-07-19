use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::{FailingHealthClient, InMemoryHealthClient};
use crate::session::SessionStore;
use crate::session::{InMemorySessionStore, Session};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::PlatformHealthSummary;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/health", get(get_health)).with_state(state)
}

async fn state_with_session(health_client: Arc<dyn crate::HealthClient>) -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
            username: "alice".to_string(),
            role: common::Role::Admin,
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client,
        agents_client: Arc::new(crate::agents_client::agents_client_test::InMemoryAgentsClient::default()),
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
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id)
}

#[tokio::test]
async fn renders_platform_health_when_signed_in() {
    let health_client = Arc::new(InMemoryHealthClient {
        summary: PlatformHealthSummary {
            status: "up".to_string(),
            services: vec![ServiceHealthSummary {
                name: "ingestion-service".to_string(),
                status: "up".to_string(),
            }],
        },
    });
    let (state, session_id) = state_with_session(health_client).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("ingestion-service"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session(Arc::new(FailingHealthClient)).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (state, session_id) = state_with_session(Arc::new(FailingHealthClient)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/health")
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
