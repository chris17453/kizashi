use super::*;
use axum::routing::post;
use axum::Router;
use common::{ActionType, EventStatus};
use serde_json::json;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryActionDispatcher {
    pub dispatched: Mutex<Vec<ActionRef>>,
}

#[async_trait]
impl ActionDispatcher for InMemoryActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        _event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        self.dispatched.lock().unwrap().push(action.clone());
        Ok(json!({"dispatched": true}))
    }
}

pub struct FailingActionDispatcher;

#[async_trait]
impl ActionDispatcher for FailingActionDispatcher {
    async fn dispatch(
        &self,
        _action: &ActionRef,
        _event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        Err(DispatchError::Unreachable("simulated failure".to_string()))
    }
}

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

async fn spawn_stub_webhook(status: axum::http::StatusCode) -> String {
    async fn ok_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::OK
    }
    async fn error_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    }
    let app = if status.is_success() {
        Router::new().route("/hook", post(ok_handler))
    } else {
        Router::new().route("/hook", post(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/hook")
}

#[tokio::test]
async fn dispatch_posts_to_the_configured_url_and_succeeds() {
    let url = spawn_stub_webhook(axum::http::StatusCode::OK).await;
    let dispatcher = HttpActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::Webhook, config: json!({"url": url}) };

    let result = dispatcher.dispatch(&action, &sample_event()).await.unwrap();
    assert_eq!(result["http_status"], json!(200));
}

#[tokio::test]
async fn dispatch_without_url_in_config_fails_fast() {
    let dispatcher = HttpActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::Email, config: json!({}) };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::MissingUrl));
}

#[tokio::test]
async fn dispatch_returns_rejected_on_target_error() {
    let url = spawn_stub_webhook(axum::http::StatusCode::INTERNAL_SERVER_ERROR).await;
    let dispatcher = HttpActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::TeamsAlert, config: json!({"url": url}) };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::Rejected(500)));
}

#[tokio::test]
async fn dispatch_returns_unreachable_when_target_is_down() {
    let dispatcher = HttpActionDispatcher::new(None);
    let action = ActionRef {
        action_type: ActionType::CreateTicket,
        config: json!({"url": "http://127.0.0.1:1/hook"}),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::Unreachable(_)));
}

#[tokio::test]
async fn dispatch_returns_unreachable_for_a_malformed_egress_proxy_url() {
    // Proves the proxy configuration actually plumbs through per-dispatch, not just accepted
    // and ignored: an invalid EGRESS_PROXY_URL surfaces as a real dispatch failure.
    let dispatcher = HttpActionDispatcher::new(Some("not a valid url".to_string()));
    let action = ActionRef {
        action_type: ActionType::Webhook,
        config: json!({"url": "http://example.com"}),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::Unreachable(_)));
}
