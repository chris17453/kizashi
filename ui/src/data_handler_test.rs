use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::{
    FailingIngestionStatsClient, InMemoryIngestionStatsClient,
};
use crate::saved_search_queries_client::SavedSearchQueriesClient;
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
    Router::new()
        .route("/data", get(get_data))
        .route("/data/export.csv", get(get_data_export_csv))
        .route("/data/reprocess", axum::routing::post(post_reprocess))
        .route("/data/saved-searches", axum::routing::post(post_save_search))
        .route("/data/saved-searches/:id/delete", axum::routing::post(post_delete_saved_search))
        .with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    state_with_session_role(common::Role::Admin).await
}

async fn state_with_session_role(role: common::Role) -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
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
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id)
}

#[tokio::test]
async fn renders_search_results_when_signed_in() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: Uuid::new_v4(),
        connector_id: "zendesk".to_string(),
        source_type: "ticket".to_string(),
        ingested_at: chrono::Utc::now(),
        raw_payload: serde_json::json!({"subject": "printer on fire"}),
        normalized_payload: None,
    });
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?q=printer")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("zendesk"));
}

#[tokio::test]
async fn export_csv_includes_the_header_row_and_matching_records() {
    let (mut state, session_id) = state_with_session().await;
    let record_id = Uuid::new_v4();
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: record_id,
        connector_id: "zendesk".to_string(),
        source_type: "ticket".to_string(),
        ingested_at: "2026-07-18T00:00:00Z".parse().unwrap(),
        raw_payload: serde_json::json!({"subject": "printer on fire"}),
        normalized_payload: None,
    });
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data/export.csv")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/csv");
    assert!(response.headers().get("content-disposition").is_some());
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.starts_with("id,connector_id,source_type,ingested_at,normalized,raw_payload\n"));
    assert!(body.contains(&record_id.to_string()));
    assert!(body.contains("zendesk"));
    assert!(body.contains("printer on fire"));
    assert!(body.contains(",false,")); // not normalized
}

#[tokio::test]
async fn export_csv_requires_a_session() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/data/export.csv").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn export_csv_returns_500_when_the_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.stats_client = Arc::new(FailingIngestionStatsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data/export.csv")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn date_range_and_normalization_filters_are_prefilled_from_the_query_string() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?from=2026-07-15&to=2026-07-20&normalized=false")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(r#"id="from" name="from" value="2026-07-15""#));
    assert!(body.contains(r#"id="to" name="to" value="2026-07-20""#));
    let option_start = body.find(r#"value="false""#).expect("normalized=false option missing");
    let option_end = body[option_start..].find('>').unwrap() + option_start;
    assert!(
        body[option_start..option_end].contains("selected"),
        "the \"Not yet normalized\" option should be selected: {}",
        &body[option_start..option_end]
    );
}

#[tokio::test]
async fn offers_registered_sensor_names_as_a_datalist_for_the_connector_id_field() {
    let (mut state, session_id) = state_with_session().await;
    let sensors_client = Arc::new(InMemorySensorsClient::default());
    sensors_client.sensors.lock().unwrap().push(common::Sensor::new(
        Uuid::new_v4(),
        "zendesk",
        "support-poller",
        serde_json::json!({}),
    ));
    state.sensors_client = sensors_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(r#"<datalist id="sensor-names">"#));
    assert!(body.contains(r#"<option value="support-poller">"#));
    assert!(body.contains(r#"list="sensor-names""#));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/data").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.stats_client = Arc::new(FailingIngestionStatsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
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
async fn renders_the_subject_and_email_from_search_fields_prefilled() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?subject=printer&email_from=alice%40example.com")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(r#"value="printer""#));
    assert!(body.contains("alice@example.com"));
}

#[tokio::test]
async fn shows_a_next_link_when_there_are_more_results_but_no_previous_link_on_page_zero() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    *stats_client.has_more.lock().unwrap() = true;
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
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
async fn shows_a_previous_link_on_page_two_and_no_next_link_when_no_more_results() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?page=1")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Previous"));
    assert!(!body.contains("Next"));
    assert!(body.contains("Page 2"));
}

#[tokio::test]
async fn a_backend_failure_listing_saved_searches_does_not_break_the_page() {
    let (mut state, session_id) = state_with_session().await;
    state.saved_search_queries_client = Arc::new(
        crate::saved_search_queries_client::saved_search_queries_client_test::FailingSavedSearchQueriesClient,
    );

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn post_save_search_creates_a_bookmark_and_redirects() {
    let (state, session_id) = state_with_session().await;
    let saved_client = Arc::new(
        crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default(),
    );
    let mut state = state;
    state.saved_search_queries_client = saved_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/data/saved-searches")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("name=urgent%20tickets&q=urgent&connector_id=&source_type=&subject=&email_from=&attachment_filename="))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let saved = saved_client.queries.lock().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "urgent tickets");
}

#[tokio::test]
async fn saved_searches_render_as_links_on_the_page() {
    let (mut state, session_id) = state_with_session().await;
    let saved_client = Arc::new(
        crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default(),
    );
    saved_client
        .create(
            state.session_store.get(&session_id).await.unwrap().tenant_id,
            "urgent tickets",
            serde_json::json!({"q": "urgent", "connector_id": "", "source_type": "", "subject": "", "email_from": "", "attachment_filename": "", "page": 0}),
        )
        .await
        .unwrap();
    state.saved_search_queries_client = saved_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("urgent tickets"));
    assert!(body.contains("q=urgent"));
}

#[tokio::test]
async fn post_delete_saved_search_removes_it_and_redirects() {
    let (mut state, session_id) = state_with_session().await;
    let saved_client = Arc::new(
        crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default(),
    );
    let tenant_id = state.session_store.get(&session_id).await.unwrap().tenant_id;
    let created = saved_client.create(tenant_id, "to remove", serde_json::json!({})).await.unwrap();
    state.saved_search_queries_client = saved_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/data/saved-searches/{}/delete", created.id))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(saved_client.list(tenant_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn reprocess_redirects_with_the_count_and_calls_the_client() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    *stats_client.reprocessed.lock().unwrap() = 42;
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/data/reprocess")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/data?reprocessed=42");
}

#[tokio::test]
async fn reprocess_rejects_a_viewer_role() {
    let (state, session_id) = state_with_session_role(common::Role::Viewer).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/data/reprocess")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reprocess_requires_a_session() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder().method("POST").uri("/data/reprocess").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn data_page_shows_the_reprocess_button_for_an_operator_and_confirmation_when_present() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?reprocessed=5")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Reprocess unnormalized records"));
    assert!(body.contains("Republished 5 unnormalized record"));
}

#[tokio::test]
async fn data_page_hides_the_reprocess_button_for_a_viewer() {
    let (state, session_id) = state_with_session_role(common::Role::Viewer).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("Reprocess unnormalized records"));
}

#[test]
fn parse_date_range_treats_from_as_start_of_day_and_to_as_end_of_day() {
    let (from, to) = parse_date_range("2026-07-15", "2026-07-20");
    assert_eq!(from.unwrap().to_rfc3339(), "2026-07-15T00:00:00+00:00");
    assert_eq!(to.unwrap().to_rfc3339(), "2026-07-20T23:59:59+00:00");
}

#[test]
fn parse_date_range_leaves_an_empty_or_unparseable_side_as_none() {
    let (from, to) = parse_date_range("", "not-a-date");
    assert!(from.is_none());
    assert!(to.is_none());
}
