use super::*;

fn sample_payload() -> EmailPayload {
    EmailPayload {
        subject: "printer on fire".to_string(),
        from: "alice@example.com".to_string(),
        to: vec!["support@example.com".to_string()],
        cc: vec![],
        bcc: vec![],
        body: "please send help".to_string(),
        headers: std::collections::BTreeMap::new(),
        attachments: vec![EmailAttachment {
            filename: "photo.jpg".to_string(),
            content_type: "image/jpeg".to_string(),
            size_bytes: 1024,
            storage_key: Some("s3://kizashi-archive/attachments/photo.jpg".to_string()),
        }],
    }
}

#[test]
fn serializes_and_round_trips_through_json() {
    let payload = sample_payload();
    let json = serde_json::to_value(&payload).unwrap();
    let round_tripped: EmailPayload = serde_json::from_value(json).unwrap();
    assert_eq!(round_tripped, payload);
}

#[test]
fn deserializes_with_missing_optional_fields_defaulting_to_empty() {
    let json = serde_json::json!({
        "subject": "hi",
        "from": "bob@example.com",
        "body": "hello"
    });
    let payload: EmailPayload = serde_json::from_value(json).unwrap();
    assert!(payload.to.is_empty());
    assert!(payload.cc.is_empty());
    assert!(payload.attachments.is_empty());
}
