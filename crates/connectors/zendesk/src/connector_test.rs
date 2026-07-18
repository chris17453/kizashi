use super::*;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;

#[derive(Clone)]
struct StubState {
    expected_start_time: String,
    tickets: serde_json::Value,
}

async fn spawn_stub_server(status: axum::http::StatusCode, tickets: serde_json::Value) -> String {
    async fn handler(
        State(state): State<StubState>,
        headers: HeaderMap,
        Query(params): Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        if headers.get("authorization").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        if params.get("start_time") != Some(&state.expected_start_time) {
            return axum::http::StatusCode::BAD_REQUEST.into_response();
        }
        Json(serde_json::json!({"tickets": state.tickets, "end_time": 0, "count": 0}))
            .into_response()
    }
    async fn error_handler(State(status): State<axum::http::StatusCode>) -> axum::http::StatusCode {
        status
    }

    let app = if status.is_success() {
        Router::new()
            .route("/api/v2/incremental/tickets.json", get(handler))
            .with_state(StubState { expected_start_time: "1000".to_string(), tickets })
    } else {
        Router::new()
            .route("/api/v2/incremental/tickets.json", get(error_handler))
            .with_state(status)
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn connector(base_url: String) -> ZendeskConnector {
    ZendeskConnector::new(
        "zendesk",
        reqwest::Client::new(),
        base_url,
        "agent@example.com",
        "api-token",
        1000,
    )
}

#[tokio::test]
async fn polls_a_real_server_and_maps_tickets_to_raw_records() {
    let tenant_id = uuid::Uuid::new_v4();
    let tickets = serde_json::json!([{"id": 1, "subject": "help"}, {"id": 2, "subject": "urgent"}]);
    let base_url = spawn_stub_server(axum::http::StatusCode::OK, tickets.clone()).await;

    let records = connector(base_url).poll(tenant_id).await.unwrap();

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].tenant_id, tenant_id);
    assert_eq!(records[0].connector_id, "zendesk");
    assert_eq!(records[0].raw_payload, tickets[0]);
}

#[tokio::test]
async fn unauthorized_response_is_reported_as_auth_failed() {
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        spawn_stub_server(axum::http::StatusCode::UNAUTHORIZED, serde_json::json!([])).await;

    let err = connector(base_url).poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, ConnectorError::AuthFailed(_)));
}

#[tokio::test]
async fn rate_limited_response_reports_retry_after() {
    async fn rate_limited_handler() -> axum::response::Response {
        (axum::http::StatusCode::TOO_MANY_REQUESTS, [("retry-after", "42")]).into_response()
    }
    let app = Router::new().route("/api/v2/incremental/tickets.json", get(rate_limited_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base_url = format!("http://{addr}");

    let err = connector(base_url).poll(uuid::Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, ConnectorError::RateLimited { retry_after_secs: 42 }));
}

#[tokio::test]
async fn server_error_is_reported_as_source_unavailable() {
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        spawn_stub_server(axum::http::StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!([]))
            .await;

    let err = connector(base_url).poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, ConnectorError::SourceUnavailable(_)));
}
