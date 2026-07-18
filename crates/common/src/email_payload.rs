#[path = "email_payload_test.rs"]
#[cfg(test)]
mod email_payload_test;

use serde::{Deserialize, Serialize};

/// Attachment metadata only — never the attachment's own bytes. A large binary has no business
/// living inline in a JSONB column; connectors that need the content itself store it in MinIO/S3
/// (same object store retention-service already archives into) and put a reference here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmailAttachment {
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
}

/// The documented `raw_payload` shape for `SourceType::Message` records that originate from an
/// email connector (Graph Mail, and the planned IMAP connector) — nothing enforces this at the
/// database level (`raw_payload` stays plain JSONB, per spec §2 principle 2's "schema never
/// changes at the envelope level"), but Ingestion Service's structured search (subject/from/
/// attachment filters) reads these exact field names, so a connector that wants to be
/// searchable this way needs to match it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmailPayload {
    pub subject: String,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub body: String,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub attachments: Vec<EmailAttachment>,
}
