use super::*;

#[test]
fn repository_version_contract_keeps_name_and_increments_version() {
    let tenant_id = Uuid::new_v4();
    let first = EventTypeDefinition::new(
        tenant_id,
        "risk.alert",
        serde_json::json!({"score": {"type": "number"}}),
    );
    let second = first.next_version(
        serde_json::json!({"score": {"type": "number"}, "reason": {"type": "string"}}),
    );
    assert_eq!(second.tenant_id, tenant_id);
    assert_eq!(second.name, first.name);
    assert_eq!(second.version, first.version + 1);
    assert_ne!(second.id, first.id);
}
