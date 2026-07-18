//! Schema/contract test for the `record.ingested` bus message (spec §3), so Ingestion
//! Service (producer) and Normalization Service (consumer) cannot silently drift apart on
//! the wire shape, per CLAUDE.md §2's contract-test requirement.

use common::{RawRecord, SourceType};
use serde_json::json;
use uuid::Uuid;

#[test]
fn record_ingested_message_is_a_raw_record_json_object_with_required_fields() {
    let record =
        RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({"subject": "hi"}));
    let message = serde_json::to_value(&record).unwrap();

    let obj = message.as_object().expect("record.ingested payload must be a JSON object");
    for field in ["id", "connector_id", "source_type", "ingested_at", "raw_payload", "tenant_id"] {
        assert!(
            obj.contains_key(field),
            "record.ingested payload missing required field `{field}`"
        );
    }
}

#[test]
fn record_ingested_message_round_trips_back_into_a_raw_record() {
    let record =
        RawRecord::new("graph:mail", SourceType::Message, Uuid::new_v4(), json!({"body": "hi"}));
    let message = serde_json::to_vec(&record).unwrap();

    let deserialized: RawRecord = serde_json::from_slice(&message)
        .expect("consumers must be able to deserialize the published payload");
    assert_eq!(deserialized, record);
}
