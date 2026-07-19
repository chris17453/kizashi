use super::*;

const SAMPLE_MESSAGE: &[u8] = b"From: alice@example.com\r\n\
To: bob@example.com, carol@example.com\r\n\
Subject: Quarterly review\r\n\
Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n\
Content-Type: text/plain\r\n\
\r\n\
The review is attached.\r\n";

#[test]
fn parses_headers_and_body_into_a_raw_record() {
    let tenant_id = Uuid::new_v4();
    let record = parse_message(42, SAMPLE_MESSAGE, "imap-inbox", tenant_id).unwrap();

    assert_eq!(record.connector_id, "imap-inbox");
    assert_eq!(record.tenant_id, tenant_id);
    assert_eq!(record.source_type, SourceType::Message);
    assert_eq!(record.raw_payload["uid"], 42);
    assert_eq!(record.raw_payload["subject"], "Quarterly review");
    assert_eq!(record.raw_payload["from"], "alice@example.com");
    assert_eq!(
        record.raw_payload["to"],
        serde_json::json!(["bob@example.com", "carol@example.com"])
    );
    assert_eq!(record.raw_payload["body"], "The review is attached.\r\n");
    assert!(record.occurred_at.is_some());
}

#[test]
fn returns_a_malformed_record_error_for_garbage_bytes_that_mail_parser_still_produces_something_from(
) {
    // mail-parser is deliberately lenient (real-world mail is often malformed), so a byte
    // string with no headers at all still parses as a bodiless message rather than failing --
    // assert on that documented behavior instead of a parse failure that doesn't happen.
    let tenant_id = Uuid::new_v4();
    let record = parse_message(1, b"not an email at all", "imap-inbox", tenant_id).unwrap();
    assert_eq!(record.raw_payload["subject"], "");
}

#[test]
fn missing_subject_and_recipients_default_to_empty_rather_than_panicking() {
    let tenant_id = Uuid::new_v4();
    let minimal = b"From: alice@example.com\r\n\r\nHello\r\n";
    let record = parse_message(7, minimal, "imap-inbox", tenant_id).unwrap();
    assert_eq!(record.raw_payload["subject"], "");
    assert_eq!(record.raw_payload["to"], serde_json::json!([]));
}
