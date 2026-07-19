use super::*;
use common::Agent;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryAgentPublisher {
    pub published: Mutex<Vec<AgentChangeEvent>>,
}

#[async_trait]
impl AgentPublisher for InMemoryAgentPublisher {
    async fn publish_agent_changed(
        &self,
        event: &AgentChangeEvent,
    ) -> Result<(), AgentPublishError> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub struct FailingAgentPublisher;

#[async_trait]
impl AgentPublisher for FailingAgentPublisher {
    async fn publish_agent_changed(
        &self,
        _event: &AgentChangeEvent,
    ) -> Result<(), AgentPublishError> {
        Err(AgentPublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_event() -> AgentChangeEvent {
    AgentChangeEvent::Upserted(Agent::new(
        Uuid::new_v4(),
        "zendesk",
        "support-poller",
        serde_json::json!({}),
    ))
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemoryAgentPublisher::default();
    let event = sample_event();

    publisher.publish_agent_changed(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingAgentPublisher;
    let err = publisher.publish_agent_changed(&sample_event()).await.unwrap_err();
    assert!(matches!(err, AgentPublishError::Bus(_)));
}
