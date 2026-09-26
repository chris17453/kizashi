//! `yocho-crypto` — per-subject envelope encryption, blind indexes and crypto-shredding
//! (ADR-0204), behind the `KeyProvider` trait from ADR-0202.
//!
//! Platform-generic by design: a subject is `(tenant_id, subject_type, subject_id)` and nothing
//! here knows what a mailbox, contact or account is.
//!
//! - [`KeyProvider`] wraps/unwraps per-subject data-encryption keys (DEKs) under a tenant
//!   key-encryption key (KEK). [`LocalKeyProvider`] derives KEKs from a master key supplied via
//!   env/config, for dev, CI and non-Azure deployments.
//!   TODO(ADR-0202): an Azure Key Vault `KeyProvider` is the first production implementation and
//!   is a follow-up PR; it slots in behind the same trait without touching callers.
//! - [`envelope`] seals field bytes with AES-256-GCM, a random nonce, a version byte and AAD
//!   that binds the ciphertext to its tenant and subject.
//! - [`DekCache`] keeps unwrapped DEKs in memory only, TTL-bounded and evictable per subject.
//! - [`BlindIndexer`] computes HMAC-SHA256 lookup tokens under a per-tenant index key that is
//!   never a DEK.
//! - [`SubjectCrypto::shred`] destroys a subject's wrapped DEK (leaving a tombstone row), writes
//!   an immutable audit row, evicts the cache and returns the `subject.shredded` event.

pub mod blind_index;
pub mod dek;
pub mod dek_cache;
pub mod envelope;
pub mod error;
pub mod key_provider;
pub mod key_store;
pub mod local_key_provider;
pub mod memory_key_store;
pub mod pg_key_store;
pub mod subject;
pub mod subject_crypto;
pub mod subject_shredded;

pub use blind_index::{normalize_email, BlindIndex, BlindIndexer};
pub use dek::{Dek, IndexKey};
pub use dek_cache::DekCache;
pub use error::CryptoError;
pub use key_provider::{KekId, KeyProvider};
pub use key_store::{
    KeyAuditAction, KeyAuditEntry, ShredResult, StoredSubjectKey, SubjectKeyStore,
};
pub use local_key_provider::{LocalKeyProvider, MASTER_KEY_ENV};
pub use memory_key_store::InMemorySubjectKeyStore;
pub use pg_key_store::{migrator, PgSubjectKeyStore};
pub use subject::SubjectRef;
pub use subject_crypto::SubjectCrypto;
pub use subject_shredded::{
    SubjectShredded, SUBJECT_SHREDDED_EXCHANGE, SUBJECT_SHREDDED_SCHEMA_VERSION,
};
