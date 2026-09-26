use crate::trigger_definition::ActionType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A reusable, tenant-scoped provider configuration for a governed trigger response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionTemplate {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub action_type: ActionType,
    pub config: serde_json::Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Validates the minimum configuration contract required by the action executor. Keeping this
/// in `common` makes authoring-time validation agree with execution-time dispatch across the
/// Config/Admin API and Console form paths.
pub fn validate_action_template_config(
    action_type: ActionType,
    config: &serde_json::Value,
) -> Result<(), String> {
    let object = config
        .as_object()
        .ok_or_else(|| "provider configuration must be a JSON object".to_string())?;
    let non_empty_string = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("provider configuration requires a non-empty `{key}` field"))
    };
    let recipients = || match object.get("to") {
        Some(serde_json::Value::String(value)) if !value.trim().is_empty() => Ok(()),
        Some(serde_json::Value::Array(values))
            if values
                .iter()
                .any(|value| value.as_str().is_some_and(|item| !item.trim().is_empty())) =>
        {
            Ok(())
        }
        _ => Err("provider configuration requires a non-empty `to` recipient or array".to_string()),
    };
    match action_type {
        ActionType::Email => {
            let smtp = object.get("smtp_host").and_then(serde_json::Value::as_str).is_some();
            let graph = object.get("graph_client_id").and_then(serde_json::Value::as_str).is_some();
            if smtp {
                non_empty_string("smtp_host")?;
                non_empty_string("from")?;
                recipients()?;
            } else if graph {
                for key in [
                    "graph_token_url",
                    "graph_client_id",
                    "graph_client_secret",
                    "graph_from_user_id",
                ] {
                    non_empty_string(key)?;
                }
                recipients()?;
            } else {
                non_empty_string("url")?;
            }
        }
        ActionType::Webhook
        | ActionType::TeamsAlert
        | ActionType::CreateTicket
        | ActionType::Custom
        | ActionType::GeneratePdf
        | ActionType::GenerateXlsx => {
            non_empty_string("url")?;
        }
    }
    Ok(())
}
