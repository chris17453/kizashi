use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAgentRepository {
    pub agents: Mutex<Vec<StoredAgent>>,
}

#[async_trait]
impl AgentRepository for InMemoryAgentRepository {
    async fn upsert(&self, agent: Agent) -> Result<(), AgentRepositoryError> {
        let mut agents = self.agents.lock().unwrap();
        match agents.iter_mut().find(|a| a.agent.id == agent.id) {
            Some(existing) => existing.agent = agent,
            None => agents.push(StoredAgent { agent, last_polled_at: None }),
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), AgentRepositoryError> {
        self.agents.lock().unwrap().retain(|a| a.agent.id != id);
        Ok(())
    }

    async fn list_enabled(&self) -> Result<Vec<StoredAgent>, AgentRepositoryError> {
        Ok(self.agents.lock().unwrap().iter().filter(|a| a.agent.enabled).cloned().collect())
    }

    async fn mark_polled(
        &self,
        id: Uuid,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AgentRepositoryError> {
        let mut agents = self.agents.lock().unwrap();
        if let Some(found) = agents.iter_mut().find(|a| a.agent.id == id) {
            found.last_polled_at = Some(at);
        }
        Ok(())
    }
}

fn sample_agent(enabled: bool) -> Agent {
    Agent { enabled, ..Agent::new(Uuid::new_v4(), "zendesk", "poller", serde_json::json!({})) }
}

#[tokio::test]
async fn upsert_inserts_a_new_agent() {
    let repo = InMemoryAgentRepository::default();
    let agent = sample_agent(true);

    repo.upsert(agent.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].agent, agent);
    assert!(enabled[0].last_polled_at.is_none());
}

#[tokio::test]
async fn upsert_replaces_an_existing_agent_by_id() {
    let repo = InMemoryAgentRepository::default();
    let agent = sample_agent(true);
    repo.upsert(agent.clone()).await.unwrap();

    let mut renamed = agent.clone();
    renamed.name = "renamed".to_string();
    repo.upsert(renamed.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].agent.name, "renamed");
}

#[tokio::test]
async fn list_enabled_excludes_disabled_agents() {
    let repo = InMemoryAgentRepository::default();
    repo.upsert(sample_agent(true)).await.unwrap();
    repo.upsert(sample_agent(false)).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
}

#[tokio::test]
async fn delete_removes_the_agent() {
    let repo = InMemoryAgentRepository::default();
    let agent = sample_agent(true);
    repo.upsert(agent.clone()).await.unwrap();

    repo.delete(agent.id).await.unwrap();

    assert!(repo.list_enabled().await.unwrap().is_empty());
}

#[tokio::test]
async fn mark_polled_records_the_timestamp() {
    let repo = InMemoryAgentRepository::default();
    let agent = sample_agent(true);
    repo.upsert(agent.clone()).await.unwrap();

    let now = chrono::Utc::now();
    repo.mark_polled(agent.id, now).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled[0].last_polled_at, Some(now));
}
