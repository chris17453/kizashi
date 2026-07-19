//! Integration test against a real IMAP server (CLAUDE.md §2 — "no vendor lock-in, self-hosted
//! deps... test against the real thing"). Requires IMAP_TEST_HOST/IMAP_TEST_PORT pointing at a
//! real IMAP server (a greenmail container in CI/local dev — see docs/features.md for the exact
//! `docker run` invocation used to verify this connector) with a mailbox seeded with at least
//! one message, plus IMAP_TEST_USERNAME/IMAP_TEST_PASSWORD credentials for it.

use common::connector::Connector;
use connector_imap::ImapConnector;

struct TestConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
}

fn test_config() -> TestConfig {
    TestConfig {
        host: std::env::var("IMAP_TEST_HOST").expect("IMAP_TEST_HOST must be set to run this test"),
        port: std::env::var("IMAP_TEST_PORT")
            .expect("IMAP_TEST_PORT must be set to run this test")
            .parse()
            .expect("IMAP_TEST_PORT must be a port number"),
        username: std::env::var("IMAP_TEST_USERNAME")
            .expect("IMAP_TEST_USERNAME must be set to run this test"),
        password: std::env::var("IMAP_TEST_PASSWORD")
            .expect("IMAP_TEST_PASSWORD must be set to run this test"),
    }
}

#[tokio::test]
async fn polls_a_real_imap_server_and_returns_the_seeded_message_as_a_raw_record() {
    let cfg = test_config();
    let tenant_id = uuid::Uuid::new_v4();

    let connector = ImapConnector::new(
        "imap-live-test",
        cfg.host,
        cfg.port,
        cfg.username,
        cfg.password,
        "INBOX",
        chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        false, // greenmail's test server doesn't present a TLS cert
    );

    let records = connector.poll(tenant_id).await.expect("poll against a real IMAP server failed");

    assert!(!records.is_empty(), "expected at least one seeded message in the mailbox");
    let record = &records[0];
    assert_eq!(record.connector_id, "imap-live-test");
    assert_eq!(record.tenant_id, tenant_id);
    assert_eq!(record.raw_payload["subject"], "Live IMAP connector test");
    assert_eq!(record.raw_payload["from"], "sender@example.com");
}

#[tokio::test]
async fn a_wrong_password_against_a_real_imap_server_is_reported_as_auth_failed() {
    let cfg = test_config();
    let tenant_id = uuid::Uuid::new_v4();

    let connector = ImapConnector::new(
        "imap-live-test",
        cfg.host,
        cfg.port,
        cfg.username,
        "definitely-the-wrong-password",
        "INBOX",
        chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        false,
    );

    let err = connector.poll(tenant_id).await.unwrap_err();
    assert!(matches!(err, common::connector::ConnectorError::AuthFailed(_)));
}
