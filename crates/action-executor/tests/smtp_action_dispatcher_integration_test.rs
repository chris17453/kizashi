//! Integration test against a real SMTP server (CLAUDE.md §2) — reuses the same `greenmail`
//! container the IMAP connector's live test uses (ADR-0022/ADR-0023), since greenmail speaks
//! both SMTP and IMAP. Sends a real message via `SmtpActionDispatcher`, then reads it back via
//! a real `ImapConnector::poll` to prove delivery end-to-end, not just "the SMTP server
//! accepted it." Requires SMTP_TEST_HOST/SMTP_TEST_PORT/IMAP_TEST_PORT plus
//! IMAP_TEST_USERNAME/IMAP_TEST_PASSWORD pointing at that server.

use action_executor::{ActionDispatcher, SmtpActionDispatcher};
use common::connector::Connector;
use common::{ActionRef, ActionType, Event, EventStatus};
use connector_imap::ImapConnector;
use serde_json::json;

fn test_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set to run this test"))
}

fn sample_event() -> Event {
    Event {
        id: uuid::Uuid::new_v4(),
        tenant_id: uuid::Uuid::new_v4(),
        event_type: "sentiment_drop".to_string(),
        source_connector_ids: vec![],
        entity_ref: "cust-42".to_string(),
        group_key: "cust-42".to_string(),
        payload: json!({"score": -0.8}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: EventStatus::New,
        record_ids: vec![],
    }
}

#[tokio::test]
async fn a_real_smtp_send_is_actually_delivered_and_readable_via_imap() {
    let smtp_host = test_env("SMTP_TEST_HOST");
    let smtp_port: u16 = test_env("SMTP_TEST_PORT").parse().expect("SMTP_TEST_PORT must be a port");
    let imap_port: u16 = test_env("IMAP_TEST_PORT").parse().expect("IMAP_TEST_PORT must be a port");
    let imap_username = test_env("IMAP_TEST_USERNAME");
    let imap_password = test_env("IMAP_TEST_PASSWORD");

    let dispatcher = SmtpActionDispatcher::new();
    let action = ActionRef {
        action_type: ActionType::Email,
        config: json!({
            "smtp_host": smtp_host,
            "smtp_port": smtp_port,
            "smtp_use_tls": false,
            "from": "kizashi-alerts@example.com",
            "to": format!("{imap_username}@example.com"),
            "subject": "action-executor live SMTP test",
        }),
    };

    dispatcher.dispatch(&action, &sample_event()).await.expect("real SMTP send failed");

    let imap_connector = ImapConnector::new(
        "smtp-live-test-verifier",
        smtp_host,
        imap_port,
        imap_username,
        imap_password,
        "INBOX",
        chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        false,
    );
    let records =
        imap_connector.poll(uuid::Uuid::new_v4()).await.expect("failed to poll the IMAP mailbox");

    let found =
        records.iter().any(|r| r.raw_payload["subject"] == "action-executor live SMTP test");
    assert!(found, "expected to find the SMTP-sent message via IMAP, got: {records:?}");
}
