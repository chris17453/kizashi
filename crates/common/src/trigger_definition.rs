#[path = "trigger_definition_test.rs"]
#[cfg(test)]
mod trigger_definition_test;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A trigger fires when its `condition` is satisfied by events matching `event_type_match`
/// within `window`, grouped by `Event.group_key` (spec §5.4).
///
/// v1 ships fixed condition "shapes" rather than a full expression language — see
/// docs/adr/0001-trigger-condition-dsl-shape.md. This keeps the evaluator provably
/// non-panicking on operator-authored config (spec §2 "config over code" + the
/// property/fuzz-test requirement in CLAUDE.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub event_type_match: String,
    pub condition: TriggerCondition,
    pub window_seconds: i64,
    pub actions: Vec<ActionRef>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum TriggerCondition {
    /// Fires when at least `count` matching events share a `group_key` within the window.
    CountOverWindow { count: u32 },
    /// Fires when a numeric field in the event payload crosses `threshold` in `direction`.
    ThresholdOverWindow { field: String, threshold: f64, direction: ThresholdDirection },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdDirection {
    Above,
    Below,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRef {
    pub action_type: ActionType,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Email,
    Webhook,
    TeamsAlert,
    CreateTicket,
    Custom,
}

impl TriggerDefinition {
    /// Evaluates `condition` against a slice of numeric payload values already extracted
    /// for events sharing one group_key within the window. Never panics on malformed input:
    /// missing/non-numeric fields count as absent, not an error.
    pub fn evaluate(&self, matching_event_count: u32, field_values: &[f64]) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.condition {
            TriggerCondition::CountOverWindow { count } => matching_event_count >= *count,
            TriggerCondition::ThresholdOverWindow { threshold, direction, .. } => {
                field_values.iter().any(|v| match direction {
                    ThresholdDirection::Above => *v > *threshold,
                    ThresholdDirection::Below => *v < *threshold,
                })
            }
        }
    }
}
