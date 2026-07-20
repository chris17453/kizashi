use super::*;

#[test]
fn new_generates_a_random_id_and_defaults_to_enabled() {
    let tenant_id = Uuid::new_v4();
    let sensor = Sensor::new(tenant_id, "zendesk", "support-poller", serde_json::json!({}));

    assert_eq!(sensor.tenant_id, tenant_id);
    assert_eq!(sensor.connector_type, "zendesk");
    assert_eq!(sensor.name, "support-poller");
    assert!(sensor.enabled);
}

#[test]
fn new_generates_distinct_ids_for_distinct_sensors() {
    let tenant_id = Uuid::new_v4();
    let a = Sensor::new(tenant_id, "zendesk", "a", serde_json::json!({}));
    let b = Sensor::new(tenant_id, "zendesk", "b", serde_json::json!({}));

    assert_ne!(a.id, b.id);
}
