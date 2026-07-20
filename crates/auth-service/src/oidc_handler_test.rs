use super::*;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use crate::oidc_client::oidc_client_test::{FailingOidcClient, InMemoryOidcClient};
use crate::session_client::session_client_test::{FailingSessionClient, InMemorySessionClient};
use crate::tenant_repository::tenant_repository_test::InMemoryTenantRepository;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/auth/oidc/:provider/authorize", get(authorize))
        .route("/v1/auth/oidc/:provider/callback", post(callback))
        .with_state(state)
}

fn state_with_provider(
    provider: &str,
    client: Arc<dyn OidcClient>,
    session_client: Arc<dyn crate::session_client::SessionClient>,
) -> AuthState {
    let mut oidc_clients: OidcClients = std::collections::HashMap::new();
    oidc_clients.insert(provider.to_string(), client);
    AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", Uuid::new_v4())),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client,
        oidc_clients,
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
    }
}

#[tokio::test]
async fn authorize_returns_url_and_verifier_for_a_known_provider() {
    let state = state_with_provider(
        "entra",
        Arc::new(InMemoryOidcClient::default()),
        Arc::new(InMemorySessionClient::default()),
    );

    let response = router(state)
        .oneshot(
            Request::builder().uri("/v1/auth/oidc/entra/authorize").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: AuthorizeResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(!parsed.authorization_url.is_empty());
    assert!(!parsed.code_verifier.is_empty());
}

#[tokio::test]
async fn authorize_returns_404_for_an_unknown_provider() {
    let state = state_with_provider(
        "entra",
        Arc::new(InMemoryOidcClient::default()),
        Arc::new(InMemorySessionClient::default()),
    );

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/auth/oidc/nonexistent/authorize")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn authorize_returns_500_when_the_client_fails_to_build_the_request() {
    let state = state_with_provider(
        "entra",
        Arc::new(FailingOidcClient),
        Arc::new(InMemorySessionClient::default()),
    );

    let response = router(state)
        .oneshot(
            Request::builder().uri("/v1/auth/oidc/entra/authorize").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn callback_completes_exchange_and_mints_a_session() {
    let session_client = Arc::new(InMemorySessionClient::default());
    let state = state_with_provider(
        "entra",
        Arc::new(InMemoryOidcClient::default()),
        session_client.clone(),
    );

    let body = serde_json::json!({"code": "auth-code", "code_verifier": "verifier", "tenant_name": "acme"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/oidc/entra/callback")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(session_client.minted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn callback_returns_404_for_an_unknown_provider() {
    let state = state_with_provider(
        "entra",
        Arc::new(InMemoryOidcClient::default()),
        Arc::new(InMemorySessionClient::default()),
    );

    let body = serde_json::json!({"code": "auth-code", "code_verifier": "verifier", "tenant_name": "acme"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/oidc/nonexistent/callback")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn callback_returns_502_when_code_exchange_fails() {
    let state = state_with_provider(
        "entra",
        Arc::new(FailingOidcClient),
        Arc::new(InMemorySessionClient::default()),
    );

    let body = serde_json::json!({"code": "auth-code", "code_verifier": "verifier", "tenant_name": "acme"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/oidc/entra/callback")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn callback_returns_502_when_session_mint_fails() {
    let state = state_with_provider(
        "entra",
        Arc::new(InMemoryOidcClient::default()),
        Arc::new(FailingSessionClient),
    );

    let body = serde_json::json!({"code": "auth-code", "code_verifier": "verifier", "tenant_name": "acme"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/oidc/entra/callback")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn callback_returns_400_when_the_tenant_name_is_unknown() {
    let state = state_with_provider(
        "entra",
        Arc::new(InMemoryOidcClient::default()),
        Arc::new(InMemorySessionClient::default()),
    );

    let body = serde_json::json!({"code": "auth-code", "code_verifier": "verifier", "tenant_name": "nonexistent-workspace"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/oidc/entra/callback")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
