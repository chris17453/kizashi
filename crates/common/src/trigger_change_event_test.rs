use super::*;
use crate::{ThresholdDirection, TriggerCondition};
use uuid::Uuid;

fn sample_trigger() -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "urgent-spike".to_string(),
        event_type_match: "priority_score".to_string(),
        condition: TriggerCondition::ThresholdOverWindow {
            field: "priority_score".to_string(),
            threshold: 5.0,
            direction: ThresholdDirection::Above,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[test]
fn upserted_round_trips_through_json() {
    let event = TriggerChangeEvent::Upserted(sample_trigger());
    let json = serde_json::to_string(&event).unwrap();
    let back: TriggerChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}

#[test]
fn deleted_round_trips_through_json() {
    let event = TriggerChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };
    let json = serde_json::to_string(&event).unwrap();
    let back: TriggerChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}
