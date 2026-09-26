use super::*;

#[test]
fn supported_action_types_match_trigger_action_contract() {
    assert!(supported_action_type(ActionType::Email));
    assert!(supported_action_type(ActionType::Custom));
}

#[test]
fn validation_rejects_non_object_config() {
    let template = ActionTemplate {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "bad".to_string(),
        description: String::new(),
        action_type: ActionType::Webhook,
        config: serde_json::json!([]),
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(validate(&template).is_err());
}
