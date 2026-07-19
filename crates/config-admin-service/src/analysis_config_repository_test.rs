use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAnalysisConfigRepository {
    pub configs: Mutex<Vec<AnalysisConfig>>,
}

#[async_trait]
impl AnalysisConfigRepository for InMemoryAnalysisConfigRepository {
    async fn upsert(
        &self,
        config: AnalysisConfig,
    ) -> Result<AnalysisConfig, AnalysisConfigRepositoryError> {
        let mut configs = self.configs.lock().unwrap();
        match configs.iter_mut().find(|c| c.tenant_id == config.tenant_id) {
            Some(existing) => *existing = config.clone(),
            None => configs.push(config.clone()),
        }
        Ok(config)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError> {
        Ok(self.configs.lock().unwrap().iter().find(|c| c.tenant_id == tenant_id).cloned())
    }
}

pub struct FailingAnalysisConfigRepository;

#[async_trait]
impl AnalysisConfigRepository for FailingAnalysisConfigRepository {
    async fn upsert(
        &self,
        _config: AnalysisConfig,
    ) -> Result<AnalysisConfig, AnalysisConfigRepositoryError> {
        Err(AnalysisConfigRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError> {
        Err(AnalysisConfigRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn upsert_then_get_round_trips() {
    let repo = InMemoryAnalysisConfigRepository::default();
    let tenant_id = Uuid::new_v4();
    let config = AnalysisConfig::new(tenant_id, "look for urgent tickets");

    repo.upsert(config.clone()).await.unwrap();

    let found = repo.get(tenant_id).await.unwrap();
    assert_eq!(found, Some(config));
}

#[tokio::test]
async fn upsert_replaces_the_existing_config_for_that_tenant() {
    let repo = InMemoryAnalysisConfigRepository::default();
    let tenant_id = Uuid::new_v4();
    repo.upsert(AnalysisConfig::new(tenant_id, "first prompt")).await.unwrap();

    let updated = AnalysisConfig::new(tenant_id, "second prompt");
    repo.upsert(updated.clone()).await.unwrap();

    let found = repo.get(tenant_id).await.unwrap();
    assert_eq!(found, Some(updated));
}

#[tokio::test]
async fn get_returns_none_for_a_tenant_with_no_config() {
    let repo = InMemoryAnalysisConfigRepository::default();
    let found = repo.get(Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
}
