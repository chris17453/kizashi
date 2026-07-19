use super::*;
use axum::extract::Json as JsonExtractor;
use axum::routing::post;
use axum::Router;
use common::{ActionType, EventStatus};
use serde_json::json;
use std::sync::Mutex;
use uuid::Uuid;

fn sample_event() -> Event {
    Event {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        event_type: "sentiment_drop".to_string(),
        source_connector_ids: vec![],
        entity_ref: "cust-42".to_string(),
        group_key: "cust-42".to_string(),
        payload: json!({"score": -0.8}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: EventStatus::New,
        record_ids: vec![],
    }
}

async fn spawn_capturing_stub() -> (String, std::sync::Arc<Mutex<Vec<serde_json::Value>>>) {
    let captured: std::sync::Arc<Mutex<Vec<serde_json::Value>>> = Default::default();
    let captured_clone = captured.clone();
    let handler = move |JsonExtractor(body): JsonExtractor<serde_json::Value>| {
        let captured = captured_clone.clone();
        async move {
            captured.lock().unwrap().push(body);
            axum::http::StatusCode::OK
        }
    };
    let app = Router::new().route("/hook", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/hook"), captured)
}

#[tokio::test]
async fn dispatch_sends_a_real_teams_message_card_shape() {
    let (url, captured) = spawn_capturing_stub().await;
    let dispatcher = TeamsAlertActionDispatcher::new(None);
    let action = ActionRef {
        action_type: ActionType::TeamsAlert,
        config: json!({"url": url, "title": "Sentiment Alert"}),
    };
    let event = sample_event();

    dispatcher.dispatch(&action, &event).await.unwrap();

    let bodies = captured.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    let body = &bodies[0];
    // The exact shape a real Teams incoming webhook validates and requires.
    assert_eq!(body["@type"], "MessageCard");
    assert_eq!(body["@context"], "http://schema.org/extensions");
    assert_eq!(body["title"], "Sentiment Alert");
    assert!(body["summary"].as_str().unwrap().contains("sentiment_drop"));
    let facts = body["sections"][0]["facts"].as_array().unwrap();
    assert!(facts.iter().any(|f| f["name"] == "Entity" && f["value"] == "cust-42"));
}

#[tokio::test]
async fn dispatch_defaults_the_title_when_not_configured() {
    let (url, captured) = spawn_capturing_stub().await;
    let dispatcher = TeamsAlertActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::TeamsAlert, config: json!({"url": url}) };

    dispatcher.dispatch(&action, &sample_event()).await.unwrap();

    let body = &captured.lock().unwrap()[0];
    assert_eq!(body["title"], "Kizashi alert");
}

#[tokio::test]
async fn dispatch_returns_missing_url_when_config_has_no_url() {
    let dispatcher = TeamsAlertActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::TeamsAlert, config: json!({}) };

    let result = dispatcher.dispatch(&action, &sample_event()).await;
    assert!(matches!(result, Err(DispatchError::MissingUrl)));
}

#[tokio::test]
async fn dispatch_returns_rejected_when_the_server_errors() {
    async fn error_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::BAD_REQUEST
    }
    let app = Router::new().route("/hook", post(error_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let url = format!("http://{addr}/hook");

    let dispatcher = TeamsAlertActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::TeamsAlert, config: json!({"url": url}) };

    let result = dispatcher.dispatch(&action, &sample_event()).await;
    assert!(matches!(result, Err(DispatchError::Rejected(400))));
}

#[tokio::test]
async fn dispatch_returns_unreachable_when_the_server_is_down() {
    let dispatcher = TeamsAlertActionDispatcher::new(None);
    let action = ActionRef {
        action_type: ActionType::TeamsAlert,
        config: json!({"url": "http://127.0.0.1:1/hook"}),
    };

    let result = dispatcher.dispatch(&action, &sample_event()).await;
    assert!(matches!(result, Err(DispatchError::Unreachable(_))));
}
