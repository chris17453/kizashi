use super::*;
use serde_json::json;

#[test]
fn new_starts_pending() {
    let trigger_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let exec =
        ActionExecution::new(trigger_id, event_id, ActionType::Email, json!({"to": "a@b.com"}));

    assert_eq!(exec.status, ActionExecutionStatus::Pending);
    assert_eq!(exec.trigger_id, trigger_id);
    assert_eq!(exec.event_id, event_id);
}

#[test]
fn retry_creates_new_row_referencing_same_trigger_and_event() {
    let trigger_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let original = ActionExecution::new(trigger_id, event_id, ActionType::Webhook, json!({}));
    let retried = original.retry(json!({"attempt": 2}));

    assert_ne!(retried.id, original.id, "retry must be a new append-only row, not a mutation");
    assert_eq!(retried.trigger_id, original.trigger_id);
    assert_eq!(retried.event_id, original.event_id);
    assert_eq!(retried.status, ActionExecutionStatus::Retried);
    assert_eq!(retried.action_type, original.action_type);
}

#[test]
fn status_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&ActionExecutionStatus::Retried).unwrap(), "\"retried\"");
}
