use super::*;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::Router;
use oauth2::http::{header, HeaderValue, Method};

async fn spawn_token_endpoint() -> String {
    let app = Router::new().route(
        "/token",
        post(|headers: HeaderMap, body: Bytes| async move {
            let content_type =
                headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            let echoed = format!("{content_type}|{}", String::from_utf8_lossy(&body));
            (StatusCode::CREATED, [("x-echo", "yes")], echoed)
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn token_request(base: &str) -> HttpRequest {
    let mut headers = oauth2::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    HttpRequest {
        url: format!("{base}/token").parse().unwrap(),
        method: Method::POST,
        headers,
        body: b"grant_type=client_credentials".to_vec(),
    }
}

#[tokio::test]
async fn forwards_method_headers_and_body_and_returns_status_headers_and_body() {
    let base = spawn_token_endpoint().await;
    let client = reqwest::Client::new();

    let response = execute_oauth2_request(&client, token_request(&base)).await.unwrap();

    assert_eq!(response.status_code.as_u16(), 201);
    assert_eq!(response.headers.get("x-echo").unwrap(), "yes");
    assert_eq!(
        String::from_utf8(response.body).unwrap(),
        "application/x-www-form-urlencoded|grant_type=client_credentials"
    );
}

#[tokio::test]
async fn surfaces_transport_failures_as_an_error_instead_of_panicking() {
    let client = reqwest::Client::new();
    // Port 9 (discard) on localhost is not listening in test environments.
    let result = execute_oauth2_request(&client, token_request("http://127.0.0.1:9")).await;

    assert!(matches!(result, Err(OAuth2HttpError::Transport(_))));
}
