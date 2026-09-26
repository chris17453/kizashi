use super::*;
use serde_json::json;

#[test]
fn encrypted_blob_serializes_as_base64_string() {
    let blob = EncryptedBlob(vec![0, 1, 2, 250, 255]);
    assert_eq!(serde_json::to_value(&blob).unwrap(), json!("AAEC+v8="));
}

#[test]
fn encrypted_blob_round_trips() {
    let blob = EncryptedBlob((0..=255).collect());
    let text = serde_json::to_string(&blob).unwrap();
    assert_eq!(serde_json::from_str::<EncryptedBlob>(&text).unwrap(), blob);
}

#[test]
fn encrypted_blob_rejects_invalid_base64_and_non_strings() {
    assert!(serde_json::from_value::<EncryptedBlob>(json!("not base64!!")).is_err());
    assert!(serde_json::from_value::<EncryptedBlob>(json!([1, 2, 3])).is_err());
}

#[test]
fn provenance_default_is_empty() {
    let p = Provenance::default();
    assert!(p.source_item_ids.is_empty());
    assert_eq!(p.model_version, None);
    assert_eq!(p.encrypted, None);
}

#[test]
fn contribution_round_trips() {
    let c = Contribution { signal_id: SignalId::new(), weight: 0.4, value: -1.2 };
    let text = serde_json::to_string(&c).unwrap();
    assert_eq!(serde_json::from_str::<Contribution>(&text).unwrap(), c);
}
