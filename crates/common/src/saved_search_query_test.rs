use super::*;

#[test]
fn new_generates_a_random_id() {
    let tenant_id = Uuid::new_v4();
    let query =
        SavedSearchQuery::new(tenant_id, "urgent tickets", serde_json::json!({"q": "urgent"}));

    assert_eq!(query.tenant_id, tenant_id);
    assert_eq!(query.name, "urgent tickets");
    assert_eq!(query.filter, serde_json::json!({"q": "urgent"}));
}

#[test]
fn new_generates_distinct_ids_for_distinct_queries() {
    let tenant_id = Uuid::new_v4();
    let a = SavedSearchQuery::new(tenant_id, "a", serde_json::json!({}));
    let b = SavedSearchQuery::new(tenant_id, "b", serde_json::json!({}));

    assert_ne!(a.id, b.id);
}
