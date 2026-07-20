use super::*;
use crate::local_login_handler::AuthState;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use crate::tenant_branding_repository::tenant_branding_repository_test::{
    FailingTenantBrandingRepository, InMemoryTenantBrandingRepository,
};
use crate::tenant_repository::tenant_repository_test::InMemoryTenantRepository;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/tenants/:name/branding", get(get_branding).put(put_branding))
        .route("/v1/tenants/id/:id/branding", get(get_branding_by_id))
        .with_state(state)
}

fn default_state() -> AuthState {
    AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(InMemoryTenantRepository::default()),
        tenant_branding_repository: Arc::new(InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(
            crate::session_client::session_client_test::InMemorySessionClient::default(),
        ),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
    }
}

#[tokio::test]
async fn get_branding_returns_null_fields_when_nothing_is_configured() {
    let branding_repo = Arc::new(InMemoryTenantBrandingRepository::default());
    branding_repo.branding.lock().unwrap().insert(
        "acme".to_string(),
        crate::tenant_branding_repository::TenantBranding {
            product_name: None,
            logo_url: None,
            accent_color: None,
        },
    );
    let state = AuthState { tenant_branding_repository: branding_repo, ..default_state() };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/tenants/acme/branding").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["product_name"], serde_json::Value::Null);
}

#[tokio::test]
async fn get_branding_returns_404_for_an_unknown_workspace() {
    let state = default_state();

    let response = router(state)
        .oneshot(
            Request::builder().uri("/v1/tenants/nonexistent/branding").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_branding_returns_500_when_the_backend_fails() {
    let state = AuthState {
        tenant_branding_repository: Arc::new(FailingTenantBrandingRepository),
        ..default_state()
    };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/tenants/acme/branding").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn get_branding_by_id_returns_the_stored_values() {
    let branding_repo = Arc::new(InMemoryTenantBrandingRepository::default());
    let tenant_id = Uuid::new_v4();
    branding_repo.branding.lock().unwrap().insert(
        tenant_id.to_string(),
        crate::tenant_branding_repository::TenantBranding {
            product_name: Some("Acme Signals".to_string()),
            logo_url: None,
            accent_color: None,
        },
    );
    let state = AuthState { tenant_branding_repository: branding_repo, ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/tenants/id/{tenant_id}/branding"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["product_name"], "Acme Signals");
}

#[tokio::test]
async fn get_branding_by_id_returns_404_for_an_unknown_id() {
    let state = default_state();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/tenants/id/{}/branding", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn put_branding_requires_admin_role() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "operator")
                .header("x-username", "alice")
                .body(Body::from(
                    serde_json::json!({"product_name": "Acme", "logo_url": null, "accent_color": null})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn put_branding_requires_a_username_header() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .body(Body::from(
                    serde_json::json!({"product_name": "Acme", "logo_url": null, "accent_color": null})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_branding_rejects_an_invalid_accent_color() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-username", "alice")
                .body(Body::from(
                    serde_json::json!({"product_name": null, "logo_url": null, "accent_color": "red; } body { display:none"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_branding_accepts_a_valid_hex_color() {
    let branding_repo = Arc::new(InMemoryTenantBrandingRepository::default());
    let state = AuthState { tenant_branding_repository: branding_repo, ..default_state() };
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-username", "alice")
                .body(Body::from(
                    serde_json::json!({"product_name": null, "logo_url": null, "accent_color": "#ff6600"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn put_branding_as_admin_saves_and_records_the_real_actor() {
    let branding_repo = Arc::new(InMemoryTenantBrandingRepository::default());
    let state = AuthState { tenant_branding_repository: branding_repo.clone(), ..default_state() };
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-username", "alice@acme.example.com")
                .body(Body::from(
                    serde_json::json!({"product_name": "Acme Signals", "logo_url": null, "accent_color": "#ff6600"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        *branding_repo.last_update_actor.lock().unwrap(),
        Some("alice@acme.example.com".to_string())
    );
}

#[tokio::test]
async fn put_branding_requires_a_tenant_id_header() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .header("x-username", "alice")
                .body(Body::from(
                    serde_json::json!({"product_name": "Acme", "logo_url": null, "accent_color": null})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_branding_rejects_a_caller_from_a_different_tenant() {
    // An Admin authenticated as tenant A must not be able to overwrite tenant B's branding just
    // by knowing/guessing tenant B's id in the path -- this was a real cross-tenant write bug:
    // the handler never checked the path id against the caller's own X-Tenant-Id.
    let branding_repo = Arc::new(InMemoryTenantBrandingRepository::default());
    let state = AuthState { tenant_branding_repository: branding_repo.clone(), ..default_state() };
    let victim_tenant_id = Uuid::new_v4();
    let attacker_tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/tenants/{victim_tenant_id}/branding"))
                .header("content-type", "application/json")
                .header("x-role", "admin")
                .header("x-tenant-id", attacker_tenant_id.to_string())
                .header("x-username", "attacker-admin")
                .body(Body::from(
                    serde_json::json!({"product_name": "Defaced", "logo_url": null, "accent_color": null})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(branding_repo.branding.lock().unwrap().get(&victim_tenant_id.to_string()), None);
}
