use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAgentRepository {
    pub agents: Mutex<Vec<Agent>>,
}

impl InMemoryAgentRepository {
    pub fn with_agent(agent: Agent) -> Self {
        Self { agents: Mutex::new(vec![agent]) }
    }
}

#[async_trait]
impl AgentRepository for InMemoryAgentRepository {
    async fn create(&self, agent: Agent) -> Result<Agent, AgentRepositoryError> {
        self.agents.lock().unwrap().push(agent.clone());
        Ok(agent)
    }

    async fn update(&self, agent: Agent) -> Result<Agent, AgentRepositoryError> {
        let mut agents = self.agents.lock().unwrap();
        match agents.iter_mut().find(|a| a.id == agent.id && a.tenant_id == agent.tenant_id) {
            Some(existing) => {
                *existing = agent.clone();
                Ok(agent)
            }
            None => Err(AgentRepositoryError::NotFound(agent.id)),
        }
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Agent>, AgentRepositoryError> {
        Ok(self
            .agents
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.id == id && a.tenant_id == tenant_id)
            .cloned())
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Agent>, AgentRepositoryError> {
        Ok(self
            .agents
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AgentRepositoryError> {
        let mut agents = self.agents.lock().unwrap();
        let before_len = agents.len();
        agents.retain(|a| !(a.id == id && a.tenant_id == tenant_id));
        if agents.len() == before_len {
            return Err(AgentRepositoryError::NotFound(id));
        }
        Ok(())
    }

    async fn find_by_name(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<Agent>, AgentRepositoryError> {
        Ok(self
            .agents
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.tenant_id == tenant_id && a.name == name)
            .cloned())
    }
}

pub struct FailingAgentRepository;

#[async_trait]
impl AgentRepository for FailingAgentRepository {
    async fn create(&self, _agent: Agent) -> Result<Agent, AgentRepositoryError> {
        Err(AgentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update(&self, _agent: Agent) -> Result<Agent, AgentRepositoryError> {
        Err(AgentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Agent>, AgentRepositoryError> {
        Err(AgentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<Agent>, AgentRepositoryError> {
        Err(AgentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), AgentRepositoryError> {
        Err(AgentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn find_by_name(
        &self,
        _tenant_id: Uuid,
        _name: &str,
    ) -> Result<Option<Agent>, AgentRepositoryError> {
        Err(AgentRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_agent(tenant_id: Uuid) -> Agent {
    Agent::new(
        tenant_id,
        "zendesk",
        "support-poller",
        serde_json::json!({"url": "https://example.zendesk.com"}),
    )
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let repo = InMemoryAgentRepository::default();
    let tenant_id = Uuid::new_v4();
    let agent = sample_agent(tenant_id);

    repo.create(agent.clone()).await.unwrap();
    let found = repo.get(tenant_id, agent.id).await.unwrap();
    assert_eq!(found, Some(agent));
}

#[tokio::test]
async fn update_of_unknown_agent_returns_not_found() {
    let repo = InMemoryAgentRepository::default();
    let agent = sample_agent(Uuid::new_v4());

    let err = repo.update(agent).await.unwrap_err();
    assert!(matches!(err, AgentRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryAgentRepository::with_agent(sample_agent(tenant_id));
    repo.create(sample_agent(Uuid::new_v4())).await.unwrap();

    let found = repo.list(tenant_id).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn delete_removes_the_agent() {
    let tenant_id = Uuid::new_v4();
    let agent = sample_agent(tenant_id);
    let repo = InMemoryAgentRepository::with_agent(agent.clone());

    repo.delete(tenant_id, agent.id).await.unwrap();
    let found = repo.get(tenant_id, agent.id).await.unwrap();
    assert_eq!(found, None);
}

#[tokio::test]
async fn delete_of_unknown_agent_returns_not_found() {
    let repo = InMemoryAgentRepository::default();

    let err = repo.delete(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, AgentRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn find_by_name_returns_the_matching_agent() {
    let tenant_id = Uuid::new_v4();
    let agent = sample_agent(tenant_id);
    let repo = InMemoryAgentRepository::with_agent(agent.clone());

    let found = repo.find_by_name(tenant_id, "support-poller").await.unwrap();
    assert_eq!(found, Some(agent));
}

#[tokio::test]
async fn find_by_name_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let agent = sample_agent(tenant_id);
    let repo = InMemoryAgentRepository::with_agent(agent);

    let found = repo.find_by_name(Uuid::new_v4(), "support-poller").await.unwrap();
    assert_eq!(found, None);
}

#[tokio::test]
async fn find_by_name_returns_none_for_unknown_name() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryAgentRepository::with_agent(sample_agent(tenant_id));

    let found = repo.find_by_name(tenant_id, "nonexistent").await.unwrap();
    assert_eq!(found, None);
}
