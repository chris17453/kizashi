//! Schema/contract test for the `event.created` bus message (spec §3), so Trigger Engine
//! (producer) and Action Executor (consumer) cannot silently drift apart on the wire shape,
//! per CLAUDE.md §2's contract-test requirement.

use common::Event;
use serde_json::json;
use uuid::Uuid;

#[test]
fn event_created_message_has_all_required_fields() {
    let event = Event::new(
        Uuid::new_v4(),
        "sentiment",
        "cust-1",
        "cust-1",
        json!({"value": -0.8}),
        chrono::Utc::now(),
    );
    let message = serde_json::to_value(&event).unwrap();
    let obj = message.as_object().expect("event.created payload must be a JSON object");

    for field in [
        "id",
        "tenant_id",
        "event_type",
        "source_connector_ids",
        "entity_ref",
        "group_key",
        "payload",
        "occurred_at",
        "created_at",
        "status",
    ] {
        assert!(obj.contains_key(field), "event.created payload missing required field `{field}`");
    }
}

#[test]
fn event_created_message_round_trips() {
    let event = Event::new(
        Uuid::new_v4(),
        "sentiment",
        "cust-1",
        "cust-1",
        json!({"value": -0.8}),
        chrono::Utc::now(),
    );
    let message = serde_json::to_vec(&event).unwrap();
    let deserialized: Event = serde_json::from_slice(&message).unwrap();
    assert_eq!(deserialized, event);
}
