#[path = "error_test.rs"]
#[cfg(test)]
mod error_test;

use chrono::{DateTime, Utc};
use thiserror::Error;

/// Every failure mode of `yocho-crypto`, typed so callers can tell "this subject was erased"
/// apart from "the ciphertext is corrupt" apart from "the key store is down".
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("master key is not configured (set YOCHO_MASTER_KEY)")]
    MissingMasterKey,
    #[error("master key must be base64 of exactly 32 bytes")]
    InvalidMasterKey,
    #[error("key material must be exactly 32 bytes")]
    InvalidKeyLength,
    #[error("KEK id does not belong to this tenant/provider")]
    KekMismatch,
    #[error("wrapped DEK could not be unwrapped")]
    UnwrapFailed,
    #[error("failed to encrypt")]
    EncryptFailed,
    #[error("ciphertext is truncated or malformed")]
    MalformedCiphertext,
    #[error("unsupported ciphertext format version {0}")]
    UnsupportedVersion(u8),
    #[error("ciphertext failed authentication (wrong key, subject, tenant or tampered)")]
    DecryptFailed,
    #[error("no key exists for this subject")]
    KeyNotFound,
    #[error("subject was crypto-shredded at {shredded_at}")]
    SubjectShredded { shredded_at: DateTime<Utc> },
    #[error("key store error: {0}")]
    Store(String),
}

impl From<sqlx::Error> for CryptoError {
    fn from(err: sqlx::Error) -> Self {
        CryptoError::Store(err.to_string())
    }
}
