use super::*;
use serde_json::json;
use uuid::Uuid;

fn source() -> DataSource {
    DataSource {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "Northwind ERP".to_string(),
        description: "Authoritative inventory projection".to_string(),
        kind: DataSourceKind::Database,
        mode: DataSourceMode::Projection,
        connection: json!({"host": "erp.example.test", "database": "northwind"}),
        credential_ref: Some("vault://tenant/northwind/read".to_string()),
        enabled: true,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn data_source_contract_accepts_reference_based_external_connection() {
    assert!(validate_data_source(&source()).is_ok());
}

#[test]
fn data_source_contract_rejects_secrets_and_invalid_command_modes() {
    let mut invalid_secret = source();
    invalid_secret.connection = json!({"host": "erp.example.test", "password": "do-not-store"});
    assert!(validate_data_source(&invalid_secret).is_err());

    let mut invalid_command = source();
    invalid_command.mode = DataSourceMode::Command;
    invalid_command.credential_ref = None;
    assert!(validate_data_source(&invalid_command).is_err());
}
