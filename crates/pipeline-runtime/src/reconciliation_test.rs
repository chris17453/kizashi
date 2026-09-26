use super::*;

#[test]
fn confirmation_requires_authoritative_version_and_evidence() {
    let value = serde_json::json!({"reconciliation":{"object_id":uuid::Uuid::new_v4(),"object_type_id":uuid::Uuid::new_v4(),"source_version":"42","properties":{"status":"approved"},"evidence":{"uri":"s3://evidence/42"}}});
    assert!(parse_projection(&value).is_ok());
}
