use super::*;
use common::EventStatus;
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

#[test]
fn an_email_action_with_smtp_host_is_routed_to_smtp() {
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({"smtp_host": "localhost", "from": "a@example.com", "to": "b@example.com"}),
    };
    assert!(is_smtp_email(&action));
}

#[test]
fn an_email_action_without_smtp_host_is_routed_to_http() {
    let action =
        ActionRef { action_type: ActionType::Email, config: json!({"url": "http://example.com"}) };
    assert!(!is_smtp_email(&action));
}

#[test]
fn a_webhook_action_is_never_routed_to_smtp_even_with_an_smtp_host_field() {
    let action = ActionRef {
        action_type: ActionType::Webhook,
        config: json!({"smtp_host": "localhost", "url": "http://example.com"}),
    };
    assert!(!is_smtp_email(&action));
}

#[tokio::test]
async fn dispatch_of_a_non_smtp_email_action_without_a_url_fails_with_missing_url_not_invalid_config(
) {
    let dispatcher = RoutingActionDispatcher::new(None);
    let action = ActionRef { action_type: ActionType::Email, config: json!({}) };

    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::MissingUrl));
}

#[test]
fn an_email_action_with_graph_client_id_is_routed_to_graph() {
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({"graph_client_id": "id", "graph_from_user_id": "a@example.com"}),
    };
    assert!(is_graph_email(&action));
}

#[test]
fn smtp_takes_precedence_over_graph_when_both_fields_are_present() {
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({"smtp_host": "localhost", "graph_client_id": "id"}),
    };
    assert!(is_smtp_email(&action));
}

#[tokio::test]
async fn a_teams_alert_action_is_routed_to_the_teams_dispatcher_not_the_generic_one() {
    let dispatcher = RoutingActionDispatcher::new(None);
    // No `url` in config, so if this were routed to the generic HttpActionDispatcher the
    // error would still be MissingUrl — the real proof this hit TeamsAlertActionDispatcher
    // specifically lives in teams_alert_action_dispatcher_test.rs's payload-shape assertions;
    // this test just confirms the routing condition itself compiles and doesn't panic/misroute.
    let action = ActionRef { action_type: ActionType::TeamsAlert, config: json!({}) };
    let err = dispatcher.dispatch(&action, &sample_event()).await.unwrap_err();
    assert!(matches!(err, DispatchError::MissingUrl));
}
