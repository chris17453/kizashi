//! Dev/CI/non-Azure [`KeyProvider`] (ADR-0202): tenant KEKs and index keys are derived from one
//! 32-byte master key with HMAC-SHA256 under distinct labels, so nothing but the master key has
//! to be stored. The master key comes from `YOCHO_MASTER_KEY` (base64) or config — it is never
//! hard-coded, and losing it makes every wrapped DEK unrecoverable (ADR-0204 consequences).
//!
//! Limitations, deliberately accepted for a local provider: one KEK version per tenant
//! (`local:v1:<tenant>`); rotating the master key requires re-wrapping every DEK. A managed
//! provider (Azure Key Vault, follow-up) handles rotation natively.

#[path = "local_key_provider_test.rs"]
#[cfg(test)]
mod local_key_provider_test;

use crate::{CryptoError, Dek, IndexKey, KekId, KeyProvider};
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as base64_engine;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const MASTER_KEY_ENV: &str = "YOCHO_MASTER_KEY";
pub(crate) const WRAP_VERSION_V1: u8 = 0x01;
const KEK_LABEL: &[u8] = b"yocho/local/kek/v1/";
const INDEX_LABEL: &[u8] = b"yocho/local/index/v1/";
const NONCE_LEN: usize = 12;

pub struct LocalKeyProvider {
    master: Zeroizing<[u8; 32]>,
}

impl LocalKeyProvider {
    pub fn new(master: [u8; 32]) -> Self {
        Self { master: Zeroizing::new(master) }
    }

    pub fn from_base64(encoded: &str) -> Result<Self, CryptoError> {
        let bytes = Zeroizing::new(
            base64_engine.decode(encoded.trim()).map_err(|_| CryptoError::InvalidMasterKey)?,
        );
        let key: [u8; 32] =
            bytes.as_slice().try_into().map_err(|_| CryptoError::InvalidMasterKey)?;
        Ok(Self::new(key))
    }

    /// Reads the master key from [`MASTER_KEY_ENV`].
    pub fn from_env() -> Result<Self, CryptoError> {
        Self::from_env_value(std::env::var(MASTER_KEY_ENV).ok())
    }

    pub(crate) fn from_env_value(value: Option<String>) -> Result<Self, CryptoError> {
        match value {
            Some(v) if !v.trim().is_empty() => Self::from_base64(&v),
            _ => Err(CryptoError::MissingMasterKey),
        }
    }

    fn kek_id_for(tenant_id: Uuid) -> KekId {
        KekId::new(format!("local:v1:{tenant_id}"))
    }

    fn derive(&self, label: &[u8], tenant_id: Uuid) -> [u8; 32] {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.master.as_slice())
            .expect("HMAC accepts any key length");
        mac.update(label);
        mac.update(tenant_id.as_bytes());
        mac.finalize().into_bytes().into()
    }

    pub(crate) fn derive_kek(&self, tenant_id: Uuid) -> Dek {
        Dek::from_bytes(self.derive(KEK_LABEL, tenant_id))
    }

    fn check_kek(tenant_id: Uuid, kek_id: &KekId) -> Result<(), CryptoError> {
        if *kek_id == Self::kek_id_for(tenant_id) {
            Ok(())
        } else {
            Err(CryptoError::KekMismatch)
        }
    }
}

#[async_trait]
impl KeyProvider for LocalKeyProvider {
    async fn create_tenant_kek(&self, tenant_id: Uuid) -> Result<KekId, CryptoError> {
        Ok(Self::kek_id_for(tenant_id))
    }

    async fn wrap_dek(
        &self,
        tenant_id: Uuid,
        kek_id: &KekId,
        dek: &Dek,
    ) -> Result<Vec<u8>, CryptoError> {
        Self::check_kek(tenant_id, kek_id)?;
        let kek = self.derive_kek(tenant_id);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(kek.as_bytes()));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let aad = wrap_aad(kek_id);
        let ct = cipher
            .encrypt(&nonce, Payload { msg: dek.as_bytes(), aad: &aad })
            .map_err(|_| CryptoError::EncryptFailed)?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(WRAP_VERSION_V1);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    async fn unwrap_dek(
        &self,
        tenant_id: Uuid,
        kek_id: &KekId,
        wrapped: &[u8],
    ) -> Result<Dek, CryptoError> {
        Self::check_kek(tenant_id, kek_id)?;
        let (&version, rest) = wrapped.split_first().ok_or(CryptoError::UnwrapFailed)?;
        if version != WRAP_VERSION_V1 || rest.len() < NONCE_LEN {
            return Err(CryptoError::UnwrapFailed);
        }
        let (nonce, ct) = rest.split_at(NONCE_LEN);
        let kek = self.derive_kek(tenant_id);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(kek.as_bytes()));
        let aad = wrap_aad(kek_id);
        let plain = Zeroizing::new(
            cipher
                .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: &aad })
                .map_err(|_| CryptoError::UnwrapFailed)?,
        );
        Dek::from_slice(&plain).map_err(|_| CryptoError::UnwrapFailed)
    }

    async fn tenant_index_key(&self, tenant_id: Uuid) -> Result<IndexKey, CryptoError> {
        Ok(IndexKey::from_bytes(self.derive(INDEX_LABEL, tenant_id)))
    }
}

fn wrap_aad(kek_id: &KekId) -> Vec<u8> {
    let mut aad = b"yocho/local/wrap/v1/".to_vec();
    aad.extend_from_slice(kek_id.as_str().as_bytes());
    aad
}
