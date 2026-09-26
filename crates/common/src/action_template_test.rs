use super::*;
use crate::ActionType;
use chrono::Utc;
use uuid::Uuid;

#[test]
fn action_template_round_trips_provider_config() {
    let template = ActionTemplate {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "Escalate via webhook".to_string(),
        description: "Notify the incident desk".to_string(),
        action_type: ActionType::Webhook,
        config: serde_json::json!({"url":"https://example.test/hooks/incident"}),
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let decoded: ActionTemplate =
        serde_json::from_value(serde_json::to_value(&template).unwrap()).unwrap();
    assert_eq!(decoded, template);
}

#[test]
fn provider_config_requires_endpoint_for_http_actions() {
    assert!(validate_action_template_config(ActionType::Webhook, &serde_json::json!({})).is_err());
    assert!(validate_action_template_config(
        ActionType::Webhook,
        &serde_json::json!({"url":"https://hooks.example.test"}),
    )
    .is_ok());
}

#[test]
fn provider_config_accepts_smtp_and_graph_email_contracts() {
    assert!(validate_action_template_config(
        ActionType::Email,
        &serde_json::json!({"smtp_host":"smtp.example.test","from":"alerts@example.test","to":["ops@example.test"]}),
    )
    .is_ok());
    assert!(validate_action_template_config(
        ActionType::Email,
        &serde_json::json!({"graph_token_url":"https://login.example.test/token","graph_client_id":"id","graph_client_secret":"secret","graph_from_user_id":"sender","to":"ops@example.test"}),
    )
    .is_ok());
}
