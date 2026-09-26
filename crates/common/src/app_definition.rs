#[cfg(test)]
#[path = "app_definition_test.rs"]
mod app_definition_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tenant-owned composition of stable model and workflow surfaces into an operational app.
/// The app definition contains declarative blocks only; arbitrary executable code is excluded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub model_type_id: Option<Uuid>,
    pub blocks: serde_json::Value,
    pub enabled: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn validate_app_definition(app: &AppDefinition) -> Result<(), &'static str> {
    if app.name.trim().is_empty() || app.name.len() > 160 {
        return Err("name must be between 1 and 160 characters");
    }
    if app.description.len() > 2_000 {
        return Err("description is too long");
    }
    if app.version < 1 {
        return Err("version must be positive");
    }
    let blocks = app.blocks.as_array().ok_or("blocks must be a JSON array")?;
    if blocks.is_empty() || blocks.len() > 30 {
        return Err("blocks must contain between 1 and 30 entries");
    }
    for block in blocks {
        let block = block.as_object().ok_or("each app block must be a JSON object")?;
        let kind = block
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or("app block kind is required")?;
        if !matches!(kind, "form" | "table" | "upload" | "queue" | "dashboard" | "detail") {
            return Err("app block kind is not supported");
        }
        if !block.get("config").is_some_and(serde_json::Value::is_object) {
            return Err("app block config must be an object");
        }
        if block.contains_key("script") {
            return Err("inline scripts are not supported in app definitions");
        }
    }
    Ok(())
}
