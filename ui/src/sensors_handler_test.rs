use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::sensors_client::sensors_client_test::{FailingSensorsClient, InMemorySensorsClient};
use crate::sensors_client::SensorsClient;
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
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
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
}

#[tokio::test]
async fn operator_role_sees_register_form_and_write_buttons() {
    let (state, _admin_session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
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
}

#[tokio::test]
async fn get_sensors_renders_the_sensors_table_when_signed_in() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .sensors_client
        .register_sensor(
            Role::Operator,
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

#[tokio::test]
async fn post_sensors_registers_and_redirects() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let sensors_client = Arc::new(InMemorySensorsClient::default());
    state.sensors_client = sensors_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("connector_type=zendesk&name=support-poller&config={}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/sensors");
    assert_eq!(sensors_client.sensors.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn post_sensors_with_invalid_json_config_rerenders_with_an_error() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("connector_type=zendesk&name=support-poller&config=not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("must be valid JSON"));
}

#[tokio::test]
async fn post_sensors_backend_failure_rerenders_with_an_error() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    state.sensors_client = Arc::new(FailingSensorsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sensors")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("connector_type=zendesk&name=support-poller&config={}"))
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
async fn post_delete_sensor_removes_it_and_redirects() {
    let (mut state, session_id, tenant_id) = state_with_session().await;
    let sensors_client = Arc::new(InMemorySensorsClient::default());
    let sensor = sensors_client
        .register_sensor(
            Role::Operator,
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    state.sensors_client = sensors_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sensors/{}/delete", sensor.id))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(sensors_client.sensors.lock().unwrap().is_empty());
}

#[tokio::test]
async fn post_toggle_sensor_flips_enabled_and_redirects() {
    let (mut state, session_id, tenant_id) = state_with_session().await;
    let sensors_client = Arc::new(InMemorySensorsClient::default());
    let sensor = sensors_client
        .register_sensor(
            Role::Operator,
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    assert!(sensor.enabled);
    state.sensors_client = sensors_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sensors/{}/toggle", sensor.id))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let stored = sensors_client.sensors.lock().unwrap();
    assert!(!stored.iter().find(|a| a.id == sensor.id).unwrap().enabled);
}

#[tokio::test]
async fn post_toggle_sensor_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sensors/{}/toggle", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn shows_a_next_link_when_there_are_more_sensors_but_no_previous_link_on_page_zero() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let sensors_client = Arc::new(InMemorySensorsClient::default());
    *sensors_client.has_more.lock().unwrap() = true;
    state.sensors_client = sensors_client;

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
    assert!(body.contains("Next"));
    assert!(!body.contains("Previous"));
}

#[tokio::test]
async fn shows_a_previous_link_on_page_two() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/sensors?page=1")
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
