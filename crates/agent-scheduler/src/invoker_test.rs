use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryInvoker {
    pub invocations: Mutex<Vec<Agent>>,
}

#[async_trait]
impl Invoker for InMemoryInvoker {
    async fn invoke(
        &self,
        agent: &Agent,
        _last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError> {
        self.invocations.lock().unwrap().push(agent.clone());
        Ok(None)
    }
}

pub struct FailingInvoker;

#[async_trait]
impl Invoker for FailingInvoker {
    async fn invoke(
        &self,
        _agent: &Agent,
        _last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError> {
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

    let args = invoker.build_run_args(&agent, "http://ingestion-gateway:8080", "test-key", None);

    let joined = args.join(" ");
    assert!(joined.contains(&format!("TENANT_ID={}", agent.tenant_id)));
    assert!(joined.contains("CONNECTOR_ID=support-poller"));
    assert!(joined.contains("INGESTION_GATEWAY_URL=http://ingestion-gateway:8080"));
    assert!(joined.contains("INGESTION_GATEWAY_API_KEY=test-key"));
    assert!(joined.contains("ZENDESK_SUBDOMAIN=acme"));
    assert!(joined.contains("ZENDESK_API_TOKEN=tok"));
    assert!(joined.contains("kizashi-zendesk-connector"));
}

fn imap_agent(since_date: &str) -> Agent {
    Agent::new(
        uuid::Uuid::new_v4(),
        "imap",
        "mail-poller",
        serde_json::json!({"IMAP_HOST": "mail.example.com", "IMAP_SINCE_DATE": since_date}),
    )
}

#[test]
fn imap_since_date_is_unmodified_on_a_first_ever_poll() {
    let invoker = DockerInvoker::new(
        "kizashi".to_string(),
        "kizashi-net".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "test-key".to_string(),
    );
    let agent = imap_agent("2025-01-19");

    let args = invoker.build_run_args(&agent, "http://ingestion-gateway:8080", "test-key", None);

    let joined = args.join(" ");
    assert!(joined.contains("IMAP_SINCE_DATE=2025-01-19"));
    assert!(!joined.contains("IMAP_SINCE_UID="));
}

#[test]
fn imap_since_uid_is_injected_from_the_last_checkpoint_on_a_later_poll() {
    let invoker = DockerInvoker::new(
        "kizashi".to_string(),
        "kizashi-net".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "test-key".to_string(),
    );
    let agent = imap_agent("2025-01-19");

    let args =
        invoker.build_run_args(&agent, "http://ingestion-gateway:8080", "test-key", Some("42"));

    let joined = args.join(" ");
    assert!(joined.contains("IMAP_SINCE_UID=42"));
    // The originally-configured backfill date is still passed through unmodified — the
    // connector itself prefers IMAP_SINCE_UID when both are present (see ImapConnector's
    // search_query()), so there's no need to strip or override it here.
    assert!(joined.contains("IMAP_SINCE_DATE=2025-01-19"));
}

#[test]
fn non_imap_connectors_are_unaffected_by_a_checkpoint() {
    let invoker = DockerInvoker::new(
        "kizashi".to_string(),
        "kizashi-net".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "test-key".to_string(),
    );
    let agent = sample_agent();

    let args = invoker.build_run_args(
        &agent,
        "http://ingestion-gateway:8080",
        "test-key",
        Some("some-checkpoint"),
    );

    let joined = args.join(" ");
    assert!(joined.contains("ZENDESK_SUBDOMAIN=acme"));
    assert!(!joined.contains("IMAP_SINCE_UID="));
}

#[test]
fn extract_checkpoint_finds_the_marker_line_among_other_stdout_output() {
    let stdout = b"some log line\nKIZASHI_CHECKPOINT=12345\nanother line\n";
    assert_eq!(extract_checkpoint(stdout), Some("12345".to_string()));
}

#[test]
fn extract_checkpoint_returns_none_when_the_marker_is_absent() {
    let stdout = b"just some ordinary output\n";
    assert_eq!(extract_checkpoint(stdout), None);
}

#[tokio::test]
async fn in_memory_invoker_records_invocations() {
    let invoker = InMemoryInvoker::default();
    let agent = sample_agent();

    invoker.invoke(&agent, None).await.unwrap();

    assert_eq!(invoker.invocations.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failing_invoker_returns_an_error() {
    let invoker = FailingInvoker;
    let err = invoker.invoke(&sample_agent(), None).await.unwrap_err();
    assert!(matches!(err, InvokeError::Failed(_)));
}
