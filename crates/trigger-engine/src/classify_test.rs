use super::*;
use common::{RawRecord, SourceType};
use serde_json::json;
use uuid::Uuid;

fn record_with_analysis(analysis: serde_json::Value) -> AnalyzedRecord {
    let raw = RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({}));
    AnalyzedRecord::new(raw, analysis)
}

#[test]
fn candidates_picks_up_every_numeric_top_level_key() {
    let record = record_with_analysis(json!({"sentiment": -0.8, "urgency": 0.6}));

    let mut found = candidates(&record);
    found.sort_by(|a, b| a.event_type.cmp(&b.event_type));

    assert_eq!(
        found,
        vec![
            Candidate { event_type: "sentiment".to_string(), numeric_value: -0.8 },
            Candidate { event_type: "urgency".to_string(), numeric_value: 0.6 },
        ]
    );
}

#[test]
fn candidates_ignores_non_numeric_keys() {
    let record = record_with_analysis(json!({"sentiment": -0.8, "summary": "printer on fire"}));
    let found = candidates(&record);
    assert_eq!(found, vec![Candidate { event_type: "sentiment".to_string(), numeric_value: -0.8 }]);
}

#[test]
fn candidates_on_non_object_analysis_is_empty_not_a_panic() {
    let record = record_with_analysis(json!("not an object"));
    assert!(candidates(&record).is_empty());

    let record = record_with_analysis(json!(null));
    assert!(candidates(&record).is_empty());

    let record = record_with_analysis(json!([1, 2, 3]));
    assert!(candidates(&record).is_empty());
}

#[test]
fn group_key_uses_normalized_entity_ref_when_present() {
    let mut raw = RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({}));
    raw.normalized_payload = Some(json!({"entity_ref": "cust-42"}));
    let record = AnalyzedRecord::new(raw, json!({}));

    assert_eq!(group_key(&record), "cust-42");
}

#[test]
fn group_key_falls_back_to_record_id_when_no_entity_ref() {
    let raw = RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({}));
    let record_id = raw.id;
    let record = AnalyzedRecord::new(raw, json!({}));

    assert_eq!(group_key(&record), record_id.to_string());
}
