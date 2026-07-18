use super::*;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;

async fn spawn_stub_server(
    items: serde_json::Value,
    status: axum::http::StatusCode,
    expected_token: Option<&'static str>,
) -> String {
    async fn handler(
        State((items, expected_token)): State<(serde_json::Value, Option<&'static str>)>,
        headers: HeaderMap,
    ) -> axum::response::Response {
        if let Some(expected) = expected_token {
            let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
            if auth != Some(&format!("Bearer {expected}")) {
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }
        }
        Json(items).into_response()
    }
    async fn error_handler(State(status): State<axum::http::StatusCode>) -> axum::http::StatusCode {
        status
    }

    let app = if status.is_success() {
        Router::new().route("/items", get(handler)).with_state((items, expected_token))
    } else {
        Router::new().route("/items", get(error_handler)).with_state(status)
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/items")
}

#[tokio::test]
async fn polls_a_real_server_and_maps_each_array_item_to_a_raw_record() {
    let tenant_id = uuid::Uuid::new_v4();
    let items = serde_json::json!([{"a": 1}, {"b": 2}]);
    let url = spawn_stub_server(items.clone(), axum::http::StatusCode::OK, None).await;
    let connector = GenericConnector::new("generic", reqwest::Client::new(), url, None);

    let records = connector.poll(tenant_id).await.unwrap();

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].tenant_id, tenant_id);
    assert_eq!(records[0].connector_id, "generic");
    assert_eq!(records[0].raw_payload, items[0]);
    assert_eq!(records[1].raw_payload, items[1]);
}

#[tokio::test]
async fn sends_the_configured_bearer_token() {
    let tenant_id = uuid::Uuid::new_v4();
    let url =
        spawn_stub_server(serde_json::json!([]), axum::http::StatusCode::OK, Some("secret-token"))
            .await;
    let connector = GenericConnector::new(
        "generic",
        reqwest::Client::new(),
        url,
        Some("secret-token".to_string()),
    );

    let records = connector.poll(tenant_id).await.unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn missing_bearer_token_is_rejected_as_auth_failure() {
    let tenant_id = uuid::Uuid::new_v4();
    let url =
        spawn_stub_server(serde_json::json!([]), axum::http::StatusCode::OK, Some("secret-token"))
            .await;
    let connector = GenericConnector::new("generic", reqwest::Client::new(), url, None);

    let err = connector.poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, ConnectorError::AuthFailed(_)));
}

#[tokio::test]
async fn server_error_is_reported_as_source_unavailable() {
    let tenant_id = uuid::Uuid::new_v4();
    let url = spawn_stub_server(
        serde_json::json!([]),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        None,
    )
    .await;
    let connector = GenericConnector::new("generic", reqwest::Client::new(), url, None);

    let err = connector.poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, ConnectorError::SourceUnavailable(_)));
}

#[tokio::test]
async fn unreachable_server_is_reported_as_source_unavailable() {
    let tenant_id = uuid::Uuid::new_v4();
    let connector =
        GenericConnector::new("generic", reqwest::Client::new(), "http://127.0.0.1:1", None);

    let err = connector.poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, ConnectorError::SourceUnavailable(_)));
}
