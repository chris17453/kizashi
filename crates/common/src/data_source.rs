use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The transport family used to reach an external or uploaded data system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceKind {
    Database,
    Api,
    Stream,
    Cdc,
    Upload,
    Batch,
}

/// Declares whether Kizashi reads evidence, maintains a projection, or may issue a governed
/// command to the authoritative system. Command mode is only a capability declaration; it does
/// not bypass workflow approval, idempotency, or confirmation requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceMode {
    Read,
    Projection,
    Command,
}

/// A tenant-owned external-system definition. Connection information is deliberately limited to
/// non-secret transport metadata; credentials are supplied by an external secret reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSource {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub kind: DataSourceKind,
    pub mode: DataSourceMode,
    pub connection: serde_json::Value,
    pub credential_ref: Option<String>,
    pub enabled: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Validates data-source configuration before it is persisted. This is intentionally a narrow
/// common contract: adapters validate their own host/path/topic shape, but no adapter can cause
/// passwords, tokens, or API keys to be persisted in platform configuration.
pub fn validate_data_source(source: &DataSource) -> Result<(), &'static str> {
    if source.name.trim().is_empty() || source.name.len() > 160 {
        return Err("name must be between 1 and 160 characters");
    }
    if source.description.len() > 2_000 {
        return Err("description is too long");
    }
    if !source.connection.is_object() {
        return Err("connection must be a JSON object");
    }
    if contains_secret_key(&source.connection) {
        return Err("connection must not contain credentials; use credential_ref");
    }
    if source
        .credential_ref
        .as_ref()
        .is_some_and(|reference| reference.trim().is_empty() || reference.len() > 512)
    {
        return Err("credential_ref must be between 1 and 512 characters when present");
    }
    if source.mode == DataSourceMode::Command && source.credential_ref.is_none() {
        return Err("command data sources require a credential_ref");
    }
    if source.version < 1 {
        return Err("version must be positive");
    }
    Ok(())
}

fn contains_secret_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(values) => values.iter().any(|(key, value)| {
            let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
            matches!(normalized.as_str(), "password" | "secret" | "token" | "apikey")
                || contains_secret_key(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_secret_key),
        _ => false,
    }
}

#[cfg(test)]
#[path = "data_source_test.rs"]
mod data_source_test;
