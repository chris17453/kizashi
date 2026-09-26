#[path = "dek_test.rs"]
#[cfg(test)]
mod dek_test;

use crate::CryptoError;
use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::OsRng;
use std::fmt;
use zeroize::Zeroize;

macro_rules! secret_key_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        ///
        /// Key bytes are zeroed on drop and never rendered by `Debug`.
        #[derive(Clone)]
        pub struct $name([u8; 32]);

        impl $name {
            pub fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub fn from_slice(bytes: &[u8]) -> Result<Self, CryptoError> {
                let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::InvalidKeyLength)?;
                Ok(Self(arr))
            }

            pub fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                self.0.zeroize();
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}(<redacted>)", stringify!($name))
            }
        }
    };
}

secret_key_type!(
    Dek,
    "A per-subject AES-256 data-encryption key, in plaintext form (memory only)."
);
secret_key_type!(IndexKey, "A per-tenant HMAC-SHA256 key for blind indexes; never a DEK.");

impl Dek {
    /// A fresh random DEK from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }
}
