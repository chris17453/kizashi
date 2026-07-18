use super::*;
use serde_json::json;

#[test]
fn new_defaults_to_new_status_and_empty_source_connectors() {
    let tenant_id = Uuid::new_v4();
    let event = Event::new(
        tenant_id,
        "sentiment.negative",
        "customer-42",
        "customer-42",
        json!({"score": -0.8}),
        Utc::now(),
    );

    assert_eq!(event.status, EventStatus::New);
    assert!(event.source_connector_ids.is_empty());
    assert!(event.is_actionable());
}

#[test]
fn dismissed_and_actioned_events_are_not_actionable() {
    let tenant_id = Uuid::new_v4();
    let mut event = Event::new(tenant_id, "t", "e", "g", json!({}), Utc::now());

    event.status = EventStatus::Dismissed;
    assert!(!event.is_actionable());

    event.status = EventStatus::Actioned;
    assert!(!event.is_actionable());

    event.status = EventStatus::Triggered;
    assert!(event.is_actionable());
}

#[test]
fn event_status_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&EventStatus::New).unwrap(), "\"new\"");
    assert_eq!(serde_json::to_string(&EventStatus::Actioned).unwrap(), "\"actioned\"");
}

#[test]
fn round_trips_through_json() {
    let tenant_id = Uuid::new_v4();
    let mut event = Event::new(tenant_id, "t", "e", "g", json!({"k": "v"}), Utc::now());
    event.source_connector_ids = vec!["zendesk".to_string(), "graph:mail".to_string()];

    let serialized = serde_json::to_string(&event).unwrap();
    let deserialized: Event = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, event);
}
