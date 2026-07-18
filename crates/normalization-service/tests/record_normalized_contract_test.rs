//! Schema/contract test for the `record.normalized` bus message (spec §3), so Normalization
//! Service (producer) and Analysis Service (consumer) cannot silently drift apart on the wire
//! shape, per CLAUDE.md §2's contract-test requirement.

use common::{RawRecord, SourceType};
use serde_json::json;
use uuid::Uuid;

#[test]
fn record_normalized_message_carries_a_populated_normalized_payload() {
    let mut record =
        RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({"description": "hi"}));
    record.normalized_payload = Some(json!({"text": "hi"}));

    let message = serde_json::to_value(&record).unwrap();
    let obj = message.as_object().expect("record.normalized payload must be a JSON object");

    assert!(obj.contains_key("normalized_payload"));
    assert!(!obj["normalized_payload"].is_null());
}

#[test]
fn record_normalized_message_round_trips_back_into_a_raw_record() {
    let mut record =
        RawRecord::new("graph:mail", SourceType::Message, Uuid::new_v4(), json!({"body": "hi"}));
    record.normalized_payload = Some(json!({"text": "hi"}));

    let message = serde_json::to_vec(&record).unwrap();
    let deserialized: RawRecord = serde_json::from_slice(&message).unwrap();
    assert_eq!(deserialized, record);
}
