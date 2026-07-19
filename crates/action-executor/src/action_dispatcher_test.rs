use super::*;
use axum::extract::Json as JsonExtractor;
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

#[test]
fn render_body_template_substitutes_recognized_placeholders() {
    let event = sample_event();
    let template = json!({
        "text": "{{event_type}} for {{entity_ref}} (tenant {{tenant_id}})",
        "nested": {"group": "{{group_key}}", "raw_payload": "{{payload}}"},
        "list": ["{{event_type}}", "literal"],
        "unchanged_number": 42,
    });

    let rendered = render_body_template(&template, &event);

    assert_eq!(rendered["text"], format!("sentiment for cust-1 (tenant {})", event.tenant_id));
    assert_eq!(rendered["nested"]["group"], "cust-1");
    assert_eq!(rendered["nested"]["raw_payload"], event.payload.to_string());
    assert_eq!(rendered["list"][0], "sentiment");
    assert_eq!(rendered["list"][1], "literal");
    assert_eq!(rendered["unchanged_number"], 42);
}

#[test]
fn render_body_template_leaves_unrecognized_placeholders_as_literal_text() {
    let event = sample_event();
    let template = json!({"text": "{{not_a_real_field}} stays literal"});

    let rendered = render_body_template(&template, &event);

    assert_eq!(rendered["text"], "{{not_a_real_field}} stays literal");
}

async fn spawn_capturing_stub_webhook() -> (String, std::sync::Arc<Mutex<Vec<serde_json::Value>>>) {
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
async fn dispatch_sends_the_rendered_body_template_instead_of_the_generic_envelope() {
    let (url, captured) = spawn_capturing_stub_webhook().await;
    let dispatcher = HttpActionDispatcher::new(None);
    let action = ActionRef {
        action_type: ActionType::Webhook,
        config: json!({"url": url, "body_template": {"text": "alert: {{entity_ref}}"}}),
    };

    dispatcher.dispatch(&action, &sample_event()).await.unwrap();

    let bodies = captured.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0], json!({"text": "alert: cust-1"}));
    assert!(bodies[0].get("action_type").is_none(), "must not fall back to the generic envelope");
}

#[tokio::test]
async fn dispatch_without_a_body_template_still_sends_the_generic_envelope() {
    let (url, captured) = spawn_capturing_stub_webhook().await;
    let dispatcher = HttpActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::Webhook, config: json!({"url": url}) };

    dispatcher.dispatch(&action, &sample_event()).await.unwrap();

    let bodies = captured.lock().unwrap();
    assert_eq!(bodies[0]["action_type"], "webhook");
    assert!(bodies[0].get("event").is_some());
}
