use super::*;
use axum::{
    extract::Path,
    routing::{get, post},
    Json, Router,
};
use std::future::IntoFuture;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryInvoker {
    pub invocations: Mutex<Vec<Sensor>>,
}

#[async_trait]
impl Invoker for InMemoryInvoker {
    async fn invoke(
        &self,
        sensor: &Sensor,
        _last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError> {
        self.invocations.lock().unwrap().push(sensor.clone());
        Ok(None)
    }
}

pub struct FailingInvoker;

#[async_trait]
impl Invoker for FailingInvoker {
    async fn invoke(
        &self,
        _sensor: &Sensor,
        _last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError> {
        Err(InvokeError::Failed("simulated failure".to_string()))
    }
}

fn sample_sensor() -> Sensor {
    Sensor::new(
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
fn docker_invoker_builds_env_args_from_sensor_config_and_identity() {
    let invoker = DockerInvoker::new(
        "kizashi".to_string(),
        "kizashi-net".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "test-key".to_string(),
    );
    let sensor = sample_sensor();

    let args = invoker.build_run_args(&sensor, "http://ingestion-gateway:8080", "test-key", None);

    let joined = args.join(" ");
    assert!(joined.contains(&format!("TENANT_ID={}", sensor.tenant_id)));
    assert!(joined.contains("CONNECTOR_ID=support-poller"));
    assert!(joined.contains("INGESTION_GATEWAY_URL=http://ingestion-gateway:8080"));
    assert!(joined.contains("INGESTION_GATEWAY_API_KEY=test-key"));
    assert!(joined.contains("ZENDESK_SUBDOMAIN=acme"));
    assert!(joined.contains("ZENDESK_API_TOKEN=tok"));
    assert!(joined.contains("kizashi-zendesk-connector"));
}

#[test]
fn kubernetes_job_manifest_preserves_tenant_env_and_checkpoint() {
    let invoker = KubernetesJobInvoker::new(
        reqwest::Client::new(),
        "https://kubernetes.test".to_string(),
        "kizashi".to_string(),
        "token".to_string(),
        "kizashi".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "gateway-key".to_string(),
        std::time::Duration::from_secs(30),
    );
    let sensor = imap_sensor("2025-01-19");
    let manifest = invoker.job_manifest(
        &sensor,
        "kizashi-mail-poller-abc123",
        "http://ingestion-gateway:8080",
        "gateway-key",
        Some("42"),
    );
    assert_eq!(manifest["kind"], "Job");
    assert_eq!(manifest["metadata"]["name"], "kizashi-mail-poller-abc123");
    assert_eq!(manifest["spec"]["backoffLimit"], 0);
    let env = manifest["spec"]["template"]["spec"]["containers"][0]["env"].as_array().unwrap();
    let env_map = env
        .iter()
        .filter_map(|item| Some((item["name"].as_str()?, item["value"].as_str()?)))
        .collect::<std::collections::HashMap<_, _>>();
    let tenant_id = sensor.tenant_id.to_string();
    assert_eq!(env_map.get("TENANT_ID").copied(), Some(tenant_id.as_str()));
    assert_eq!(env_map.get("IMAP_SINCE_UID"), Some(&"42"));
    assert_eq!(
        manifest["spec"]["template"]["spec"]["containers"][0]["image"],
        "kizashi-imap-connector"
    );
}

async fn fake_kubernetes_create_job(
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "metadata": {"name": body["metadata"]["name"].clone()}
    }))
}

async fn fake_kubernetes_job_status(Path(_job_name): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": {"succeeded": 1}}))
}

async fn fake_kubernetes_pods() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "items": [{"metadata": {"name": "connector-pod"}}]
    }))
}

async fn fake_kubernetes_pod_logs() -> &'static str {
    "connector started\nKIZASHI_CHECKPOINT=99\n"
}

#[tokio::test]
async fn kubernetes_invoker_runs_the_job_lifecycle_and_returns_checkpoint() {
    let app = Router::new()
        .route("/apis/batch/v1/namespaces/kizashi/jobs", post(fake_kubernetes_create_job))
        .route("/apis/batch/v1/namespaces/kizashi/jobs/:job_name", get(fake_kubernetes_job_status))
        .route("/api/v1/namespaces/kizashi/pods", get(fake_kubernetes_pods))
        .route("/api/v1/namespaces/kizashi/pods/:pod_name/log", get(fake_kubernetes_pod_logs));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(axum::serve(listener, app).into_future());

    let invoker = KubernetesJobInvoker::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        "kizashi".to_string(),
        "test-token".to_string(),
        "kizashi".to_string(),
        "http://ingestion-gateway:8080".to_string(),
        "gateway-key".to_string(),
        std::time::Duration::from_secs(5),
    );
    let checkpoint = invoker.invoke(&sample_sensor(), None).await.unwrap();

    server.abort();
    assert_eq!(checkpoint, Some("99".to_string()));
}

fn imap_sensor(since_date: &str) -> Sensor {
    Sensor::new(
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
    let sensor = imap_sensor("2025-01-19");

    let args = invoker.build_run_args(&sensor, "http://ingestion-gateway:8080", "test-key", None);

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
    let sensor = imap_sensor("2025-01-19");

    let args =
        invoker.build_run_args(&sensor, "http://ingestion-gateway:8080", "test-key", Some("42"));

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
    let sensor = sample_sensor();

    let args = invoker.build_run_args(
        &sensor,
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
    let sensor = sample_sensor();

    invoker.invoke(&sensor, None).await.unwrap();

    assert_eq!(invoker.invocations.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failing_invoker_returns_an_error() {
    let invoker = FailingInvoker;
    let err = invoker.invoke(&sample_sensor(), None).await.unwrap_err();
    assert!(matches!(err, InvokeError::Failed(_)));
}
