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
