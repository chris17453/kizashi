//! Schema/contract test for the `record.analyzed` bus message (spec §3), so Analysis Service
//! (producer) and the Aggregation/Trigger Engine (consumer) cannot silently drift apart on the
//! wire shape, per CLAUDE.md §2's contract-test requirement.

use common::{AnalyzedRecord, RawRecord, SourceType};
use serde_json::json;
use uuid::Uuid;

#[test]
fn record_analyzed_message_has_record_and_analysis_fields() {
    let record =
        RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({"description": "hi"}));
    let analyzed = AnalyzedRecord::new(record, json!({"sentiment": -0.8}));

    let message = serde_json::to_value(&analyzed).unwrap();
    let obj = message.as_object().expect("record.analyzed payload must be a JSON object");

    for field in ["record", "analysis", "analyzed_at"] {
        assert!(
            obj.contains_key(field),
            "record.analyzed payload missing required field `{field}`"
        );
    }
}

#[test]
fn record_analyzed_message_round_trips() {
    let record =
        RawRecord::new("graph:mail", SourceType::Message, Uuid::new_v4(), json!({"body": "hi"}));
    let analyzed = AnalyzedRecord::new(record, json!({"sentiment": 0.2}));

    let message = serde_json::to_vec(&analyzed).unwrap();
    let deserialized: AnalyzedRecord = serde_json::from_slice(&message).unwrap();
    assert_eq!(deserialized, analyzed);
}
