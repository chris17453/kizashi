use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTenantRepository {
    pub tenants: Mutex<HashMap<String, Uuid>>,
}

impl InMemoryTenantRepository {
    pub fn with_tenant(name: impl Into<String>, id: Uuid) -> Self {
        let mut tenants = HashMap::new();
        tenants.insert(name.into(), id);
        Self { tenants: Mutex::new(tenants) }
    }
}

#[async_trait]
impl TenantRepository for InMemoryTenantRepository {
    async fn id_for_name(&self, name: &str) -> Result<Option<Uuid>, TenantRepositoryError> {
        Ok(self.tenants.lock().unwrap().get(name).copied())
    }
}

pub struct FailingTenantRepository;

#[async_trait]
impl TenantRepository for FailingTenantRepository {
    async fn id_for_name(&self, _name: &str) -> Result<Option<Uuid>, TenantRepositoryError> {
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
