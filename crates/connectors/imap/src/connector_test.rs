use super::*;

fn sample_connector() -> ImapConnector {
    ImapConnector::new(
        "imap-inbox",
        "imap.example.com",
        993,
        "user@example.com",
        "secret",
        "INBOX",
        chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        true,
    )
}

#[test]
fn reports_its_own_connector_id_and_source_type() {
    let c = sample_connector();
    assert_eq!(c.connector_id(), "imap-inbox");
    assert_eq!(c.source_type(), SourceType::Message);
}
