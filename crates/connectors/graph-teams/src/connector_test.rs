use super::*;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;

async fn spawn_stub_token_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(
            serde_json::json!({"access_token": "stub-token", "token_type": "bearer", "expires_in": 3600}),
        )
    }
    let app = Router::new().route("/token", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/token")
}

async fn spawn_stub_graph_server(
    status: axum::http::StatusCode,
    messages: serde_json::Value,
) -> String {
    async fn handler(
        State(messages): State<serde_json::Value>,
        headers: HeaderMap,
    ) -> axum::response::Response {
        if headers.get("authorization") != Some(&"Bearer stub-token".parse().unwrap()) {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({"value": messages})).into_response()
    }
    async fn error_handler(State(status): State<axum::http::StatusCode>) -> axum::http::StatusCode {
        status
    }

    let app = if status.is_success() {
        Router::new()
            .route("/teams/:team_id/channels/:channel_id/messages", get(handler))
            .with_state(messages)
    } else {
        Router::new()
            .route("/teams/:team_id/channels/:channel_id/messages", get(error_handler))
            .with_state(status)
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn connector(graph_url: String, token_url: String) -> GraphTeamsConnector {
    GraphTeamsConnector::new(
        "graph-teams",
        reqwest::Client::new(),
        graph_url,
        token_url,
        "client-id",
        "client-secret",
        "team-1",
        "channel-1",
    )
}

#[tokio::test]
async fn polls_a_real_graph_server_and_maps_messages_to_raw_records() {
    let tenant_id = uuid::Uuid::new_v4();
    let messages = serde_json::json!([{"id": "1", "body": {"content": "hi team"}}]);
    let token_url = spawn_stub_token_server().await;
    let graph_url = spawn_stub_graph_server(axum::http::StatusCode::OK, messages.clone()).await;

    let records = connector(graph_url, token_url).poll(tenant_id).await.unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].tenant_id, tenant_id);
    assert_eq!(records[0].connector_id, "graph-teams");
    assert_eq!(records[0].raw_payload, messages[0]);
}

#[tokio::test]
async fn graph_unauthorized_response_is_reported_as_auth_failed() {
    let tenant_id = uuid::Uuid::new_v4();
    let token_url = spawn_stub_token_server().await;
    let graph_url =
        spawn_stub_graph_server(axum::http::StatusCode::UNAUTHORIZED, serde_json::json!([])).await;

    let err = connector(graph_url, token_url).poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, ConnectorError::AuthFailed(_)));
}

#[tokio::test]
async fn unreachable_token_endpoint_is_reported_as_auth_failed() {
    let err = connector("http://127.0.0.1:1".to_string(), "http://127.0.0.1:1".to_string())
        .poll(uuid::Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::AuthFailed(_)));
}
