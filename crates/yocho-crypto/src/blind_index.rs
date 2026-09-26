//! Blind indexes (ADR-0204 §4): deterministic HMAC-SHA256 tokens for equality lookups on
//! encrypted fields, keyed by a per-tenant index key that is never a DEK. The same email in two
//! tenants yields unrelated tokens, and shredding a subject's DEK does not change its tokens —
//! callers must hard-delete index rows alongside the subject's Postgres rows.

#[path = "blind_index_test.rs"]
#[cfg(test)]
mod blind_index_test;

use crate::subject::push_len_prefixed;
use crate::IndexKey;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Canonical form of an email address for indexing: surrounding whitespace trimmed, lowercased.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlindIndex(pub [u8; 32]);

impl BlindIndex {
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

pub struct BlindIndexer {
    key: IndexKey,
}

impl BlindIndexer {
    pub fn new(key: IndexKey) -> Self {
        Self { key }
    }

    /// Token for `value` in the namespace `field` (so an email and a subject line with the same
    /// text never collide). `value` is used as given; normalise first.
    pub fn index(&self, field: &str, value: &str) -> BlindIndex {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.key.as_bytes())
            .expect("HMAC accepts any key length");
        let mut input = Vec::with_capacity(16 + field.len() + value.len());
        push_len_prefixed(&mut input, field.as_bytes());
        push_len_prefixed(&mut input, value.as_bytes());
        mac.update(&input);
        BlindIndex(mac.finalize().into_bytes().into())
    }

    pub fn index_email(&self, raw: &str) -> BlindIndex {
        self.index("email", &normalize_email(raw))
    }
}
