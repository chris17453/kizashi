#[path = "action_dispatcher_test.rs"]
#[cfg(test)]
pub(crate) mod action_dispatcher_test;

use async_trait::async_trait;
use common::{ActionRef, Event};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("action config missing required `url` field")]
    MissingUrl,
    #[error("target unreachable: {0}")]
    Unreachable(String),
    #[error("target rejected the action: HTTP {0}")]
    Rejected(u16),
}

/// Executes one action for a firing event. v1 uses a single HTTP-POST dispatch model for every
/// ActionType (ADR-0007) — genuinely functional against any webhook-shaped endpoint (Teams
/// incoming webhooks, Slack, Zapier/n8n relays, most ticketing/email HTTP APIs), not a stub.
#[async_trait]
pub trait ActionDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError>;
}

/// `egress_proxy_url` is `None` by default (ADR-0021: adoption is opt-in). When set, every
/// dispatch builds a fresh client proxied through Egress Gateway, identified as
/// `(event.tenant_id, "action-executor")` — unlike a connector process (one tenant for its
/// whole lifetime), Action Executor is multi-tenant within one process, so the proxy identity
/// can't be baked into one shared client at startup the way `connector_runtime::
/// build_outbound_client` is used by connectors; it has to be resolved per event instead.
pub struct HttpActionDispatcher {
    egress_proxy_url: Option<String>,
}

impl HttpActionDispatcher {
    pub fn new(egress_proxy_url: Option<String>) -> Self {
        Self { egress_proxy_url }
    }
}

#[async_trait]
impl ActionDispatcher for HttpActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        let url =
            action.config.get("url").and_then(|v| v.as_str()).ok_or(DispatchError::MissingUrl)?;

        let body = serde_json::json!({
            "action_type": action.action_type,
            "action_config": action.config,
            "event": event,
        });

        let client = common::build_outbound_client(
            self.egress_proxy_url.as_deref(),
            event.tenant_id,
            "action-executor",
        )
        .map_err(|e| DispatchError::Unreachable(e.to_string()))?;

        let response = client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DispatchError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DispatchError::Rejected(response.status().as_u16()));
        }

        let status = response.status().as_u16();
        Ok(serde_json::json!({"http_status": status}))
    }
}
