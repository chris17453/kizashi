use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::InMemoryBacklogClient;
use crate::backup_status_client::backup_status_client_test::{
    FailingBackupStatusClient, InMemoryBackupStatusClient,
};
use crate::backup_status_client::{BackupRun, BackupStatusClient};
use crate::branding_client::branding_client_test::InMemoryBrandingClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::oidc_client::oidc_client_test::InMemoryOidcClient;
use crate::pending_oidc_flow::InMemoryPendingOidcFlowStore;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
use crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::users_client::users_client_test::InMemoryUsersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/security/backups", get(get_backups)).with_state(state)
}

fn sample_session(role: Role) -> Session {
    Session {
        bearer_token: "tok".to_string(),
        tenant_id: Uuid::new_v4(),
        username: "alice".to_string(),
        role,
        created_at: chrono::Utc::now(),
    }
}

async fn state_with(
    session_store: InMemorySessionStore,
    backup_status_client: Arc<dyn BackupStatusClient>,
) -> AppState {
    AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(InMemoryBrandingClient::default()),
        oidc_client: Arc::new(InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        sensors_client: Arc::new(InMemorySensorsClient::default()),
        api_keys_client: Arc::new(InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(InMemoryBacklogClient::default()),
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        normalization_mappings_client: Arc::new(InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        ingestion_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        egress_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
        login_attempts_client: Arc::new(InMemoryLoginAttemptsClient::default()),
        backup_status_client,
    }
}

async fn get_page(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    get_page_at(state, session_id, "/security/backups").await
}

async fn get_page_at(state: AppState, session_id: &str, uri: &str) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_can_view_backup_status() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Admin)).await;
    let client = InMemoryBackupStatusClient {
        runs: std::sync::Mutex::new(vec![BackupRun {
            started_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            status: "success".to_string(),
            target: "postgres/test.dump".to_string(),
            size_bytes: Some(4096),
            error: None,
        }]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("postgres/test.dump"));
    assert!(body.contains("Success"));
}

#[tokio::test]
async fn shows_a_load_older_link_when_a_full_page_is_returned() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Admin)).await;
    let runs = (0..20)
        .map(|i| BackupRun {
            started_at: chrono::Utc::now() - chrono::Duration::days(i),
            completed_at: Some(chrono::Utc::now()),
            status: "success".to_string(),
            target: format!("postgres/{i}.dump"),
            size_bytes: Some(4096),
            error: None,
        })
        .collect();
    let client = InMemoryBackupStatusClient { runs: std::sync::Mutex::new(runs) };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("/security/backups?before="));
    // The cursor's rendered offset (`+00:00`) must be percent-encoded, not left as a raw `+` --
    // an unencoded `+` in a query string is decoded as a space by `serde_urlencoded`/the
    // application/x-www-form-urlencoded convention axum's Query extractor follows, corrupting
    // the timestamp on click. Assert against the actual link text, not the whole body, since
    // an unrelated `+` could appear elsewhere on the page.
    let link_start = body.find("/security/backups?before=").unwrap();
    let link_end = body[link_start..].find('"').unwrap() + link_start;
    assert!(
        !body[link_start..link_end].contains('+'),
        "Load older link must not contain a raw '+': {}",
        &body[link_start..link_end]
    );
}

#[tokio::test]
async fn no_load_older_link_when_fewer_than_a_full_page_is_returned() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Admin)).await;
    let client = InMemoryBackupStatusClient {
        runs: std::sync::Mutex::new(vec![BackupRun {
            started_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            status: "success".to_string(),
            target: "postgres/test.dump".to_string(),
            size_bytes: Some(4096),
            error: None,
        }]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("Load older"));
}

#[tokio::test]
async fn before_query_param_is_forwarded_to_the_client() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Admin)).await;
    let cutoff: chrono::DateTime<chrono::Utc> = "2026-07-18T00:00:00Z".parse().unwrap();
    let client = InMemoryBackupStatusClient {
        runs: std::sync::Mutex::new(vec![
            BackupRun {
                started_at: cutoff - chrono::Duration::days(1),
                completed_at: Some(cutoff),
                status: "success".to_string(),
                target: "postgres/older.dump".to_string(),
                size_bytes: Some(4096),
                error: None,
            },
            BackupRun {
                started_at: cutoff,
                completed_at: Some(cutoff),
                status: "success".to_string(),
                target: "postgres/newer.dump".to_string(),
                size_bytes: Some(4096),
                error: None,
            },
        ]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response =
        get_page_at(state, &session_id, "/security/backups?before=2026-07-18T00:00:00Z").await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("postgres/older.dump"));
    assert!(!body.contains("postgres/newer.dump"));
}

#[tokio::test]
async fn shows_an_empty_state_with_no_runs() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Admin)).await;
    let state = state_with(store, Arc::new(InMemoryBackupStatusClient::default())).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No backup runs recorded"));
}

#[tokio::test]
async fn shows_an_error_when_the_backend_is_unreachable() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Admin)).await;
    let state = state_with(store, Arc::new(FailingBackupStatusClient)).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("simulated failure"));
}

#[tokio::test]
async fn non_admin_gets_forbidden() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Role::Operator)).await;
    let state = state_with(store, Arc::new(InMemoryBackupStatusClient::default())).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let state = state_with(
        InMemorySessionStore::default(),
        Arc::new(InMemoryBackupStatusClient::default()),
    )
    .await;

    let response = router(state)
        .oneshot(Request::builder().uri("/security/backups").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
