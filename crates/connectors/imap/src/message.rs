#[path = "message_test.rs"]
#[cfg(test)]
mod message_test;

use common::connector::ConnectorError;
use common::raw_record::{RawRecord, SourceType};
use mail_parser::MessageParser;
use uuid::Uuid;

/// Parses one message's raw RFC822 bytes (as returned by IMAP `FETCH ... (RFC822)`) into a
/// `RawRecord`. Kept as a pure function, separate from `ImapConnector::poll`'s network I/O, so
/// the mapping logic is unit-testable against static byte fixtures without a real IMAP server —
/// the network path itself is proven by the live integration test instead (CLAUDE.md §2).
pub fn parse_message(
    uid: u32,
    raw_rfc822: &[u8],
    connector_id: &str,
    tenant_id: Uuid,
) -> Result<RawRecord, ConnectorError> {
    let message = MessageParser::default().parse(raw_rfc822).ok_or_else(|| {
        ConnectorError::MalformedRecord(format!("uid {uid}: not a valid RFC822 message"))
    })?;

    let subject = message.subject().unwrap_or_default().to_string();
    let from = message
        .from()
        .and_then(|addr| addr.first())
        .and_then(|a| a.address())
        .unwrap_or_default()
        .to_string();
    let to: Vec<String> = message
        .to()
        .map(|addr| {
            addr.iter().filter_map(|a| a.address().map(|s| s.to_string())).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let body = message
        .body_text(0)
        .map(|s| s.to_string())
        .or_else(|| message.body_html(0).map(|s| s.to_string()))
        .unwrap_or_default();
    let occurred_at = message.date().map(|d| {
        chrono::DateTime::from_timestamp(d.to_timestamp(), 0).unwrap_or_else(chrono::Utc::now)
    });

    // Message-Id is globally stable across re-polls of the same message (RFC 5322 §3.6.4),
    // which is what makes idempotent re-ingestion possible when IMAP's date-only `SINCE`
    // search re-scans an overlapping window on every poll. Falling back to
    // "{connector_id}:{uid}" when a message has no Message-Id header still dedupes correctly
    // within one mailbox, since IMAP UIDs are unique and non-reused within a mailbox.
    let external_id = message
        .message_id()
        .map(|id| format!("<{id}>"))
        .unwrap_or_else(|| format!("{connector_id}:{uid}"));

    let payload = serde_json::json!({
        "uid": uid,
        "subject": subject,
        "from": from,
        "to": to,
        "body": body,
    });

    let mut record = RawRecord::new(connector_id, SourceType::Message, tenant_id, payload)
        .with_external_id(external_id);
    record.occurred_at = occurred_at;
    Ok(record)
}
