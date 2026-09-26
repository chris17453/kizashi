use super::*;
use axum::{routing::post, Json, Router};
use std::future::IntoFuture;
use std::net::SocketAddr;

#[tokio::test]
async fn http_client_returns_the_generated_summary() {
    let app = Router::new().route(
        "/v1/incident-brief",
        post(|| async { Json(serde_json::json!({ "summary": "bounded impact" })) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    let client = HttpIncidentBriefClient::new(reqwest::Client::new(), format!("http://{address}"));
    assert_eq!(
        client.generate(Uuid::new_v4(), serde_json::json!({"events": 2})).await.unwrap(),
        "bounded impact"
    );
}
