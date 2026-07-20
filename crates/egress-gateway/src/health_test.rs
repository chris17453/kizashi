use super::*;
use crate::allowlist::allowlist_test::{FailingAllowlistRepository, InMemoryAllowlistRepository};
use crate::allowlist_audit_log::allowlist_audit_log_test::InMemoryAllowlistAuditLogReader;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

const TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";

fn state() -> AdminState {
    AdminState {
        allowlist_repository: Arc::new(InMemoryAllowlistRepository::default()),
        allowlist_audit_log_reader: Arc::new(InMemoryAllowlistAuditLogReader::default()),
    }
}

#[tokio::test]
async fn healthz_returns_200() {
    let app = build_router(state());
    let response =
        app.oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_allowlist_returns_empty_when_none_configured() {
    let app = build_router(state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let domains: Vec<String> = serde_json::from_slice(&bytes).unwrap();
    assert!(domains.is_empty());
}

#[tokio::test]
async fn get_allowlist_requires_tenant_header() {
    let app = build_router(state());
    let response = app
        .oneshot(Request::builder().uri("/v1/allowlist").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_then_get_allowlist_round_trips() {
    let app_state = state();
    let body = serde_json::json!({"domains": ["zendesk.com"]});

    let put_response = build_router(app_state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .header("x-role", "operator")
                .header("x-username", "operator@example.com")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = build_router(app_state)
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(get_response.into_body(), usize::MAX).await.unwrap();
    let domains: Vec<String> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(domains, vec!["zendesk.com".to_string()]);
}

#[tokio::test]
async fn get_allowlist_returns_500_on_backend_failure() {
    let app_state = AdminState {
        allowlist_repository: Arc::new(FailingAllowlistRepository),
        allowlist_audit_log_reader: Arc::new(InMemoryAllowlistAuditLogReader::default()),
    };
    let response = build_router(app_state)
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// --- RBAC: PUT /v1/allowlist controls a tenant's egress SSRF/exfiltration containment
// boundary (ADR-0021) and must require at least Operator, the same as every other
// config-mutating write endpoint in the platform (ADR-0016).

#[tokio::test]
async fn put_allowlist_requires_role_header() {
    let app = build_router(state());
    let body = serde_json::json!({"domains": ["zendesk.com"]});
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_allowlist_rejects_a_viewer_role() {
    let app = build_router(state());
    let body = serde_json::json!({"domains": ["zendesk.com"]});
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .header("x-role", "viewer")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn put_allowlist_allows_an_operator_role() {
    let app = build_router(state());
    let body = serde_json::json!({"domains": ["zendesk.com"]});
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .header("x-role", "operator")
                .header("x-username", "operator@example.com")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn put_allowlist_requires_username_header() {
    let app = build_router(state());
    let body = serde_json::json!({"domains": ["zendesk.com"]});
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .header("x-role", "operator")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_allowlist_never_requires_a_role_header_read_only_stays_unrestricted() {
    // GET is not a config-mutation, so it deliberately keeps its existing behavior
    // (tenant-scoped, no role check) — only the write path changes here.
    let app = build_router(state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", TENANT_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// --- GET /v1/audit-log/:entity_id (ADR-0097)

#[tokio::test]
async fn get_audit_log_returns_entries_for_the_entity() {
    let audit_log_reader = Arc::new(InMemoryAllowlistAuditLogReader::default());
    let tenant_uuid: uuid::Uuid = TENANT_ID.parse().unwrap();
    audit_log_reader.entries.lock().unwrap().push(crate::AllowlistAuditLogEntry {
        id: uuid::Uuid::new_v4(),
        tenant_id: tenant_uuid,
        entity_type: "egress_allowlist".to_string(),
        entity_id: tenant_uuid,
        change_type: crate::AllowlistChangeType::Updated,
        actor: "operator@example.com".to_string(),
        before: None,
        after: serde_json::json!({"domains": ["zendesk.com"]}),
        changed_at: chrono::Utc::now(),
    });
    let app_state = AdminState {
        allowlist_repository: Arc::new(InMemoryAllowlistRepository::default()),
        allowlist_audit_log_reader: audit_log_reader,
    };

    let response = build_router(app_state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/audit-log/{TENANT_ID}"))
                .header("x-tenant-id", TENANT_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<crate::AllowlistAuditLogEntry> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].actor, "operator@example.com");
}
