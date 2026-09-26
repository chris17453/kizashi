use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Projection pipelines materialize selected evidence/model state. Command pipelines are the
/// separate, governed path for a confirmed external-system write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineMode {
    Projection,
    Command,
}

/// A tenant-owned, versioned flow definition. Executions are deliberately not embedded here:
/// their durable idempotency, retry, outbox/inbox, and reconciliation state belongs to the
/// pipeline runtime slice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub data_source_id: Uuid,
    pub target_object_type_id: Option<Uuid>,
    pub mode: PipelineMode,
    pub steps: serde_json::Value,
    pub enabled: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Validates the initial declarative pipeline vocabulary. Adapter-specific configuration stays
/// inside each step's `config` object, but executable source text is excluded until a sandboxed,
/// versioned transform runtime exists.
pub fn validate_pipeline_definition(pipeline: &PipelineDefinition) -> Result<(), &'static str> {
    if pipeline.name.trim().is_empty() || pipeline.name.len() > 160 {
        return Err("name must be between 1 and 160 characters");
    }
    if pipeline.description.len() > 2_000 {
        return Err("description is too long");
    }
    if pipeline.version < 1 {
        return Err("version must be positive");
    }
    let steps = pipeline.steps.as_array().ok_or("steps must be a JSON array")?;
    if steps.is_empty() || steps.len() > 50 {
        return Err("steps must contain between 1 and 50 entries");
    }
    let mut write_backs = 0;
    for step in steps {
        let step = step.as_object().ok_or("each step must be a JSON object")?;
        let kind =
            step.get("kind").and_then(serde_json::Value::as_str).ok_or("step kind is required")?;
        if !matches!(kind, "extract" | "transform" | "validate" | "match" | "route" | "write_back")
        {
            return Err("step kind is not supported");
        }
        let config = step.get("config").ok_or("step config is required")?;
        if !config.is_object() {
            return Err("step config must be a JSON object");
        }
        if contains_script_key(config) {
            return Err("inline scripts are not supported in pipeline definitions");
        }
        if kind == "write_back" {
            write_backs += 1;
        }
    }
    match pipeline.mode {
        PipelineMode::Projection if write_backs > 0 => {
            Err("projection pipelines cannot include write_back steps")
        }
        PipelineMode::Command if write_backs == 0 => {
            Err("command pipelines require a write_back step")
        }
        _ => Ok(()),
    }
}

fn contains_script_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("script") || contains_script_key(value)),
        serde_json::Value::Array(values) => values.iter().any(contains_script_key),
        _ => false,
    }
}

#[cfg(test)]
#[path = "pipeline_definition_test.rs"]
mod pipeline_definition_test;
