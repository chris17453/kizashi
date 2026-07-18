use super::*;
use serde_json::json;

fn sample_payload() -> serde_json::Value {
    json!({"subject": "help", "body": "my printer is on fire"})
}

#[test]
fn new_sets_defaults_and_generates_id() {
    let tenant_id = Uuid::new_v4();
    let record = RawRecord::new("zendesk", SourceType::Ticket, tenant_id, sample_payload());

    assert_ne!(record.id, Uuid::nil());
    assert_eq!(record.connector_id, "zendesk");
    assert_eq!(record.source_type, SourceType::Ticket);
    assert_eq!(record.tenant_id, tenant_id);
    assert_eq!(record.raw_payload, sample_payload());
    assert!(record.occurred_at.is_none());
    assert!(record.normalized_payload.is_none());
    assert!(!record.is_normalized());
}

#[test]
fn is_normalized_true_once_normalized_payload_is_set() {
    let tenant_id = Uuid::new_v4();
    let mut record = RawRecord::new("zendesk", SourceType::Ticket, tenant_id, sample_payload());
    record.normalized_payload = Some(json!({"text": "my printer is on fire"}));
    assert!(record.is_normalized());
}

#[test]
fn source_type_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&SourceType::SqlRow).unwrap(), "\"sql_row\"");
    assert_eq!(serde_json::to_string(&SourceType::FabricRecord).unwrap(), "\"fabric_record\"");
    assert_eq!(serde_json::to_string(&SourceType::Message).unwrap(), "\"message\"");
}

#[test]
fn round_trips_through_json_with_optional_fields_omitted() {
    let tenant_id = Uuid::new_v4();
    let record = RawRecord::new("graph:mail", SourceType::Message, tenant_id, sample_payload());

    let serialized = serde_json::to_string(&record).unwrap();
    assert!(!serialized.contains("occurred_at"));
    assert!(!serialized.contains("normalized_payload"));

    let deserialized: RawRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, record);
}

#[test]
fn round_trips_with_all_fields_populated() {
    let tenant_id = Uuid::new_v4();
    let mut record = RawRecord::new("sql:crm", SourceType::SqlRow, tenant_id, sample_payload());
    record.occurred_at = Some(Utc::now());
    record.normalized_payload = Some(json!({"entity_ref": "cust-123"}));

    let serialized = serde_json::to_string(&record).unwrap();
    let deserialized: RawRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, record);
}
