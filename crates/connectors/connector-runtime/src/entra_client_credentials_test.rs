use super::*;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;

async fn spawn_stub_token_endpoint(status: axum::http::StatusCode) -> String {
    async fn ok_handler() -> axum::response::Response {
        axum::response::Json(serde_json::json!({
            "access_token": "stub-access-token",
            "token_type": "bearer",
            "expires_in": 3600
        }))
        .into_response()
    }
    async fn error_handler() -> axum::response::Response {
        axum::http::StatusCode::BAD_REQUEST.into_response()
    }

    let app = if status.is_success() {
        Router::new().route("/token", post(ok_handler))
    } else {
        Router::new().route("/token", post(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/token")
}

#[tokio::test]
async fn fetches_an_access_token_from_a_real_token_endpoint() {
    let token_url = spawn_stub_token_endpoint(axum::http::StatusCode::OK).await;

    let token = fetch_access_token(
        &token_url,
        "client-id",
        "client-secret",
        "https://example.com/.default",
        reqwest::Client::new(),
    )
    .await
    .unwrap();

    assert_eq!(token, "stub-access-token");
}

#[tokio::test]
async fn returns_token_request_error_when_the_endpoint_rejects_the_request() {
    let token_url = spawn_stub_token_endpoint(axum::http::StatusCode::BAD_REQUEST).await;

    let err = fetch_access_token(
        &token_url,
        "client-id",
        "client-secret",
        "scope",
        reqwest::Client::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(err, EntraAuthError::TokenRequest(_)));
}

#[tokio::test]
async fn returns_config_error_for_an_invalid_token_url() {
    let err = fetch_access_token(
        "not a url",
        "client-id",
        "client-secret",
        "scope",
        reqwest::Client::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, EntraAuthError::Config(_)));
}

#[tokio::test]
async fn the_token_request_actually_goes_through_the_provided_client_not_a_default_one() {
    // Proves the token fetch is not silently falling back to oauth2's own internal default
    // client: build a client proxied through a deliberately-invalid proxy URL and confirm the
    // request fails the way a misconfigured proxy would, rather than succeeding via some other
    // client oauth2 built internally.
    let token_url = spawn_stub_token_endpoint(axum::http::StatusCode::OK).await;
    let broken_proxy_client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
        .build()
        .unwrap();

    let err =
        fetch_access_token(&token_url, "client-id", "client-secret", "scope", broken_proxy_client)
            .await
            .unwrap_err();

    assert!(matches!(err, EntraAuthError::TokenRequest(_)));
}
