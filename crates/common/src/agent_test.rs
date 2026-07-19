use super::*;

#[test]
fn new_generates_a_random_id_and_defaults_to_enabled() {
    let tenant_id = Uuid::new_v4();
    let agent = Agent::new(tenant_id, "zendesk", "support-poller", serde_json::json!({}));

    assert_eq!(agent.tenant_id, tenant_id);
    assert_eq!(agent.connector_type, "zendesk");
    assert_eq!(agent.name, "support-poller");
    assert!(agent.enabled);
}

#[test]
fn new_generates_distinct_ids_for_distinct_agents() {
    let tenant_id = Uuid::new_v4();
    let a = Agent::new(tenant_id, "zendesk", "a", serde_json::json!({}));
    let b = Agent::new(tenant_id, "zendesk", "b", serde_json::json!({}));

    assert_ne!(a.id, b.id);
}
