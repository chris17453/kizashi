//! Orchestrates provider + store + cache into the API callers use: `encrypt`, `decrypt`,
//! `blind_indexer`, `shred`, `evict`.

#[path = "subject_crypto_test.rs"]
#[cfg(test)]
mod subject_crypto_test;

use crate::{
    envelope, BlindIndexer, CryptoError, Dek, DekCache, KeyProvider, StoredSubjectKey,
    SubjectKeyStore, SubjectRef, SubjectShredded,
};
use std::sync::Arc;
use uuid::Uuid;

/// Actor recorded on audit rows for keys created implicitly by the first `encrypt`.
pub const SYSTEM_ACTOR: &str = "system:yocho-crypto";

pub struct SubjectCrypto {
    provider: Arc<dyn KeyProvider>,
    store: Arc<dyn SubjectKeyStore>,
    cache: DekCache,
}

impl SubjectCrypto {
    pub fn new(
        provider: Arc<dyn KeyProvider>,
        store: Arc<dyn SubjectKeyStore>,
        cache: DekCache,
    ) -> Self {
        Self { provider, store, cache }
    }

    /// Seals `plaintext` for `subject`, creating the subject's DEK on first use.
    pub async fn encrypt(
        &self,
        subject: &SubjectRef,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let dek = self.load_dek(subject, true).await?;
        envelope::seal(&dek, subject, plaintext)
    }

    /// Opens an envelope sealed for `subject`. A shredded subject yields
    /// [`CryptoError::SubjectShredded`]; one that never had a key yields
    /// [`CryptoError::KeyNotFound`].
    pub async fn decrypt(
        &self,
        subject: &SubjectRef,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let dek = self.load_dek(subject, false).await?;
        envelope::open(&dek, subject, ciphertext)
    }

    pub async fn blind_indexer(&self, tenant_id: Uuid) -> Result<BlindIndexer, CryptoError> {
        Ok(BlindIndexer::new(self.provider.tenant_index_key(tenant_id).await?))
    }

    /// Crypto-shreds `subject`: destroys its wrapped DEK (tombstone + audit row in one
    /// transaction), evicts the local cache, and returns the `subject.shredded` event for the
    /// caller to publish so other nodes evict too.
    pub async fn shred(
        &self,
        subject: &SubjectRef,
        actor: &str,
    ) -> Result<SubjectShredded, CryptoError> {
        self.cache.evict(subject);
        let result = self.store.shred(subject, actor).await?;
        self.cache.evict(subject);
        Ok(SubjectShredded::new(subject, result.shredded_at, actor))
    }

    /// Drops a cached DEK — the handler for a `subject.shredded` event from another node.
    pub fn evict(&self, subject: &SubjectRef) {
        self.cache.evict(subject);
    }

    async fn load_dek(&self, subject: &SubjectRef, create: bool) -> Result<Arc<Dek>, CryptoError> {
        if let Some(dek) = self.cache.get(subject) {
            return Ok(dek);
        }
        let dek = match self.store.get(subject).await? {
            Some(row) => self.unwrap_row(&row).await?,
            None if create => self.create_dek(subject).await?,
            None => return Err(CryptoError::KeyNotFound),
        };
        let dek = Arc::new(dek);
        self.cache.insert(subject.clone(), dek.clone());
        Ok(dek)
    }

    async fn unwrap_row(&self, row: &StoredSubjectKey) -> Result<Dek, CryptoError> {
        if let Some(shredded_at) = row.shredded_at {
            return Err(CryptoError::SubjectShredded { shredded_at });
        }
        match (&row.wrapped_dek, &row.kek_id) {
            (Some(wrapped), Some(kek_id)) => {
                self.provider.unwrap_dek(row.subject.tenant_id, kek_id, wrapped).await
            }
            _ => Err(CryptoError::Store("live key row is missing its wrapped DEK".into())),
        }
    }

    async fn create_dek(&self, subject: &SubjectRef) -> Result<Dek, CryptoError> {
        let kek_id = self.provider.create_tenant_kek(subject.tenant_id).await?;
        let dek = Dek::generate();
        let wrapped = self.provider.wrap_dek(subject.tenant_id, &kek_id, &dek).await?;
        let stored = self.store.insert_if_absent(subject, &wrapped, &kek_id, SYSTEM_ACTOR).await?;
        if stored.wrapped_dek.as_deref() == Some(wrapped.as_slice()) {
            return Ok(dek);
        }
        // Lost a creation race (or hit a tombstone): use whatever is actually stored.
        self.unwrap_row(&stored).await
    }
}
