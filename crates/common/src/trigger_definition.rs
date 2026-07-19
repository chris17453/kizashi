#[path = "trigger_definition_test.rs"]
#[cfg(test)]
mod trigger_definition_test;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Fires when every listed event type has accumulated at least its own `min_count` of
    /// signals for the same `group_key` within the window (ADR-0027) — e.g. "a negative-
    /// sentiment email AND an unresolved chat message from the same customer." Not an
    /// open-ended AND/OR/NOT tree; a closed, enumerable "all of these must be present" shape,
    /// same spirit as the other two.
    CorrelatedOverWindow { conditions: Vec<CorrelatedCondition> },
}

/// One leg of a `CorrelatedOverWindow` condition: `event_type` must have at least `min_count`
/// signals within the trigger's window for the condition to be satisfied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrelatedCondition {
    pub event_type: String,
    pub min_count: u32,
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
            // Not evaluated through this path — see `evaluate_correlated`.
            TriggerCondition::CorrelatedOverWindow { .. } => false,
        }
    }

    /// Evaluates a `CorrelatedOverWindow` condition (ADR-0027): fires only when `counts`
    /// carries at least `min_count` for every listed event type. A missing entry (no signal of
    /// that type seen in the window at all) counts as zero, not an error — never panics on
    /// malformed/adversarial config, same guarantee `evaluate` gives the other two shapes.
    pub fn evaluate_correlated(&self, counts: &HashMap<String, u32>) -> bool {
        if !self.enabled {
            return false;
        }
        let TriggerCondition::CorrelatedOverWindow { conditions } = &self.condition else {
            return false;
        };
        !conditions.is_empty()
            && conditions
                .iter()
                .all(|c| counts.get(&c.event_type).copied().unwrap_or(0) >= c.min_count)
    }
}
