use super::*;
use serde_json::json;

#[test]
fn empty_dedup_fields_yields_no_fingerprint() {
    let payload = json!({"text": "printer on fire"});
    assert_eq!(compute_fingerprint(&[], &payload), None);
}

#[test]
fn identical_payloads_produce_the_same_fingerprint() {
    let dedup_fields = vec!["text".to_string(), "severity".to_string()];
    let payload = json!({"text": "printer on fire", "severity": "high", "ignored": "noise"});

    let a = compute_fingerprint(&dedup_fields, &payload);
    let b = compute_fingerprint(&dedup_fields, &payload);
    assert!(a.is_some());
    assert_eq!(a, b);
}

#[test]
fn a_different_value_in_a_dedup_field_changes_the_fingerprint() {
    let dedup_fields = vec!["text".to_string()];
    let a = compute_fingerprint(&dedup_fields, &json!({"text": "printer on fire"}));
    let b = compute_fingerprint(&dedup_fields, &json!({"text": "printer is fine"}));
    assert_ne!(a, b);
}

#[test]
fn a_different_value_in_a_non_dedup_field_does_not_change_the_fingerprint() {
    let dedup_fields = vec!["text".to_string()];
    let a = compute_fingerprint(&dedup_fields, &json!({"text": "printer on fire", "id": "1"}));
    let b = compute_fingerprint(&dedup_fields, &json!({"text": "printer on fire", "id": "2"}));
    assert_eq!(a, b);
}

#[test]
fn field_order_in_the_dedup_fields_list_does_not_affect_the_fingerprint() {
    let payload = json!({"text": "printer on fire", "severity": "high"});
    let a = compute_fingerprint(&["text".to_string(), "severity".to_string()], &payload);
    let b = compute_fingerprint(&["severity".to_string(), "text".to_string()], &payload);
    assert_eq!(a, b);
}

#[test]
fn a_missing_dedup_field_is_treated_as_null_without_panicking() {
    let dedup_fields = vec!["does_not_exist".to_string()];
    let fingerprint = compute_fingerprint(&dedup_fields, &json!({"text": "hi"}));
    assert!(fingerprint.is_some());
}
