use super::*;
use std::collections::BTreeMap;
use uuid::Uuid;

fn sample_mapping() -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping::new(Uuid::new_v4(), "ticket", field_map)
}

#[test]
fn upserted_round_trips_through_json() {
    let event = MappingChangeEvent::Upserted(sample_mapping());
    let json = serde_json::to_string(&event).unwrap();
    let back: MappingChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}

#[test]
fn deleted_round_trips_through_json() {
    let event = MappingChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };
    let json = serde_json::to_string(&event).unwrap();
    let back: MappingChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}
