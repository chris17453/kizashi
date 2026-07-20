//! Auth/header-gating tests for `policy_handlers.rs` — split out of `policy_handlers_test.rs`
//! (CLAUDE.md §0.6/§2: split by responsibility rather than let a single test file grow past 500
//! lines). Reuses the router/state/request helpers defined there via `pub(crate)` visibility.

use super::policy_handlers_test::{
    default_state, router, sample_policy, send_raw, TEST_INTERNAL_SECRET,
};
use axum::http::StatusCode;
use uuid::Uuid;

#[tokio::test]
async fn create_policy_requires_role_header() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send_raw(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
        ],
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_policy_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send_raw(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "viewer".to_string()),
        ],
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_policy_allows_an_operator_role() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send_raw(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "operator".to_string()),
            ("x-username", "test-user".to_string()),
        ],
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_policy_requires_username_header() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send_raw(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "operator".to_string()),
        ],
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_policy_requires_username_header() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send_raw(
        router(default_state()),
        "PUT",
        format!("/v1/retention-policies/{}", policy.id),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "operator".to_string()),
        ],
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_policy_requires_role_header() {
    let tenant_id = Uuid::new_v4();
    let policy_id = Uuid::new_v4();
    let response = send_raw(
        router(default_state()),
        "DELETE",
        format!("/v1/retention-policies/{policy_id}"),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
        ],
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_policy_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let policy_id = Uuid::new_v4();
    let response = send_raw(
        router(default_state()),
        "DELETE",
        format!("/v1/retention-policies/{policy_id}"),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "viewer".to_string()),
        ],
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_policy_requires_username_header() {
    let tenant_id = Uuid::new_v4();
    let policy_id = Uuid::new_v4();
    let response = send_raw(
        router(default_state()),
        "DELETE",
        format!("/v1/retention-policies/{policy_id}"),
        &[
            ("x-internal-secret", TEST_INTERNAL_SECRET.to_string()),
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "operator".to_string()),
        ],
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Regression test for the security audit finding: a caller with valid tenant/role/username
/// headers but no `X-Internal-Secret` must still be rejected, since docker-compose publishes
/// this service's port directly and any network caller could otherwise forge those headers.
#[tokio::test]
async fn list_policies_rejects_missing_internal_secret_even_with_valid_headers() {
    let tenant_id = Uuid::new_v4();
    let response = send_raw(
        router(default_state()),
        "GET",
        "/v1/retention-policies".to_string(),
        &[
            ("x-tenant-id", tenant_id.to_string()),
            ("x-role", "admin".to_string()),
            ("x-username", "test-user".to_string()),
        ],
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// `/healthz` must stay reachable with zero headers — it's the plain liveness/readiness probe,
/// not a caller-authenticated route, so the internal-secret gate must not touch it.
#[tokio::test]
async fn healthz_works_with_zero_headers() {
    let response =
        send_raw(router(default_state()), "GET", "/healthz".to_string(), &[], None).await;
    assert_eq!(response.status(), StatusCode::OK);
}
