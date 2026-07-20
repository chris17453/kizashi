use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAllowlistRepository {
    pub domains: Mutex<std::collections::HashMap<String, Vec<String>>>,
}

#[async_trait]
impl AllowlistRepository for InMemoryAllowlistRepository {
    async fn get_domains(&self, tenant_id: &str) -> Result<Vec<String>, AllowlistError> {
        Ok(self.domains.lock().unwrap().get(tenant_id).cloned().unwrap_or_default())
    }

    async fn set_domains(
        &self,
        tenant_id: &str,
        domains: Vec<String>,
        _actor: &str,
    ) -> Result<(), AllowlistError> {
        self.domains.lock().unwrap().insert(tenant_id.to_string(), domains);
        Ok(())
    }
}

pub struct FailingAllowlistRepository;

#[async_trait]
impl AllowlistRepository for FailingAllowlistRepository {
    async fn get_domains(&self, _tenant_id: &str) -> Result<Vec<String>, AllowlistError> {
        Err(AllowlistError::Backend("simulated failure".to_string()))
    }

    async fn set_domains(
        &self,
        _tenant_id: &str,
        _domains: Vec<String>,
        _actor: &str,
    ) -> Result<(), AllowlistError> {
        Err(AllowlistError::Backend("simulated failure".to_string()))
    }
}

#[test]
fn host_is_allowed_when_the_tenant_has_no_configured_allowlist() {
    assert!(is_host_allowed(&[], "api.zendesk.com"));
}

#[test]
fn host_is_allowed_when_it_exactly_matches_an_allowlisted_domain() {
    assert!(is_host_allowed(&["api.zendesk.com".to_string()], "api.zendesk.com"));
}

#[test]
fn host_is_allowed_when_it_is_a_subdomain_of_an_allowlisted_domain() {
    assert!(is_host_allowed(&["zendesk.com".to_string()], "acme.zendesk.com"));
}

#[test]
fn host_is_denied_when_it_matches_no_allowlisted_domain() {
    assert!(!is_host_allowed(&["zendesk.com".to_string()], "evil.example.com"));
}

#[test]
fn host_matching_is_not_fooled_by_a_suffix_that_is_not_a_subdomain_boundary() {
    // "notzendesk.com" must not match an allowlist entry of "zendesk.com" just because it
    // ends with the same characters.
    assert!(!is_host_allowed(&["zendesk.com".to_string()], "notzendesk.com"));
}

#[tokio::test]
async fn in_memory_repository_round_trips_domains() {
    let repo = InMemoryAllowlistRepository::default();
    repo.set_domains("tenant-a", vec!["zendesk.com".to_string()], "test-actor").await.unwrap();

    let domains = repo.get_domains("tenant-a").await.unwrap();
    assert_eq!(domains, vec!["zendesk.com".to_string()]);
}

#[tokio::test]
async fn failing_repository_returns_backend_error() {
    let repo = FailingAllowlistRepository;
    let err = repo.get_domains("tenant-a").await.unwrap_err();
    assert!(matches!(err, AllowlistError::Backend(_)));
}
