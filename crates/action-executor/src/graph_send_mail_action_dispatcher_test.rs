use super::*;
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use common::{ActionType, EventStatus};
use serde_json::json;
use uuid::Uuid;

fn sample_event() -> Event {
    Event {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        event_type: "sentiment".to_string(),
        source_connector_ids: vec![],
        entity_ref: "cust-1".to_string(),
        group_key: "cust-1".to_string(),
        payload: json!({}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: EventStatus::New,
        record_ids: vec![],
    }
}

async fn spawn_stub_token_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(json!({"access_token": "fake-token", "token_type": "bearer", "expires_in": 3600}))
    }
    let app = Router::new().route("/token", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn spawn_stub_graph_server(status: axum::http::StatusCode) -> String {
    async fn ok_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::ACCEPTED
    }
    async fn error_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    }
    let app = if status.is_success() {
        Router::new().route("/users/:id/sendMail", post(ok_handler))
    } else {
        Router::new().route("/users/:id/sendMail", post(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn base_config(token_url: &str, graph_url: &str) -> serde_json::Value {
    json!({
        "graph_token_url": token_url,
        "graph_client_id": "client-id",
        "graph_client_secret": "client-secret",
        "graph_from_user_id": "alerts@example.com",
        "graph_base_url": graph_url,
        "to": "recipient@example.com",
        "subject": "Test alert",
    })
}

#[tokio::test]
async fn dispatch_sends_mail_via_a_real_graph_style_http_request_and_succeeds() {
    let token_url = spawn_stub_token_server().await;
    let graph_url = spawn_stub_graph_server(axum::http::StatusCode::ACCEPTED).await;
    let dispatcher = GraphSendMailActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: base_config(&format!("{token_url}/token"), &graph_url),
    };

    let result = dispatcher.dispatch(&action, &sample_event()).await.unwrap();
    assert_eq!(result["sent_to"], json!(["recipient@example.com"]));
}

#[tokio::test]
async fn dispatch_returns_rejected_when_graph_errors() {
    let token_url = spawn_stub_token_server().await;
    let graph_url = spawn_stub_graph_server(axum::http::StatusCode::INTERNAL_SERVER_ERROR).await;
    let dispatcher = GraphSendMailActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: base_config(&format!("{token_url}/token"), &graph_url),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::Rejected(500)));
}

#[tokio::test]
async fn dispatch_returns_unreachable_when_the_token_endpoint_is_down() {
    let dispatcher = GraphSendMailActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: base_config("http://127.0.0.1:1/token", "http://127.0.0.1:1"),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::Unreachable(_)));
}

#[tokio::test]
async fn dispatch_returns_invalid_config_when_graph_client_id_is_missing() {
    let dispatcher = GraphSendMailActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({"graph_token_url": "http://localhost", "to": "a@example.com"}),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::InvalidConfig(msg) if msg.contains("graph_client_id")));
}

#[test]
fn recipients_accepts_a_single_string_or_an_array() {
    assert_eq!(recipients(&json!({"to": "a@example.com"})).unwrap(), vec!["a@example.com"]);
    assert_eq!(
        recipients(&json!({"to": ["a@example.com", "b@example.com"]})).unwrap(),
        vec!["a@example.com", "b@example.com"]
    );
    assert!(recipients(&json!({})).is_err());
}
