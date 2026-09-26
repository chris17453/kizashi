use super::*;
#[test]
fn app_definition_accepts_stable_composable_blocks() {
    let app = AppDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "Operations".to_string(),
        description: String::new(),
        model_type_id: None,
        blocks: serde_json::json!([{"kind":"form","config":{}},{"kind":"queue","config":{}}]),
        enabled: true,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(validate_app_definition(&app).is_ok());
}
#[test]
fn app_definition_rejects_executable_blocks() {
    let app = AppDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "Operations".to_string(),
        description: String::new(),
        model_type_id: None,
        blocks: serde_json::json!([{"kind":"form","config":{},"script":"alert(1)"}]),
        enabled: true,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(validate_app_definition(&app).is_err());
}
