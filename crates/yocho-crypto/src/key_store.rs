//! Persistence of wrapped subject DEKs plus their immutable audit trail.
//!
//! Shred policy (ADR-0204 §5, choice documented here): shredding **nulls the wrapped DEK and
//! keeps a tombstone row** with `shredded_at`, rather than deleting the row. The tombstone lets
//! decrypt report [`crate::CryptoError::SubjectShredded`] instead of an ambiguous "not found",
//! and blocks silently minting a fresh key for an erased subject (re-ingestion after erasure must
//! be an explicit, separately-audited decision). A tombstone holds no key material. The audit
//! table is append-only regardless.

use crate::{CryptoError, KekId, SubjectRef};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSubjectKey {
    pub subject: SubjectRef,
    /// `None` once shredded.
    pub wrapped_dek: Option<Vec<u8>>,
    /// `None` only for a tombstone written for a subject that never had a key.
    pub kek_id: Option<KekId>,
    pub created_at: DateTime<Utc>,
    pub shredded_at: Option<DateTime<Utc>>,
}

impl StoredSubjectKey {
    pub fn is_shredded(&self) -> bool {
        self.shredded_at.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShredResult {
    /// When the subject was (first) shredded.
    pub shredded_at: DateTime<Utc>,
    /// `false` if the subject was already shredded before this call.
    pub newly_shredded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAuditAction {
    Created,
    Shredded,
    ShredRepeated,
}

impl KeyAuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyAuditAction::Created => "created",
            KeyAuditAction::Shredded => "shredded",
            KeyAuditAction::ShredRepeated => "shred_repeated",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "created" => Some(KeyAuditAction::Created),
            "shredded" => Some(KeyAuditAction::Shredded),
            "shred_repeated" => Some(KeyAuditAction::ShredRepeated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAuditEntry {
    pub id: Uuid,
    pub subject: SubjectRef,
    pub action: KeyAuditAction,
    pub kek_id: Option<KekId>,
    pub actor: String,
    pub occurred_at: DateTime<Utc>,
}

/// Every mutation writes an audit row in the same transaction.
#[async_trait]
pub trait SubjectKeyStore: Send + Sync {
    async fn get(&self, subject: &SubjectRef) -> Result<Option<StoredSubjectKey>, CryptoError>;

    /// Inserts a wrapped DEK unless a row (live or tombstone) already exists, and returns the row
    /// that is now stored — the caller's on insert, the existing one otherwise.
    async fn insert_if_absent(
        &self,
        subject: &SubjectRef,
        wrapped_dek: &[u8],
        kek_id: &KekId,
        actor: &str,
    ) -> Result<StoredSubjectKey, CryptoError>;

    /// Destroys the wrapped DEK, leaving a tombstone (created if the subject had no key).
    /// Idempotent; every call is audited.
    async fn shred(&self, subject: &SubjectRef, actor: &str) -> Result<ShredResult, CryptoError>;

    /// Audit trail for one subject, oldest first.
    async fn audit_log(&self, subject: &SubjectRef) -> Result<Vec<KeyAuditEntry>, CryptoError>;
}
