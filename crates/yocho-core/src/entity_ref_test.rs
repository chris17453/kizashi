use super::*;
use serde_json::json;

#[test]
fn new_sets_type_and_id() {
    let e = EntityRef::new("email", "a@example.com");
    assert_eq!(e.entity_type, "email");
    assert_eq!(e.entity_id, "a@example.com");
}

#[test]
fn serializes_as_flat_object() {
    let e = EntityRef::new("zendesk_org", "42");
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!({"entity_type": "zendesk_org", "entity_id": "42"})
    );
}
