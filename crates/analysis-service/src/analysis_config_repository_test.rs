use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAnalysisConfigRepository {
    pub configs: Mutex<Vec<AnalysisConfig>>,
}

#[async_trait]
impl AnalysisConfigRepository for InMemoryAnalysisConfigRepository {
    async fn get(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError> {
        Ok(self.configs.lock().unwrap().iter().find(|c| c.tenant_id == tenant_id).cloned())
    }

    async fn upsert(&self, config: AnalysisConfig) -> Result<(), AnalysisConfigRepositoryError> {
        let mut configs = self.configs.lock().unwrap();
        match configs.iter_mut().find(|c| c.tenant_id == config.tenant_id) {
            Some(existing) => *existing = config,
            None => configs.push(config),
        }
        Ok(())
    }
}

#[tokio::test]
async fn get_returns_none_for_a_tenant_with_no_config() {
    let repo = InMemoryAnalysisConfigRepository::default();
    let found = repo.get(Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn upsert_inserts_a_new_config() {
    let repo = InMemoryAnalysisConfigRepository::default();
    let config = AnalysisConfig::new(Uuid::new_v4(), "look for urgent tickets");

    repo.upsert(config.clone()).await.unwrap();

    let found = repo.get(config.tenant_id).await.unwrap();
    assert_eq!(found, Some(config));
}

#[tokio::test]
async fn upsert_replaces_an_existing_config_for_the_same_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryAnalysisConfigRepository::default();
    repo.upsert(AnalysisConfig::new(tenant_id, "first")).await.unwrap();

    let updated = AnalysisConfig::new(tenant_id, "second");
    repo.upsert(updated.clone()).await.unwrap();

    let found = repo.get(tenant_id).await.unwrap();
    assert_eq!(found, Some(updated));
}

#[tokio::test]
async fn upsert_then_get_round_trips_provider_and_model_fields() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryAnalysisConfigRepository::default();
    let mut config = AnalysisConfig::new(tenant_id, "flag policy violations");
    config.provider = common::AnalysisProvider::OpenAiCompatible;
    config.model = Some("qwen3:8b".to_string());
    config.endpoint = Some("http://localhost:11434/v1".to_string());
    config.api_key = Some("unused-for-ollama".to_string());

    repo.upsert(config.clone()).await.unwrap();

    let found = repo.get(tenant_id).await.unwrap();
    assert_eq!(found, Some(config));
}
