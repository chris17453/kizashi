use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

fn sample() -> TenantBranding {
    TenantBranding {
        product_name: Some("Acme Signals".to_string()),
        logo_url: Some("https://acme.example.com/logo.png".to_string()),
        accent_color: Some("#ff6600".to_string()),
    }
}

#[derive(Default)]
pub struct InMemoryTenantBrandingRepository {
    pub branding: Mutex<HashMap<String, TenantBranding>>,
    pub last_update_actor: Mutex<Option<String>>,
}

#[async_trait]
impl TenantBrandingRepository for InMemoryTenantBrandingRepository {
    async fn branding_for_name(
        &self,
        name: &str,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError> {
        Ok(self.branding.lock().unwrap().get(name).cloned())
    }

    async fn branding_for_id(
        &self,
        id: Uuid,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError> {
        Ok(self.branding.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn update_branding(
        &self,
        tenant_id: Uuid,
        branding: TenantBranding,
        actor: &str,
    ) -> Result<(), TenantBrandingRepositoryError> {
        *self.last_update_actor.lock().unwrap() = Some(actor.to_string());
        self.branding.lock().unwrap().insert(tenant_id.to_string(), branding);
        Ok(())
    }
}

pub struct FailingTenantBrandingRepository;

#[async_trait]
impl TenantBrandingRepository for FailingTenantBrandingRepository {
    async fn branding_for_name(
        &self,
        _name: &str,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError> {
        Err(TenantBrandingRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn branding_for_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError> {
        Err(TenantBrandingRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update_branding(
        &self,
        _tenant_id: Uuid,
        _branding: TenantBranding,
        _actor: &str,
    ) -> Result<(), TenantBrandingRepositoryError> {
        Err(TenantBrandingRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn returns_none_for_a_tenant_with_no_branding_configured() {
    let repo = InMemoryTenantBrandingRepository::default();
    assert_eq!(repo.branding_for_name("acme").await.unwrap(), None);
}

#[tokio::test]
async fn update_then_lookup_round_trips() {
    let repo = InMemoryTenantBrandingRepository::default();
    let tenant_id = Uuid::new_v4();
    repo.update_branding(tenant_id, sample(), "alice").await.unwrap();
    repo.branding.lock().unwrap().insert("acme".to_string(), sample());

    let found = repo.branding_for_name("acme").await.unwrap();
    assert_eq!(found, Some(sample()));
    assert_eq!(*repo.last_update_actor.lock().unwrap(), Some("alice".to_string()));
}

#[tokio::test]
async fn update_then_lookup_by_id_round_trips() {
    let repo = InMemoryTenantBrandingRepository::default();
    let tenant_id = Uuid::new_v4();
    repo.update_branding(tenant_id, sample(), "alice").await.unwrap();

    let found = repo.branding_for_id(tenant_id).await.unwrap();
    assert_eq!(found, Some(sample()));
}
