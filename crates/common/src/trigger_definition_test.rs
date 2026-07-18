use super::*;
use proptest::prelude::*;
use serde_json::json;

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
}
