#[path = "invoker_test.rs"]
#[cfg(test)]
pub(crate) mod invoker_test;

use async_trait::async_trait;
use common::Sensor;
use reqwest::StatusCode;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InvokeError {
    #[error("failed to invoke connector: {0}")]
    Failed(String),
}

/// Runs one poll cycle for a due Sensor (ADR-0020). Deliberately process-per-poll, not an
/// in-process library call into the connector crate: keeps one connector's crash/hang from
/// affecting the scheduler or any other tenant's Sensor, matching ADR-0013's isolation stance.
#[async_trait]
pub trait Invoker: Send + Sync {
    /// `last_checkpoint` is this Sensor's most recent `Connector::checkpoint` value (ADR-0034),
    /// `None` until its first checkpoint-reporting poll succeeds. Returns the *new* checkpoint
    /// this invocation reported, if any, so the caller can persist it for the next poll —
    /// `Ok(None)` means this poll didn't report one (an empty result, or the connector doesn't
    /// support checkpointing), not that the previous checkpoint should be forgotten.
    async fn invoke(
        &self,
        sensor: &Sensor,
        last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError>;
}

/// Runs each due Sensor's connector via `docker run --rm <image>` against the local Docker
/// socket — the docker-compose deployment path (ADR-0020 Phase 1).
pub struct DockerInvoker {
    image_prefix: String,
    network: String,
    ingestion_gateway_url: String,
    ingestion_gateway_api_key: String,
}

/// Kubernetes-native equivalent of `DockerInvoker`: creates one short-lived batch/v1 Job for
/// each due sensor and waits for the Job's pod to finish. The connector's checkpoint marker is
/// read from pod logs, preserving the scheduler's existing incremental-cursor contract without
/// requiring a shared connector process or a Docker socket.
pub struct KubernetesJobInvoker {
    client: reqwest::Client,
    api_url: String,
    namespace: String,
    token: String,
    image_prefix: String,
    ingestion_gateway_url: String,
    ingestion_gateway_api_key: String,
    timeout: Duration,
}

impl KubernetesJobInvoker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: reqwest::Client,
        api_url: String,
        namespace: String,
        token: String,
        image_prefix: String,
        ingestion_gateway_url: String,
        ingestion_gateway_api_key: String,
        timeout: Duration,
    ) -> Self {
        Self {
            client,
            api_url: api_url.trim_end_matches('/').to_string(),
            namespace,
            token,
            image_prefix,
            ingestion_gateway_url,
            ingestion_gateway_api_key,
            timeout,
        }
    }

    pub fn from_env(
        image_prefix: String,
        ingestion_gateway_url: String,
        ingestion_gateway_api_key: String,
    ) -> Result<Self, InvokeError> {
        let token = std::env::var("KUBERNETES_SERVICE_ACCOUNT_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token").ok()
            })
            .map(|value| value.trim().to_string())
            .ok_or_else(|| {
                InvokeError::Failed(
                    "Kubernetes service-account token is not configured".to_string(),
                )
            })?;
        let api_url = std::env::var("KUBERNETES_API_URL")
            .unwrap_or_else(|_| "https://kubernetes.default.svc".to_string());
        let namespace = std::env::var("KUBERNETES_NAMESPACE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
            })
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "default".to_string());
        let mut builder = reqwest::Client::builder();
        let cert_path = std::env::var("KUBERNETES_CA_CERT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some("/var/run/secrets/kubernetes.io/serviceaccount/ca.crt".to_string()));
        if let Some(cert_path) = cert_path {
            if Path::new(&cert_path).exists() {
                let bytes = std::fs::read(Path::new(&cert_path))
                    .map_err(|e| InvokeError::Failed(format!("read Kubernetes CA cert: {e}")))?;
                let cert = reqwest::Certificate::from_pem(&bytes)
                    .map_err(|e| InvokeError::Failed(format!("parse Kubernetes CA cert: {e}")))?;
                builder = builder.add_root_certificate(cert);
            }
        }
        let client = builder
            .build()
            .map_err(|e| InvokeError::Failed(format!("build Kubernetes client: {e}")))?;
        let timeout = std::env::var("KUBERNETES_JOB_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(300));
        Ok(Self::new(
            client,
            api_url,
            namespace,
            token,
            image_prefix,
            ingestion_gateway_url,
            ingestion_gateway_api_key,
            timeout,
        ))
    }

    fn jobs_url(&self) -> String {
        format!("{}/apis/batch/v1/namespaces/{}/jobs", self.api_url, self.namespace)
    }

    fn image_name(&self, connector_type: &str) -> String {
        format!("{}-{connector_type}-connector", self.image_prefix)
    }

    pub(crate) fn job_manifest(
        &self,
        sensor: &Sensor,
        job_name: &str,
        ingestion_gateway_url: &str,
        ingestion_gateway_api_key: &str,
        last_checkpoint: Option<&str>,
    ) -> serde_json::Value {
        let mut env = vec![
            json!({"name":"TENANT_ID", "value": sensor.tenant_id.to_string()}),
            json!({"name":"CONNECTOR_ID", "value": sensor.name}),
            json!({"name":"INGESTION_GATEWAY_URL", "value": ingestion_gateway_url}),
            json!({"name":"INGESTION_GATEWAY_API_KEY", "value": ingestion_gateway_api_key}),
        ];
        if let Some(fields) = sensor.config.as_object() {
            for (key, value) in fields {
                let value = value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
                env.push(json!({"name": key, "value": value}));
            }
        }
        if sensor.connector_type == "imap" {
            if let Some(checkpoint) = last_checkpoint {
                env.push(json!({"name":"IMAP_SINCE_UID", "value": checkpoint}));
            }
        }
        json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {"name": job_name, "labels": {"app.kubernetes.io/name": "kizashi-connector", "kizashi.sensor_id": sensor.id.to_string()}},
            "spec": {
                "ttlSecondsAfterFinished": 300,
                "backoffLimit": 0,
                "template": {
                    "metadata": {"labels": {"job-name": job_name, "kizashi.sensor_id": sensor.id.to_string()}},
                    "spec": {"restartPolicy": "Never", "containers": [{"name": "connector", "image": self.image_name(&sensor.connector_type), "env": env}]}
                }
            }
        })
    }

    async fn request(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, InvokeError> {
        builder
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| InvokeError::Failed(format!("Kubernetes API request failed: {e}")))
    }

    async fn wait_for_job(&self, job_name: &str) -> Result<Option<String>, InvokeError> {
        let deadline = tokio::time::Instant::now() + self.timeout;
        let job_url = format!("{}/{}", self.jobs_url(), job_name);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(InvokeError::Failed(format!("Kubernetes Job {job_name} timed out")));
            }
            let response = self.request(self.client.get(&job_url)).await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Err(InvokeError::Failed(format!("Kubernetes Job {job_name} disappeared")));
            }
            if !response.status().is_success() {
                return Err(InvokeError::Failed(format!(
                    "Kubernetes Job status returned {}",
                    response.status()
                )));
            }
            let body: serde_json::Value = response
                .json()
                .await
                .map_err(|e| InvokeError::Failed(format!("decode Kubernetes Job status: {e}")))?;
            let status = body.get("status").cloned().unwrap_or_default();
            if status.get("succeeded").and_then(|v| v.as_u64()).unwrap_or(0) > 0 {
                let selector = format!("job-name={job_name}");
                let pods_url = format!(
                    "{}/api/v1/namespaces/{}/pods?labelSelector={}",
                    self.api_url, self.namespace, selector
                );
                let pods_response = self.request(self.client.get(pods_url)).await?;
                let pods: serde_json::Value = pods_response
                    .json()
                    .await
                    .map_err(|e| InvokeError::Failed(format!("decode Kubernetes pod list: {e}")))?;
                let pod_name = pods
                    .get("items")
                    .and_then(|items| items.as_array())
                    .and_then(|items| items.first())
                    .and_then(|pod| pod.get("metadata"))
                    .and_then(|meta| meta.get("name"))
                    .and_then(|v| v.as_str());
                let Some(pod_name) = pod_name else { return Ok(None) };
                let logs_url = format!(
                    "{}/api/v1/namespaces/{}/pods/{}/log",
                    self.api_url, self.namespace, pod_name
                );
                let logs =
                    self.request(self.client.get(logs_url)).await?.text().await.map_err(|e| {
                        InvokeError::Failed(format!("read connector pod logs: {e}"))
                    })?;
                return Ok(extract_checkpoint(logs.as_bytes()));
            }
            if status.get("failed").and_then(|v| v.as_u64()).unwrap_or(0) > 0 {
                return Err(InvokeError::Failed(format!("Kubernetes Job {job_name} failed")));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[async_trait]
impl Invoker for KubernetesJobInvoker {
    async fn invoke(
        &self,
        sensor: &Sensor,
        last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError> {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let safe_name = sensor
            .name
            .to_ascii_lowercase()
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>();
        let safe_name = safe_name.trim_matches('-');
        let job_name =
            format!("kizashi-{}-{}", &safe_name[..safe_name.len().min(40)], &suffix[..12]);
        let manifest = self.job_manifest(
            sensor,
            &job_name,
            &self.ingestion_gateway_url,
            &self.ingestion_gateway_api_key,
            last_checkpoint.as_deref(),
        );
        let response = self.request(self.client.post(self.jobs_url()).json(&manifest)).await?;
        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            return Err(InvokeError::Failed(format!(
                "create Kubernetes Job returned {status}: {detail}"
            )));
        }
        self.wait_for_job(&job_name).await
    }
}

impl DockerInvoker {
    /// `ingestion_gateway_api_key` is a single platform-wide key for v1 — every scheduled
    /// connector authenticates with it, rather than each Sensor carrying its own key reference
    /// as ADR-0020 originally described. A real per-Sensor key lookup is a follow-up once this
    /// service needs to actually mint/read keys via `ingestion-gateway`'s API key store,
    /// tracked as a known v1 simplification, not silently assumed solved.
    pub fn new(
        image_prefix: String,
        network: String,
        ingestion_gateway_url: String,
        ingestion_gateway_api_key: String,
    ) -> Self {
        Self { image_prefix, network, ingestion_gateway_url, ingestion_gateway_api_key }
    }

    pub(crate) fn image_name(&self, connector_type: &str) -> String {
        format!("{}-{connector_type}-connector", self.image_prefix)
    }

    /// Builds the full `docker run` argument list for one poll cycle — every connector-
    /// specific field in `sensor.config` becomes an `-e KEY=value`, alongside the identity/
    /// gateway env vars the deploy-script wizard already computes by hand
    /// (`ui/src/sensor_script_handler.rs::build_scripts`). Exposed at `pub(crate)` visibility
    /// purely so this method is independently unit-testable without shelling out.
    ///
    /// `last_checkpoint`, when present, is injected as `IMAP_SINCE_UID` for `connector_type ==
    /// "imap"` — a real incremental cursor (ADR-0034), not a generic mechanism: it's the one
    /// connector this scheduler currently knows how to checkpoint. On a sensor's first-ever
    /// poll (`last_checkpoint: None`), the operator's configured `IMAP_SINCE_DATE` is used as
    /// the connector's own `search_query()` fallback, unmodified.
    pub(crate) fn build_run_args(
        &self,
        sensor: &Sensor,
        ingestion_gateway_url: &str,
        ingestion_gateway_api_key: &str,
        last_checkpoint: Option<&str>,
    ) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--network".to_string(),
            self.network.clone(),
            "-e".to_string(),
            format!("TENANT_ID={}", sensor.tenant_id),
            "-e".to_string(),
            format!("CONNECTOR_ID={}", sensor.name),
            "-e".to_string(),
            format!("INGESTION_GATEWAY_URL={ingestion_gateway_url}"),
            "-e".to_string(),
            format!("INGESTION_GATEWAY_API_KEY={ingestion_gateway_api_key}"),
        ];

        if let Some(fields) = sensor.config.as_object() {
            for (key, value) in fields {
                let value_str =
                    value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
                args.push("-e".to_string());
                args.push(format!("{key}={value_str}"));
            }
        }

        if sensor.connector_type == "imap" {
            if let Some(checkpoint) = last_checkpoint {
                args.push("-e".to_string());
                args.push(format!("IMAP_SINCE_UID={checkpoint}"));
            }
        }

        args.push(self.image_name(&sensor.connector_type));
        args
    }
}

/// The `imap` connector prints this on its own stdout line when a poll produces a checkpoint
/// (see `crates/connectors/imap/src/main.rs`) — a plain marker rather than structured logging,
/// so it survives regardless of what the connector's tracing subscriber does.
const CHECKPOINT_MARKER: &str = "KIZASHI_CHECKPOINT=";

fn extract_checkpoint(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .find_map(|line| line.strip_prefix(CHECKPOINT_MARKER).map(str::to_string))
}

#[async_trait]
impl Invoker for DockerInvoker {
    async fn invoke(
        &self,
        sensor: &Sensor,
        last_checkpoint: Option<String>,
    ) -> Result<Option<String>, InvokeError> {
        let args = self.build_run_args(
            sensor,
            &self.ingestion_gateway_url,
            &self.ingestion_gateway_api_key,
            last_checkpoint.as_deref(),
        );

        let output = tokio::process::Command::new("docker")
            .args(&args)
            .output()
            .await
            .map_err(|e| InvokeError::Failed(format!("failed to spawn docker: {e}")))?;

        if !output.status.success() {
            return Err(InvokeError::Failed(format!(
                "docker run exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(extract_checkpoint(&output.stdout))
    }
}
