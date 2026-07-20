#[path = "encryption_test.rs"]
#[cfg(test)]
mod encryption_test;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as base64_engine;
use base64::Engine;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("encryption key must be exactly 32 bytes")]
    InvalidKeyLength,
    #[error("failed to encrypt value")]
    EncryptFailed,
    #[error("stored ciphertext is malformed or the key is wrong")]
    DecryptFailed,
}

/// AES-256-GCM at-rest encryption for `analysis_configs.api_key` (ADR-0058) — a tenant's AI
/// provider credential was previously stored in plaintext, a real gap for a product audited by
/// customers' compliance teams (flagged when ADR-0031 shipped, closed here). Nonce is generated
/// fresh per encryption and stored alongside the ciphertext (nonce || ciphertext, base64-encoded
/// for the existing TEXT column) — GCM nonces must never repeat under the same key, so it can't
/// be derived from anything static.
pub struct ApiKeyEncryptor {
    key: [u8; 32],
}

impl ApiKeyEncryptor {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Parses a 32-byte key from a base64-encoded env var value — the wire format
    /// `CONFIG_ENCRYPTION_KEY` is expected to hold.
    pub fn from_base64(encoded: &str) -> Result<Self, EncryptionError> {
        let bytes =
            base64_engine.decode(encoded.trim()).map_err(|_| EncryptionError::InvalidKeyLength)?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| EncryptionError::InvalidKeyLength)?;
        Ok(Self::new(key))
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| EncryptionError::EncryptFailed)?;

        let mut combined = Vec::with_capacity(nonce.len() + ciphertext.len());
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(&ciphertext);
        Ok(base64_engine.encode(combined))
    }

    pub fn decrypt(&self, stored: &str) -> Result<String, EncryptionError> {
        let combined = base64_engine.decode(stored).map_err(|_| EncryptionError::DecryptFailed)?;
        if combined.len() < 12 {
            return Err(EncryptionError::DecryptFailed);
        }
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let plaintext =
            cipher.decrypt(nonce, ciphertext).map_err(|_| EncryptionError::DecryptFailed)?;
        String::from_utf8(plaintext).map_err(|_| EncryptionError::DecryptFailed)
    }
}
