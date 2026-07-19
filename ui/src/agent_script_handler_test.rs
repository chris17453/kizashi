use super::*;
use crate::agents_client::agents_client_test::InMemoryAgentsClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/agents/generate", get(get_generate_select))
        .route("/agents/generate/form", get(get_generate_form))
        .route("/agents/generate/script", post(post_generate_script))
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
async fn get_generate_select_lists_every_connector_type() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents/generate")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Zendesk"));
    assert!(body.contains("Fabric"));
}

#[tokio::test]
async fn get_generate_select_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/agents/generate").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn get_generate_form_shows_the_zendesk_specific_fields() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents/generate/form?connector_type=zendesk")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("ZENDESK_SUBDOMAIN"));
    assert!(body.contains("ZENDESK_API_TOKEN"));
}

#[tokio::test]
async fn get_generate_form_auto_fills_a_freshly_generated_api_key_for_an_operator() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents/generate/form?connector_type=zendesk")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("kzsh_"), "should pre-fill an auto-generated key, not a blank field");
    assert!(body.contains("generated automatically"));
}

#[tokio::test]
async fn get_generate_form_leaves_the_api_key_blank_for_a_viewer() {
    let session_store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "viewer".to_string(),
            role: common::Role::Viewer,
        })
        .await;
    let (mut state, _session_id, _tenant_id) = state_with_session().await;
    state.session_store = Arc::new(session_store);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents/generate/form?connector_type=zendesk")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("kzsh_"));
    assert!(body.contains("can't create API keys"));
}

#[tokio::test]
async fn post_generate_script_renders_all_three_variants_with_submitted_values() {
    let (state, session_id, tenant_id) = state_with_session().await;

    let body = "connector_type=zendesk&name=support-poller&gateway_url=http%3A%2F%2Fexample.com&api_key=my-secret-key&ZENDESK_SUBDOMAIN=acme&ZENDESK_EMAIL=ops%40acme.com&ZENDESK_API_TOKEN=tok-123";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents/generate/script")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(rendered.contains("support-poller"));
    assert!(rendered.contains("my-secret-key"));
    assert!(rendered.contains("acme"));
    assert!(rendered.contains(&tenant_id.to_string()));
    assert!(rendered.contains("zendesk-connector"));
    assert!(rendered.contains("cargo run --release -p connector-zendesk"));
    assert!(rendered.contains("$env:ZENDESK_SUBDOMAIN"));
}

#[tokio::test]
async fn post_generate_script_omits_empty_optional_fields() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let body = "connector_type=generic&name=my-generic&gateway_url=http%3A%2F%2Fexample.com&api_key=key&GENERIC_SOURCE_URL=http%3A%2F%2Fsource.example.com&GENERIC_BEARER_TOKEN=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents/generate/script")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!rendered.contains("GENERIC_BEARER_TOKEN"));
}
