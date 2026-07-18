use super::*;
use serde_json::json;

#[test]
fn new_starts_at_version_one() {
    let tenant_id = Uuid::new_v4();
    let def = EventTypeDefinition::new(tenant_id, "sentiment.negative", json!({"type": "object"}));
    assert_eq!(def.version, 1);
    assert_eq!(def.tenant_id, tenant_id);
}

#[test]
fn next_version_increments_and_generates_new_id_but_keeps_name_and_tenant() {
    let tenant_id = Uuid::new_v4();
    let v1 = EventTypeDefinition::new(tenant_id, "sentiment.negative", json!({"type": "object"}));
    let v2 = v1.next_version(json!({"type": "object", "required": ["score"]}));

    assert_eq!(v2.version, 2);
    assert_eq!(v2.name, v1.name);
    assert_eq!(v2.tenant_id, v1.tenant_id);
    assert_ne!(v2.id, v1.id);
    assert_ne!(v2.field_schema, v1.field_schema);
}
