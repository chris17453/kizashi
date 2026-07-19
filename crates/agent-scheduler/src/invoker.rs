#[path = "invoker_test.rs"]
#[cfg(test)]
pub(crate) mod invoker_test;

use async_trait::async_trait;
use common::Agent;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InvokeError {
    #[error("failed to invoke connector: {0}")]
    Failed(String),
}

/// Runs one poll cycle for a due Agent (ADR-0020). Deliberately process-per-poll, not an
/// in-process library call into the connector crate: keeps one connector's crash/hang from
/// affecting the scheduler or any other tenant's Agent, matching ADR-0013's isolation stance.
#[async_trait]
pub trait Invoker: Send + Sync {
    async fn invoke(&self, agent: &Agent) -> Result<(), InvokeError>;
}

/// Runs each due Agent's connector via `docker run --rm <image>` against the local Docker
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
    /// connector authenticates with it, rather than each Agent carrying its own key reference
    /// as ADR-0020 originally described. A real per-Agent key lookup is a follow-up once this
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
    /// specific field in `agent.config` becomes an `-e KEY=value`, alongside the identity/
    /// gateway env vars the deploy-script wizard already computes by hand
    /// (`ui/src/agent_script_handler.rs::build_scripts`). Exposed at `pub(crate)` visibility
    /// purely so this method is independently unit-testable without shelling out.
    pub(crate) fn build_run_args(
        &self,
        agent: &Agent,
        ingestion_gateway_url: &str,
        ingestion_gateway_api_key: &str,
    ) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--network".to_string(),
            self.network.clone(),
            "-e".to_string(),
            format!("TENANT_ID={}", agent.tenant_id),
            "-e".to_string(),
            format!("CONNECTOR_ID={}", agent.name),
            "-e".to_string(),
            format!("INGESTION_GATEWAY_URL={ingestion_gateway_url}"),
            "-e".to_string(),
            format!("INGESTION_GATEWAY_API_KEY={ingestion_gateway_api_key}"),
        ];

        if let Some(fields) = agent.config.as_object() {
            for (key, value) in fields {
                let value_str =
                    value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
                args.push("-e".to_string());
                args.push(format!("{key}={value_str}"));
            }
        }

        args.push(self.image_name(&agent.connector_type));
        args
    }
}

#[async_trait]
impl Invoker for DockerInvoker {
    async fn invoke(&self, agent: &Agent) -> Result<(), InvokeError> {
        let args = self.build_run_args(
            agent,
            &self.ingestion_gateway_url,
            &self.ingestion_gateway_api_key,
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
        Ok(())
    }
}
