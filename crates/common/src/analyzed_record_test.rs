use super::*;
use crate::raw_record::SourceType;
use serde_json::json;
use uuid::Uuid;

#[test]
fn new_sets_analyzed_at_and_carries_the_record_and_analysis() {
    let record = RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({}));
    let analyzed = AnalyzedRecord::new(record.clone(), json!({"sentiment": -0.8}));

    assert_eq!(analyzed.record, record);
    assert_eq!(analyzed.analysis, json!({"sentiment": -0.8}));
}

#[test]
fn round_trips_through_json() {
    let record = RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({}));
    let analyzed = AnalyzedRecord::new(record, json!({"sentiment": -0.8}));

    let serialized = serde_json::to_string(&analyzed).unwrap();
    let deserialized: AnalyzedRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, analyzed);
}
