//! Router-level regression tests proving the `X-Internal-Secret` gate (`internal_secret.rs`)
//! covers the routes it's supposed to (`branding_handler`/`user_handlers`, which trust
//! `X-Role`/`X-Tenant-Id`/`X-Username`) and does NOT cover pre-session login — see
//! `internal_secret_test.rs` for unit coverage of the middleware and the gate applied across
//! every protected route, and `branding_handler_test.rs`/`user_handlers_test.rs` for handler
//! logic (those exercise the handlers directly, not through `build_router`, so they're
//! unaffected by this gate).

use crate::user_handlers::user_handlers_test::default_state;
use crate::{build_router, AuthState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SECRET: &str = "test-internal-secret";

fn test_state() -> AuthState {
    default_state()
}

#[tokio::test]
async fn put_branding_without_internal_secret_is_rejected() {
    let tenant_id = Uuid::new_v4();
    let app = build_router(test_state(), TEST_SECRET.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-username", "alice")
                .body(Body::from(
                    serde_json::json!({
                        "product_name": "Acme",
                        "logo_url": null,
                        "accent_color": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Blocked at the gate, before `put_branding` ever runs — the valid role/tenant/username
    // headers alone are no longer enough now that the port is directly reachable.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"invalid internal secret");
}

#[tokio::test]
async fn local_login_still_works_with_zero_internal_secret_header() {
    let app = build_router(test_state(), TEST_SECRET.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "tenant_name": "nonexistent",
                        "username": "nobody",
                        "password": "wrong"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // 401 comes from `local_login`'s own "unknown workspace" business logic, not from the
    // internal-secret gate — proving login isn't gated even though both paths can return 401.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "invalid workspace, username, or password");
}
