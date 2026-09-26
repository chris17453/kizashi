use super::*;

#[test]
fn validation_rejects_an_invalid_data_source() {
    let value = common::DataSource {
        id: uuid::Uuid::new_v4(),
        tenant_id: uuid::Uuid::new_v4(),
        name: String::new(),
        description: String::new(),
        kind: common::DataSourceKind::Api,
        mode: common::DataSourceMode::Read,
        connection: serde_json::json!({}),
        credential_ref: None,
        enabled: true,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(validate(&value).is_err());
}
