use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryRetentionPolicyRepository {
    pub policies: Mutex<Vec<RetentionPolicy>>,
}

impl InMemoryRetentionPolicyRepository {
    pub fn with_policy(policy: RetentionPolicy) -> Self {
        Self { policies: Mutex::new(vec![policy]) }
    }
}

#[async_trait]
impl RetentionPolicyRepository for InMemoryRetentionPolicyRepository {
    async fn create(
        &self,
        policy: RetentionPolicy,
        _actor: &str,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError> {
        self.policies.lock().unwrap().push(policy.clone());
        Ok(policy)
    }

    async fn update(
        &self,
        policy: RetentionPolicy,
        _actor: &str,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError> {
        let mut policies = self.policies.lock().unwrap();
        match policies.iter_mut().find(|p| p.id == policy.id && p.tenant_id == policy.tenant_id) {
            Some(existing) => {
                *existing = policy.clone();
                Ok(policy)
            }
            None => Err(RetentionPolicyRepositoryError::NotFound(policy.id)),
        }
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        _actor: &str,
    ) -> Result<(), RetentionPolicyRepositoryError> {
        let mut policies = self.policies.lock().unwrap();
        let before_len = policies.len();
        policies.retain(|p| !(p.id == id && p.tenant_id == tenant_id));
        if policies.len() == before_len {
            return Err(RetentionPolicyRepositoryError::NotFound(id));
        }
        Ok(())
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionPolicyRepositoryError> {
        Ok(self
            .policies
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == id && p.tenant_id == tenant_id)
            .cloned())
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError> {
        Ok(self
            .policies
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn list_all_enabled(
        &self,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError> {
        Ok(self.policies.lock().unwrap().iter().filter(|p| p.enabled).cloned().collect())
    }
}

pub struct FailingRetentionPolicyRepository;

#[async_trait]
impl RetentionPolicyRepository for FailingRetentionPolicyRepository {
    async fn create(
        &self,
        _policy: RetentionPolicy,
        _actor: &str,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError> {
        Err(RetentionPolicyRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update(
        &self,
        _policy: RetentionPolicy,
        _actor: &str,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError> {
        Err(RetentionPolicyRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn delete(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _actor: &str,
    ) -> Result<(), RetentionPolicyRepositoryError> {
        Err(RetentionPolicyRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionPolicyRepositoryError> {
        Err(RetentionPolicyRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError> {
        Err(RetentionPolicyRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_all_enabled(
        &self,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError> {
        Err(RetentionPolicyRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_policy(tenant_id: Uuid) -> RetentionPolicy {
    RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days: 90,
        enabled: true,
    }
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let repo = InMemoryRetentionPolicyRepository::default();
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);

    repo.create(policy.clone(), "tester").await.unwrap();
    let found = repo.get(tenant_id, policy.id).await.unwrap();
    assert_eq!(found, Some(policy));
}

#[tokio::test]
async fn update_of_unknown_policy_returns_not_found() {
    let repo = InMemoryRetentionPolicyRepository::default();
    let policy = sample_policy(Uuid::new_v4());

    let err = repo.update(policy, "tester").await.unwrap_err();
    assert!(matches!(err, RetentionPolicyRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryRetentionPolicyRepository::with_policy(sample_policy(tenant_id));
    repo.create(sample_policy(Uuid::new_v4()), "tester").await.unwrap();

    let found = repo.list(tenant_id).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn list_all_enabled_excludes_disabled_policies_across_tenants() {
    let repo = InMemoryRetentionPolicyRepository::default();
    let mut disabled = sample_policy(Uuid::new_v4());
    disabled.enabled = false;
    repo.create(sample_policy(Uuid::new_v4()), "tester").await.unwrap();
    repo.create(disabled, "tester").await.unwrap();

    let found = repo.list_all_enabled().await.unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].enabled);
}

#[tokio::test]
async fn delete_removes_the_policy_then_get_returns_none() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let repo = InMemoryRetentionPolicyRepository::with_policy(policy.clone());

    repo.delete(tenant_id, policy.id, "tester").await.unwrap();

    assert_eq!(repo.get(tenant_id, policy.id).await.unwrap(), None);
}

#[tokio::test]
async fn delete_of_unknown_policy_returns_not_found() {
    let repo = InMemoryRetentionPolicyRepository::default();
    let err = repo.delete(Uuid::new_v4(), Uuid::new_v4(), "tester").await.unwrap_err();
    assert!(matches!(err, RetentionPolicyRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn delete_does_not_remove_a_policy_owned_by_a_different_tenant() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let repo = InMemoryRetentionPolicyRepository::with_policy(policy.clone());

    let err = repo.delete(Uuid::new_v4(), policy.id, "tester").await.unwrap_err();
    assert!(matches!(err, RetentionPolicyRepositoryError::NotFound(_)));
    assert_eq!(repo.get(tenant_id, policy.id).await.unwrap(), Some(policy));
}
