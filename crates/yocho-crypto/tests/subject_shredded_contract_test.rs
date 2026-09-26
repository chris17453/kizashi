//! Contract test for the `subject.shredded` bus message (CLAUDE.md §2), so the producer
//! (`SubjectCrypto::shred`) and consumers (cache eviction, hard-delete jobs) cannot drift.

use serde_json::json;
use uuid::Uuid;
use yocho_crypto::{SubjectRef, SubjectShredded, SUBJECT_SHREDDED_EXCHANGE};

fn sample() -> SubjectShredded {
    SubjectShredded::new(
        &SubjectRef::new(Uuid::new_v4(), "contact", "c-1"),
        chrono::Utc::now(),
        "user:dpo",
    )
}

#[test]
fn exchange_name_is_subject_shredded() {
    assert_eq!(SUBJECT_SHREDDED_EXCHANGE, "subject.shredded");
}

#[test]
fn message_has_exactly_the_required_fields() {
    let value = serde_json::to_value(sample()).unwrap();
    let obj = value.as_object().expect("subject.shredded payload must be a JSON object");
    let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "actor",
            "id",
            "schema_version",
            "shredded_at",
            "subject_id",
            "subject_type",
            "tenant_id"
        ]
    );
    assert_eq!(obj["schema_version"], json!(1));
}

#[test]
fn message_carries_no_key_material() {
    let raw = serde_json::to_string(&sample()).unwrap();
    for forbidden in ["dek", "wrapped", "kek", "key"] {
        assert!(!raw.to_lowercase().contains(forbidden), "payload mentions `{forbidden}`: {raw}");
    }
}

#[test]
fn message_round_trips() {
    let event = sample();
    let bytes = serde_json::to_vec(&event).unwrap();
    let back: SubjectShredded = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(back, event);
}

#[test]
fn consumer_can_decode_a_literal_v1_payload() {
    let tenant = Uuid::new_v4();
    let payload = json!({
        "id": Uuid::new_v4(),
        "schema_version": 1,
        "tenant_id": tenant,
        "subject_type": "mailbox",
        "subject_id": "mbx-7",
        "shredded_at": "2026-09-26T12:00:00Z",
        "actor": "user:dpo"
    });
    let event: SubjectShredded = serde_json::from_value(payload).unwrap();
    assert_eq!(event.subject(), SubjectRef::new(tenant, "mailbox", "mbx-7"));
}
