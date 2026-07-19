use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryInvoker {
    pub invocations: Mutex<Vec<Agent>>,
}

#[async_trait]
impl Invoker for InMemoryInvoker {
    async fn invoke(&self, agent: &Agent) -> Result<(), InvokeError> {
        self.invocations.lock().unwrap().push(agent.clone());
        Ok(())
    }
}

pub struct FailingInvoker;

#[async_trait]
impl Invoker for FailingInvoker {
    async fn invoke(&self, _agent: &Agent) -> Result<(), InvokeError> {
        Err(InvokeError::Failed("simulated failure".to_string()))
    }
}

fn sample_agent() -> Agent {
    Agent::new(
        uuid::Uuid::new_v4(),
        "zendesk",
        "support-poller",
        serde_json::json!({"ZENDESK_SUBDOMAIN": "acme", "ZENDESK_API_TOKEN": "tok"}),
    )
}

#[test]
fn docker_invoker_builds_the_expected_image_name() {
    let invoker = DockerInvoker::new(
        "kizashi".to_string(),
        "kizashi-net".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "test-key".to_string(),
    );
    assert_eq!(invoker.image_name("zendesk"), "kizashi-zendesk-connector");
    assert_eq!(invoker.image_name("graph-mail"), "kizashi-graph-mail-connector");
}

#[test]
fn docker_invoker_builds_env_args_from_agent_config_and_identity() {
    let invoker = DockerInvoker::new(
        "kizashi".to_string(),
        "kizashi-net".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "test-key".to_string(),
    );
    let agent = sample_agent();

    let args = invoker.build_run_args(&agent, "http://ingestion-gateway:8080", "test-key");

    let joined = args.join(" ");
    assert!(joined.contains(&format!("TENANT_ID={}", agent.tenant_id)));
    assert!(joined.contains("CONNECTOR_ID=support-poller"));
    assert!(joined.contains("INGESTION_GATEWAY_URL=http://ingestion-gateway:8080"));
    assert!(joined.contains("INGESTION_GATEWAY_API_KEY=test-key"));
    assert!(joined.contains("ZENDESK_SUBDOMAIN=acme"));
    assert!(joined.contains("ZENDESK_API_TOKEN=tok"));
    assert!(joined.contains("kizashi-zendesk-connector"));
}

#[tokio::test]
async fn in_memory_invoker_records_invocations() {
    let invoker = InMemoryInvoker::default();
    let agent = sample_agent();

    invoker.invoke(&agent).await.unwrap();

    assert_eq!(invoker.invocations.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failing_invoker_returns_an_error() {
    let invoker = FailingInvoker;
    let err = invoker.invoke(&sample_agent()).await.unwrap_err();
    assert!(matches!(err, InvokeError::Failed(_)));
}
