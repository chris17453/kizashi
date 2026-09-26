use super::*;
use base64::Engine;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTenantRepository {
    pub tenants: Mutex<HashMap<String, Uuid>>,
    pub mfa_required: Mutex<HashMap<Uuid, bool>>,
    pub oidc_provider: Mutex<HashMap<Uuid, String>>,
    pub oidc_configs: Mutex<HashMap<(Uuid, String), OidcProviderConfig>>,
}

impl InMemoryTenantRepository {
    pub fn with_tenant(name: impl Into<String>, id: Uuid) -> Self {
        let mut tenants = HashMap::new();
        tenants.insert(name.into(), id);
        Self {
            tenants: Mutex::new(tenants),
            mfa_required: Mutex::new(HashMap::new()),
            oidc_provider: Mutex::new(HashMap::new()),
            oidc_configs: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl TenantRepository for InMemoryTenantRepository {
    async fn id_for_name(&self, name: &str) -> Result<Option<Uuid>, TenantRepositoryError> {
        Ok(self.tenants.lock().unwrap().get(name).copied())
    }

    async fn mfa_required(&self, tenant_id: Uuid) -> Result<bool, TenantRepositoryError> {
        Ok(self.mfa_required.lock().unwrap().get(&tenant_id).copied().unwrap_or(false))
    }

    async fn set_mfa_required(
        &self,
        tenant_id: Uuid,
        required: bool,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        self.mfa_required.lock().unwrap().insert(tenant_id, required);
        Ok(())
    }

    async fn oidc_provider(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<String>, TenantRepositoryError> {
        Ok(self.oidc_provider.lock().unwrap().get(&tenant_id).cloned())
    }

    async fn set_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: Option<&str>,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        let mut providers = self.oidc_provider.lock().unwrap();
        match provider {
            Some(value) => {
                providers.insert(tenant_id, value.to_string());
            }
            None => {
                providers.remove(&tenant_id);
            }
        }
        Ok(())
    }

    async fn list_tenant_oidc_providers(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantOidcProviderSummary>, TenantRepositoryError> {
        Ok(self
            .oidc_configs
            .lock()
            .unwrap()
            .iter()
            .filter_map(|((id, provider), config)| {
                (*id == tenant_id).then(|| TenantOidcProviderSummary {
                    provider: provider.clone(),
                    client_id: config.client_id.clone(),
                    auth_url: config.auth_url.clone(),
                    token_url: config.token_url.clone(),
                    userinfo_url: config.userinfo_url.clone(),
                    redirect_url: config.redirect_url.clone(),
                    has_client_secret: true,
                })
            })
            .collect())
    }

    async fn tenant_oidc_config(
        &self,
        tenant_id: Uuid,
        provider: &str,
    ) -> Result<Option<OidcProviderConfig>, TenantRepositoryError> {
        Ok(self.oidc_configs.lock().unwrap().get(&(tenant_id, provider.to_string())).cloned())
    }

    async fn upsert_tenant_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: &str,
        config: &OidcProviderConfig,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        self.oidc_configs.lock().unwrap().insert((tenant_id, provider.to_string()), config.clone());
        Ok(())
    }

    async fn delete_tenant_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: &str,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        self.oidc_configs.lock().unwrap().remove(&(tenant_id, provider.to_string()));
        Ok(())
    }
}

pub struct FailingTenantRepository;

#[async_trait]
impl TenantRepository for FailingTenantRepository {
    async fn id_for_name(&self, _name: &str) -> Result<Option<Uuid>, TenantRepositoryError> {
        Err(TenantRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn mfa_required(&self, _tenant_id: Uuid) -> Result<bool, TenantRepositoryError> {
        Err(TenantRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn set_mfa_required(
        &self,
        _tenant_id: Uuid,
        _required: bool,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        Err(TenantRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn oidc_provider(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<String>, TenantRepositoryError> {
        Err(TenantRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn set_oidc_provider(
        &self,
        _tenant_id: Uuid,
        _provider: Option<&str>,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        Err(TenantRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn finds_a_tenant_id_by_name() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTenantRepository::with_tenant("acme", tenant_id);

    let found = repo.id_for_name("acme").await.unwrap();
    assert_eq!(found, Some(tenant_id));
}

#[tokio::test]
async fn returns_none_for_an_unknown_tenant_name() {
    let repo = InMemoryTenantRepository::with_tenant("acme", Uuid::new_v4());

    let found = repo.id_for_name("nonexistent").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn stores_and_clears_a_tenant_oidc_provider_pin() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryTenantRepository::with_tenant("acme", tenant_id);

    assert_eq!(repo.oidc_provider(tenant_id).await.unwrap(), None);
    repo.set_oidc_provider(tenant_id, Some("generic"), "admin").await.unwrap();
    assert_eq!(repo.oidc_provider(tenant_id).await.unwrap(), Some("generic".to_string()));
    repo.set_oidc_provider(tenant_id, None, "admin").await.unwrap();
    assert_eq!(repo.oidc_provider(tenant_id).await.unwrap(), None);
}

#[test]
fn tenant_oidc_cipher_round_trips_and_changes_nonce() {
    let encoded = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
    let cipher = OidcCredentialCipher::from_base64(&encoded).unwrap();
    let (first, first_nonce) = cipher.encrypt("super-secret").unwrap();
    let (second, second_nonce) = cipher.encrypt("super-secret").unwrap();
    assert_ne!(first_nonce, second_nonce);
    assert_ne!(first, second);
    assert_eq!(cipher.decrypt(&first, &first_nonce).unwrap(), "super-secret");
    assert!(cipher.decrypt(&first, &second_nonce).is_err());
}

#[test]
fn tenant_oidc_cipher_rejects_wrong_key_length() {
    let encoded = base64::engine::general_purpose::STANDARD.encode([3_u8; 31]);
    assert!(OidcCredentialCipher::from_base64(&encoded).is_err());
}
