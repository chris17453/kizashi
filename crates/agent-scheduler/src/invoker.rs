#[path = "invoker_test.rs"]
#[cfg(test)]
pub(crate) mod invoker_test;

use async_trait::async_trait;
use common::Sensor;
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
/// socket — the docker-compose deployment path (ADR-0020 Phase 1). A `KubernetesJobInvoker`
/// creating a one-shot `batch/v1 Job` per due poll is the documented, not-yet-built follow-up
/// for the Kubernetes deployment path.
pub struct DockerInvoker {
    image_prefix: String,
    network: String,
    ingestion_gateway_url: String,
    ingestion_gateway_api_key: String,
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
