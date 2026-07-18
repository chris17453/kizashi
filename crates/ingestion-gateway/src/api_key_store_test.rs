use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory test double keyed by hashed API key, mirroring how the real store never sees
/// plaintext keys at rest.
#[derive(Default)]
pub struct InMemoryApiKeyStore {
    pub keys_by_hash: Mutex<HashMap<String, Uuid>>,
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
