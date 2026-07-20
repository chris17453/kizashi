use super::*;
use crate::Sensor;
use uuid::Uuid;

fn sample_sensor() -> Sensor {
    Sensor::new(Uuid::new_v4(), "zendesk", "support-poller", serde_json::json!({}))
}

#[test]
fn upserted_round_trips_through_json() {
    let event = SensorChangeEvent::Upserted(sample_sensor());
    let json = serde_json::to_string(&event).unwrap();
    let back: SensorChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}

#[test]
fn deleted_round_trips_through_json() {
    let event = SensorChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };
    let json = serde_json::to_string(&event).unwrap();
    let back: SensorChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}
