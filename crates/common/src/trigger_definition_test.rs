use super::*;
use proptest::prelude::*;
use serde_json::json;
use std::collections::HashMap;

fn count_trigger(count: u32, enabled: bool) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "three-in-window".to_string(),
        event_type_match: "sentiment.negative".to_string(),
        condition: TriggerCondition::CountOverWindow { count },
        window_seconds: 3600,
        actions: vec![ActionRef {
            action_type: ActionType::Email,
            config: json!({"to": "ops@example.com"}),
        }],
        enabled,
    }
}

fn threshold_trigger(
    threshold: f64,
    direction: ThresholdDirection,
    enabled: bool,
) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "score-threshold".to_string(),
        event_type_match: "sentiment.negative".to_string(),
        condition: TriggerCondition::ThresholdOverWindow {
            field: "score".to_string(),
            threshold,
            direction,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled,
    }
}

#[test]
fn count_over_window_fires_when_count_meets_threshold() {
    let trigger = count_trigger(3, true);
    assert!(!trigger.evaluate(2, &[]));
    assert!(trigger.evaluate(3, &[]));
    assert!(trigger.evaluate(10, &[]));
}

#[test]
fn disabled_trigger_never_fires() {
    let trigger = count_trigger(1, false);
    assert!(!trigger.evaluate(999, &[]));
}

#[test]
fn threshold_above_fires_when_any_value_exceeds() {
    let trigger = threshold_trigger(0.5, ThresholdDirection::Above, true);
    assert!(!trigger.evaluate(0, &[0.1, 0.2, 0.5]));
    assert!(trigger.evaluate(0, &[0.1, 0.51]));
}

#[test]
fn threshold_below_fires_when_any_value_under() {
    let trigger = threshold_trigger(-0.5, ThresholdDirection::Below, true);
    assert!(!trigger.evaluate(0, &[-0.1, -0.5]));
    assert!(trigger.evaluate(0, &[-0.6]));
}

#[test]
fn threshold_condition_with_empty_field_values_never_fires() {
    let trigger = threshold_trigger(0.0, ThresholdDirection::Above, true);
    assert!(!trigger.evaluate(0, &[]));
}

fn correlated_trigger(conditions: Vec<CorrelatedCondition>, enabled: bool) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "email-and-chat".to_string(),
        event_type_match: conditions.first().map(|c| c.event_type.clone()).unwrap_or_default(),
        condition: TriggerCondition::CorrelatedOverWindow { conditions },
        window_seconds: 3600,
        actions: vec![],
        enabled,
    }
}

#[test]
fn correlated_over_window_fires_only_when_every_event_type_meets_its_min_count() {
    let trigger = correlated_trigger(
        vec![
            CorrelatedCondition { event_type: "sentiment_drop_email".to_string(), min_count: 1 },
            CorrelatedCondition { event_type: "unresolved_chat".to_string(), min_count: 2 },
        ],
        true,
    );

    let mut counts = HashMap::new();
    assert!(!trigger.evaluate_correlated(&counts), "no signals at all must not fire");

    counts.insert("sentiment_drop_email".to_string(), 1);
    assert!(!trigger.evaluate_correlated(&counts), "missing the chat leg must not fire");

    counts.insert("unresolved_chat".to_string(), 1);
    assert!(
        !trigger.evaluate_correlated(&counts),
        "chat leg below its own min_count must not fire"
    );

    counts.insert("unresolved_chat".to_string(), 2);
    assert!(trigger.evaluate_correlated(&counts), "both legs satisfied must fire");
}

#[test]
fn correlated_over_window_with_no_conditions_never_fires() {
    let trigger = correlated_trigger(vec![], true);
    assert!(!trigger.evaluate_correlated(&HashMap::new()));
}

#[test]
fn disabled_correlated_trigger_never_fires_even_if_every_leg_is_satisfied() {
    let trigger = correlated_trigger(
        vec![CorrelatedCondition { event_type: "a".to_string(), min_count: 1 }],
        false,
    );
    let mut counts = HashMap::new();
    counts.insert("a".to_string(), 100);
    assert!(!trigger.evaluate_correlated(&counts));
}

#[test]
fn correlated_over_window_ignores_unrelated_event_types_in_counts() {
    let trigger = correlated_trigger(
        vec![CorrelatedCondition { event_type: "a".to_string(), min_count: 1 }],
        true,
    );
    let mut counts = HashMap::new();
    counts.insert("unrelated".to_string(), 999);
    assert!(!trigger.evaluate_correlated(&counts));
}

#[test]
fn condition_serializes_with_shape_tag() {
    let condition = TriggerCondition::CountOverWindow { count: 3 };
    let serialized = serde_json::to_string(&condition).unwrap();
    assert!(serialized.contains("\"shape\":\"count_over_window\""));
    assert!(serialized.contains("\"count\":3"));
}

proptest! {
    // Operators author trigger conditions as config; the evaluator must never panic
    // regardless of what count/field values a malformed or adversarial config produces.
    #[test]
    fn evaluate_never_panics_on_arbitrary_input(
        count in 0u32..10_000,
        matching_event_count in 0u32..10_000,
        threshold in -1e6f64..1e6,
        field_values in proptest::collection::vec(-1e6f64..1e6, 0..50),
        enabled in any::<bool>(),
    ) {
        let count_t = count_trigger(count, enabled);
        let _ = count_t.evaluate(matching_event_count, &field_values);

        let threshold_t = threshold_trigger(threshold, ThresholdDirection::Above, enabled);
        let _ = threshold_t.evaluate(matching_event_count, &field_values);

        let threshold_below_t = threshold_trigger(threshold, ThresholdDirection::Below, enabled);
        let _ = threshold_below_t.evaluate(matching_event_count, &field_values);
    }

    // Same guarantee for the correlated shape: arbitrary event-type/min_count legs and an
    // arbitrary counts map (including entries that don't match any leg) must never panic.
    #[test]
    fn evaluate_correlated_never_panics_on_arbitrary_input(
        event_types in proptest::collection::vec("[a-z_]{0,10}", 0..5),
        min_counts in proptest::collection::vec(0u32..10_000, 0..5),
        counts_values in proptest::collection::vec(("[a-z_]{0,10}", 0u32..10_000), 0..10),
        enabled in any::<bool>(),
    ) {
        let conditions: Vec<CorrelatedCondition> = event_types
            .into_iter()
            .zip(min_counts)
            .map(|(event_type, min_count)| CorrelatedCondition { event_type, min_count })
            .collect();
        let trigger = correlated_trigger(conditions, enabled);
        let counts: HashMap<String, u32> = counts_values.into_iter().collect();
        let _ = trigger.evaluate_correlated(&counts);
    }
}
