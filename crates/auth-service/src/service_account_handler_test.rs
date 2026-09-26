use super::*;
use crate::user_handlers::user_handlers_test::default_state;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, get};
use axum::Router;
use tower::ServiceExt;
use uuid::Uuid;

fn router() -> Router {
    Router::new()
        .route("/v1/service-accounts", get(list_service_accounts).post(create_service_account))
        .route("/v1/service-accounts/:id", delete(revoke_service_account))
        .route("/v1/service-accounts/introspect", get(introspect_service_account))
        .with_state(default_state())
}

#[tokio::test]
async fn management_requires_admin_role() {
    let tenant_id = Uuid::new_v4();
    let response = router()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/service-accounts")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn introspection_requires_bearer_token() {
    let response = router()
        .oneshot(
            Request::builder().uri("/v1/service-accounts/introspect").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_rejects_blank_labels() {
    let tenant_id = Uuid::new_v4();
    let response = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/service-accounts")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("x-username", "root")
                .body(Body::from(r#"{"label":"   ","role":"operator"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
