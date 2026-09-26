use crate::{CryptoError, Dek, IndexKey};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Opaque identifier of a tenant key-encryption key, as understood by the provider that issued
/// it (e.g. `local:v1:<tenant>` or, later, a Key Vault key URI). Stored next to every wrapped DEK
/// so a provider can find the right KEK — and so KEK rotation can co-exist with old rows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KekId(String);

impl KekId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Key custody boundary (ADR-0202): wraps and unwraps per-subject DEKs under a tenant KEK that
/// never leaves the provider, and hands out per-tenant blind-index keys.
///
/// Implementations: [`crate::LocalKeyProvider`] (dev/CI/non-Azure). Azure Key Vault is a
/// planned follow-up implementation of this same trait.
#[async_trait]
pub trait KeyProvider: Send + Sync {
    /// Creates the tenant's KEK if it does not exist and returns its id. Idempotent.
    async fn create_tenant_kek(&self, tenant_id: Uuid) -> Result<KekId, CryptoError>;

    /// Wraps `dek` under `kek_id`. Fails with [`CryptoError::KekMismatch`] if the KEK is not the
    /// tenant's.
    async fn wrap_dek(
        &self,
        tenant_id: Uuid,
        kek_id: &KekId,
        dek: &Dek,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Unwraps a DEK previously returned by [`KeyProvider::wrap_dek`] for the same tenant/KEK.
    async fn unwrap_dek(
        &self,
        tenant_id: Uuid,
        kek_id: &KekId,
        wrapped: &[u8],
    ) -> Result<Dek, CryptoError>;

    /// The tenant's blind-index HMAC key. Stable across calls; distinct per tenant.
    async fn tenant_index_key(&self, tenant_id: Uuid) -> Result<IndexKey, CryptoError>;
}
