use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTokenStore {
    pub tokens_by_hash: Mutex<HashMap<String, Uuid>>,
}

impl InMemoryTokenStore {
    pub fn with_token(token: &str, tenant_id: Uuid) -> Self {
        let store = Self::default();
        store.tokens_by_hash.lock().unwrap().insert(hash_token(token), tenant_id);
        store
    }
}

#[async_trait]
impl TokenStore for InMemoryTokenStore {
    async fn tenant_for_token(&self, token: &str) -> Result<Option<Uuid>, TokenStoreError> {
        Ok(self.tokens_by_hash.lock().unwrap().get(&hash_token(token)).copied())
    }

    async fn mint_token(&self, tenant_id: Uuid, _label: &str) -> Result<String, TokenStoreError> {
        let token = generate_token();
        self.tokens_by_hash.lock().unwrap().insert(hash_token(&token), tenant_id);
        Ok(token)
    }
}

pub struct FailingTokenStore;

#[async_trait]
impl TokenStore for FailingTokenStore {
    async fn tenant_for_token(&self, _token: &str) -> Result<Option<Uuid>, TokenStoreError> {
        Err(TokenStoreError::Backend("simulated failure".to_string()))
    }

    async fn mint_token(&self, _tenant_id: Uuid, _label: &str) -> Result<String, TokenStoreError> {
        Err(TokenStoreError::Backend("simulated failure".to_string()))
    }
}

#[test]
fn hash_token_is_deterministic_and_not_the_plaintext() {
    let hash1 = hash_token("secret-token");
    let hash2 = hash_token("secret-token");
    assert_eq!(hash1, hash2);
    assert_ne!(hash1, "secret-token");
}

#[tokio::test]
async fn in_memory_store_resolves_known_token_to_its_tenant() {
    let tenant_id = Uuid::new_v4();
    let store = InMemoryTokenStore::with_token("valid-token", tenant_id);

    let resolved = store.tenant_for_token("valid-token").await.unwrap();
    assert_eq!(resolved, Some(tenant_id));
}

#[tokio::test]
async fn in_memory_store_returns_none_for_unknown_token() {
    let store = InMemoryTokenStore::with_token("valid-token", Uuid::new_v4());
    let resolved = store.tenant_for_token("wrong-token").await.unwrap();
    assert_eq!(resolved, None);
}

#[tokio::test]
async fn minted_tokens_immediately_resolve_to_the_tenant_they_were_minted_for() {
    let store = InMemoryTokenStore::default();
    let tenant_id = Uuid::new_v4();

    let token = store.mint_token(tenant_id, "test-session").await.unwrap();
    let resolved = store.tenant_for_token(&token).await.unwrap();

    assert_eq!(resolved, Some(tenant_id));
}

#[test]
fn generate_token_produces_distinct_high_entropy_values() {
    let a = generate_token();
    let b = generate_token();
    assert_ne!(a, b);
    assert_eq!(a.len(), 64);
}
