use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory test double keyed by hashed API key, mirroring how the real store never sees
/// plaintext keys at rest.
#[derive(Default)]
pub struct InMemoryApiKeyStore {
    pub keys_by_hash: Mutex<HashMap<String, Uuid>>,
    pub summaries: Mutex<Vec<ApiKeySummary>>,
}

impl InMemoryApiKeyStore {
    pub fn with_key(api_key: &str, tenant_id: Uuid) -> Self {
        let store = Self::default();
        store.keys_by_hash.lock().unwrap().insert(hash_api_key(api_key), tenant_id);
        store
    }
}

#[async_trait]
impl ApiKeyStore for InMemoryApiKeyStore {
    async fn tenant_for_key(&self, api_key: &str) -> Result<Option<Uuid>, ApiKeyStoreError> {
        Ok(self.keys_by_hash.lock().unwrap().get(&hash_api_key(api_key)).copied())
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        label: &str,
    ) -> Result<(ApiKeySummary, String), ApiKeyStoreError> {
        let plaintext = generate_api_key();
        let summary = ApiKeySummary {
            id: Uuid::new_v4(),
            tenant_id,
            label: label.to_string(),
            created_at: Utc::now(),
            revoked_at: None,
        };
        self.keys_by_hash.lock().unwrap().insert(hash_api_key(&plaintext), tenant_id);
        self.summaries.lock().unwrap().push(summary.clone());
        Ok((summary, plaintext))
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, ApiKeyStoreError> {
        Ok(self
            .summaries
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn revoke(&self, tenant_id: Uuid, id: Uuid) -> Result<(), ApiKeyStoreError> {
        let mut summaries = self.summaries.lock().unwrap();
        if let Some(existing) =
            summaries.iter_mut().find(|s| s.id == id && s.tenant_id == tenant_id)
        {
            existing.revoked_at = Some(Utc::now());
        }
        Ok(())
    }
}

pub struct FailingApiKeyStore;

#[async_trait]
impl ApiKeyStore for FailingApiKeyStore {
    async fn tenant_for_key(&self, _api_key: &str) -> Result<Option<Uuid>, ApiKeyStoreError> {
        Err(ApiKeyStoreError::Backend("simulated failure".to_string()))
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _label: &str,
    ) -> Result<(ApiKeySummary, String), ApiKeyStoreError> {
        Err(ApiKeyStoreError::Backend("simulated failure".to_string()))
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, ApiKeyStoreError> {
        Err(ApiKeyStoreError::Backend("simulated failure".to_string()))
    }

    async fn revoke(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), ApiKeyStoreError> {
        Err(ApiKeyStoreError::Backend("simulated failure".to_string()))
    }
}

#[test]
fn hash_api_key_is_deterministic_and_not_the_plaintext() {
    let hash1 = hash_api_key("secret-key");
    let hash2 = hash_api_key("secret-key");
    assert_eq!(hash1, hash2);
    assert_ne!(hash1, "secret-key");
}

#[test]
fn hash_api_key_differs_for_different_keys() {
    assert_ne!(hash_api_key("key-a"), hash_api_key("key-b"));
}

#[tokio::test]
async fn in_memory_store_resolves_known_key_to_its_tenant() {
    let tenant_id = Uuid::new_v4();
    let store = InMemoryApiKeyStore::with_key("valid-key", tenant_id);

    let resolved = store.tenant_for_key("valid-key").await.unwrap();
    assert_eq!(resolved, Some(tenant_id));
}

#[tokio::test]
async fn in_memory_store_returns_none_for_unknown_key() {
    let store = InMemoryApiKeyStore::with_key("valid-key", Uuid::new_v4());
    let resolved = store.tenant_for_key("wrong-key").await.unwrap();
    assert_eq!(resolved, None);
}

#[tokio::test]
async fn create_returns_a_plaintext_key_that_resolves_to_the_tenant() {
    let store = InMemoryApiKeyStore::default();
    let tenant_id = Uuid::new_v4();

    let (summary, plaintext) = store.create(tenant_id, "ci-agent").await.unwrap();

    assert_eq!(summary.tenant_id, tenant_id);
    assert_eq!(summary.label, "ci-agent");
    assert!(summary.revoked_at.is_none());
    assert_eq!(store.tenant_for_key(&plaintext).await.unwrap(), Some(tenant_id));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let store = InMemoryApiKeyStore::default();
    let tenant_id = Uuid::new_v4();
    store.create(tenant_id, "mine").await.unwrap();
    store.create(Uuid::new_v4(), "not-mine").await.unwrap();

    let found = store.list(tenant_id).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].label, "mine");
}

#[tokio::test]
async fn revoke_marks_the_key_revoked() {
    let store = InMemoryApiKeyStore::default();
    let tenant_id = Uuid::new_v4();
    let (summary, _plaintext) = store.create(tenant_id, "to-revoke").await.unwrap();

    store.revoke(tenant_id, summary.id).await.unwrap();

    let found = store.list(tenant_id).await.unwrap();
    assert!(found[0].revoked_at.is_some());
}
