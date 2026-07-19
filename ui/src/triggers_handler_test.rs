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

#[tokio::test]
async fn post_creates_a_threshold_trigger_and_redirects() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Operator).await;
    let triggers_client =
        Arc::new(crate::triggers_client::triggers_client_test::InMemoryTriggersClient::default());
    let mut state = state;
    state.triggers_client = triggers_client.clone();

    let body = "name=urgent-spike&event_type_match=priority_score&window_seconds=3600&condition_shape=threshold_over_window&field=priority_score&threshold=5&direction=above&count=&action_url=https%3A%2F%2Fexample.com%2Fhook";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let created = triggers_client.created.lock().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].name, "urgent-spike");
    assert_eq!(created[0].event_type_match, "priority_score");
    assert_eq!(created[0].window_seconds, 3600);
    assert!(matches!(
        created[0].condition,
        common::TriggerCondition::ThresholdOverWindow { threshold, .. } if threshold == 5.0
    ));
    assert_eq!(created[0].actions.len(), 1);
}

#[tokio::test]
async fn post_creates_a_count_trigger() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Operator).await;
    let triggers_client =
        Arc::new(crate::triggers_client::triggers_client_test::InMemoryTriggersClient::default());
    let mut state = state;
    state.triggers_client = triggers_client.clone();

    let body = "name=high-volume&event_type_match=sentiment&window_seconds=1800&condition_shape=count_over_window&field=&threshold=&direction=above&count=3&action_url=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let created = triggers_client.created.lock().unwrap();
    assert_eq!(created.len(), 1);
    assert!(matches!(
        created[0].condition,
        common::TriggerCondition::CountOverWindow { count } if count == 3
    ));
    assert!(created[0].actions.is_empty());
}

#[tokio::test]
async fn post_rerenders_with_a_form_error_when_the_threshold_field_is_missing() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Operator).await;

    let body = "name=bad&event_type_match=x&window_seconds=3600&condition_shape=threshold_over_window&field=&threshold=&direction=above&count=&action_url=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("field is required"));
}

#[tokio::test]
async fn post_creates_a_correlated_trigger_deriving_event_type_match_from_the_first_leg() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Operator).await;
    let triggers_client =
        Arc::new(crate::triggers_client::triggers_client_test::InMemoryTriggersClient::default());
    let mut state = state;
    state.triggers_client = triggers_client.clone();

    let body = "name=email-and-chat&event_type_match=&window_seconds=3600&condition_shape=correlated_over_window&field=&threshold=&direction=above&count=&action_url=&correlated_event_type_1=sentiment_drop_email&correlated_min_count_1=1&correlated_event_type_2=unresolved_chat&correlated_min_count_2=1&correlated_event_type_3=&correlated_min_count_3=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let created = triggers_client.created.lock().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].event_type_match, "sentiment_drop_email");
    match &created[0].condition {
        common::TriggerCondition::CorrelatedOverWindow { conditions } => {
            assert_eq!(conditions.len(), 2);
            assert_eq!(conditions[0].event_type, "sentiment_drop_email");
            assert_eq!(conditions[0].min_count, 1);
            assert_eq!(conditions[1].event_type, "unresolved_chat");
            assert_eq!(conditions[1].min_count, 1);
        }
        other => panic!("expected CorrelatedOverWindow, got {other:?}"),
    }
}

#[tokio::test]
async fn post_rerenders_with_a_form_error_when_no_correlated_rows_are_filled_in() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Operator).await;

    let body = "name=bad&event_type_match=&window_seconds=3600&condition_shape=correlated_over_window&field=&threshold=&direction=above&count=&action_url=&correlated_event_type_1=&correlated_min_count_1=&correlated_event_type_2=&correlated_min_count_2=&correlated_event_type_3=&correlated_min_count_3=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("at least one correlated"));
}

#[tokio::test]
async fn post_rerenders_with_a_form_error_when_a_correlated_row_has_an_invalid_min_count() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Operator).await;

    let body = "name=bad&event_type_match=&window_seconds=3600&condition_shape=correlated_over_window&field=&threshold=&direction=above&count=&action_url=&correlated_event_type_1=sentiment_drop_email&correlated_min_count_1=not-a-number&correlated_event_type_2=&correlated_min_count_2=&correlated_event_type_3=&correlated_min_count_3=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("valid min count"));
}

#[tokio::test]
async fn post_rejects_a_viewer_role() {
    let (state, session_id, _tenant_id) = state_with_session_and_role(common::Role::Viewer).await;

    let body = "name=x&event_type_match=x&window_seconds=3600&condition_shape=count_over_window&field=&threshold=&direction=above&count=1&action_url=";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
