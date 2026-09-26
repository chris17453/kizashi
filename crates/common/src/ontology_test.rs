use super::*;
use serde_json::json;
use uuid::Uuid;

#[test]
fn test_object_type_serialization() {
    let ot = ObjectType {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "Customer".to_string(),
        version: 1,
        property_schema: json!({"type": "object", "properties": {"email": {"type": "string"}}}),
        mapping_rules: json!([]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let serialized = serde_json::to_string(&ot).unwrap();
    let deserialized: ObjectType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(ot, deserialized);
}

#[test]
fn link_type_history_serializes_as_an_immutable_definition_snapshot() {
    let history = LinkTypeHistory {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        link_type_id: Uuid::new_v4(),
        change_type: "updated".to_string(),
        actor: "operator".to_string(),
        before_state: Some(json!({"cardinality": "many-to-one"})),
        after_state: Some(json!({"cardinality": "one-to-many"})),
        changed_at: Utc::now(),
    };
    let round_trip: LinkTypeHistory =
        serde_json::from_value(serde_json::to_value(&history).unwrap()).unwrap();
    assert_eq!(round_trip, history);
}

#[test]
fn link_history_serializes_as_an_instance_snapshot() {
    let history = LinkHistory {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        link_id: Uuid::new_v4(),
        change_type: "created".to_string(),
        actor: "operator".to_string(),
        before_state: None,
        after_state: Some(json!({"properties": {"source": "console"}})),
        changed_at: Utc::now(),
    };
    let round_trip: LinkHistory =
        serde_json::from_value(serde_json::to_value(&history).unwrap()).unwrap();
    assert_eq!(round_trip, history);
}
