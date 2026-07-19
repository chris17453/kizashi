use super::*;
use crate::agents_client::agents_client_test::InMemoryAgentsClient;
use crate::agents_client::AgentsClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::{PlatformHealthSummary, ServiceHealthSummary};
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::ConnectorStatSummary;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use common::Role;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/overview", get(get_overview)).with_state(state)
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
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        agents_client: Arc::new(InMemoryAgentsClient::default()),
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
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn renders_kpi_cards_reflecting_real_data_when_signed_in() {
    let (mut state, session_id, tenant_id) = state_with_session().await;

    let agents_client = Arc::new(InMemoryAgentsClient::default());
    agents_client
        .register_agent(
            Role::Operator,
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    agents_client
        .register_agent(Role::Operator, tenant_id, "sql", "never-run-agent", serde_json::json!({}))
        .await
        .unwrap();
    state.agents_client = agents_client;

    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.stats.lock().unwrap().push(ConnectorStatSummary {
        connector_id: "support-poller".to_string(),
        record_count: 42,
        last_ingested_at: chrono::Utc::now(),
    });
    state.stats_client = stats_client;

    state.health_client = Arc::new(InMemoryHealthClient {
        summary: PlatformHealthSummary {
            status: "up".to_string(),
            services: vec![
                ServiceHealthSummary { name: "a".to_string(), status: "up".to_string() },
                ServiceHealthSummary { name: "b".to_string(), status: "down".to_string() },
            ],
        },
    });

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(">2<")); // agent_count
    assert!(body.contains("1 active")); // only support-poller has matching stats
    assert!(body.contains(">42<")); // total_records
    assert!(body.contains("1/2 services up"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/overview").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
