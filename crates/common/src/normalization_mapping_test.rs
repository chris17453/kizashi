use super::*;
use proptest::prelude::*;
use serde_json::json;

fn mapping(field_map: &[(&str, &str)]) -> NormalizationMapping {
    NormalizationMapping::new(
        Uuid::new_v4(),
        "ticket",
        field_map.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    )
}

#[test]
fn apply_resolves_top_level_and_nested_fields() {
    let m = mapping(&[("text", "$.description"), ("entity_ref", "$.requester.id")]);
    let raw = json!({"description": "printer on fire", "requester": {"id": "cust-1"}});

    let normalized = m.apply(&raw);
    assert_eq!(normalized["text"], json!("printer on fire"));
    assert_eq!(normalized["entity_ref"], json!("cust-1"));
}

#[test]
fn apply_yields_null_for_missing_source_field_without_panicking() {
    let m = mapping(&[("text", "$.does_not_exist")]);
    let raw = json!({"description": "hi"});

    let normalized = m.apply(&raw);
    assert_eq!(normalized["text"], serde_json::Value::Null);
}

#[test]
fn apply_handles_root_path() {
    let m = mapping(&[("whole", "$")]);
    let raw = json!({"a": 1});
    let normalized = m.apply(&raw);
    assert_eq!(normalized["whole"], raw);
}

#[test]
fn new_starts_at_version_one() {
    let m = mapping(&[("text", "$.description")]);
    assert_eq!(m.version, 1);
}

proptest! {
    // NormalizationMapping is operator-authored config-as-data (spec §2 principle 5); a
    // malformed source_path or payload shape must never panic the normalization pipeline.
    #[test]
    fn apply_never_panics_on_arbitrary_path_and_payload(
        path in "\\PC{0,40}",
        payload_key in "[a-z]{1,10}",
        payload_val in "\\PC{0,20}",
    ) {
        let m = mapping(&[("dest", path.as_str())]);
        let raw = json!({ payload_key: payload_val });
        let _ = m.apply(&raw);
    }
}
