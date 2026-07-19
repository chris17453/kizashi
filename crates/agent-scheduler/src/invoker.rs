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
    /// `last_polled_at` is `None` on an agent's first-ever poll (use its configured backfill
    /// window as-is) and `Some` on every later poll — connectors that understand a `since`-
    /// style window narrow it instead of re-scanning from the original configured start every
    /// time (see `DockerInvoker::build_run_args`'s IMAP override).
    async fn invoke(
        &self,
        agent: &Agent,
        last_polled_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), InvokeError>;
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
    /// `last_polled_at` narrows the IMAP connector's `IMAP_SINCE_DATE` on every poll after the
    /// first, instead of re-scanning from the operator's originally-configured backfill start
    /// forever — see the `Invoker::invoke` doc comment. This is deliberately special-cased to
    /// `connector_type == "imap"` rather than a generic mechanism: it's the one connector this
    /// scheduler currently knows re-scans a stateless date window, and a generic per-connector
    /// cursor protocol is real follow-up work (see ADR-0033), not something to fake here.
    pub(crate) fn build_run_args(
        &self,
        agent: &Agent,
        ingestion_gateway_url: &str,
        ingestion_gateway_api_key: &str,
        last_polled_at: Option<chrono::DateTime<chrono::Utc>>,
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

        // A one-day overlap, not an exact cursor: IMAP's SEARCH SINCE command only has date
        // granularity, so this is the coarsest safe margin against missing a message that
        // landed right at a previous poll's boundary. Real dedup (ADR-0032) is what makes the
        // resulting re-scan of that overlap day safe rather than duplicating anything.
        let imap_since_override = if agent.connector_type == "imap" {
            last_polled_at.map(|t| (t - chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
        } else {
            None
        };

        if let Some(fields) = agent.config.as_object() {
            for (key, value) in fields {
                if key == "IMAP_SINCE_DATE" {
                    if let Some(overridden) = &imap_since_override {
                        args.push("-e".to_string());
                        args.push(format!("IMAP_SINCE_DATE={overridden}"));
                        continue;
                    }
                }
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
    async fn invoke(
        &self,
        agent: &Agent,
        last_polled_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), InvokeError> {
        let args = self.build_run_args(
            agent,
            &self.ingestion_gateway_url,
            &self.ingestion_gateway_api_key,
            last_polled_at,
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
