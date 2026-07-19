use super::*;
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

#[tokio::test]
async fn dispatch_returns_invalid_config_when_smtp_host_is_missing() {
    let dispatcher = SmtpActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({"from": "a@example.com", "to": "b@example.com"}),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::InvalidConfig(msg) if msg.contains("smtp_host")));
}

#[tokio::test]
async fn dispatch_returns_invalid_config_when_to_is_missing() {
    let dispatcher = SmtpActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({"smtp_host": "localhost", "from": "a@example.com"}),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::InvalidConfig(msg) if msg.contains("to")));
}

#[tokio::test]
async fn dispatch_returns_invalid_config_for_a_malformed_from_address() {
    let dispatcher = SmtpActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({
            "smtp_host": "localhost",
            "from": "not-an-email",
            "to": "b@example.com",
        }),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::InvalidConfig(_)));
}

#[tokio::test]
async fn dispatch_returns_unreachable_when_smtp_server_is_down() {
    let dispatcher = SmtpActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({
            "smtp_host": "127.0.0.1",
            "smtp_port": 1,
            "smtp_use_tls": false,
            "from": "a@example.com",
            "to": "b@example.com",
        }),
    };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::Unreachable(_)));
}

#[test]
fn recipients_accepts_a_single_string_or_an_array() {
    assert_eq!(recipients(&json!({"to": "a@example.com"})).unwrap(), vec!["a@example.com"]);
    assert_eq!(
        recipients(&json!({"to": ["a@example.com", "b@example.com"]})).unwrap(),
        vec!["a@example.com", "b@example.com"]
    );
    assert!(recipients(&json!({})).is_err());
    assert!(recipients(&json!({"to": []})).is_err());
}
