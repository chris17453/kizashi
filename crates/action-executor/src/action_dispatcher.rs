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
    #[error("action config invalid: {0}")]
    InvalidConfig(String),
}

/// Substitutes `{{field}}` placeholders in every string leaf of `template` with the
/// corresponding value from `event` (ADR-0028) — lets an operator shape the exact JSON body a
/// real third-party webhook requires (Slack's `{"text": "..."}`, PagerDuty's Events API v2
/// envelope, a Jira/ServiceNow REST body) instead of always receiving the generic `{action_
/// type, action_config, event}` envelope, which most such targets reject outright. Recognized
/// placeholders: `event_type`, `entity_ref`, `group_key`, `tenant_id`, `occurred_at`, and
/// `payload` (the event's payload as a compact JSON string). An unrecognized placeholder is
/// left as literal text, not an error — never panics on operator-authored config.
fn render_body_template(template: &serde_json::Value, event: &Event) -> serde_json::Value {
    match template {
        serde_json::Value::String(s) => {
            let payload_str = event.payload.to_string();
            let rendered = s
                .replace("{{event_type}}", &event.event_type)
                .replace("{{entity_ref}}", &event.entity_ref)
                .replace("{{group_key}}", &event.group_key)
                .replace("{{tenant_id}}", &event.tenant_id.to_string())
                .replace("{{occurred_at}}", &event.occurred_at.to_rfc3339())
                .replace("{{payload}}", &payload_str);
            serde_json::Value::String(rendered)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|v| render_body_template(v, event)).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter().map(|(k, v)| (k.clone(), render_body_template(v, event))).collect(),
        ),
        other => other.clone(),
    }
}

/// Executes one action for a firing event. v1 uses a single HTTP-POST dispatch model for every
/// ActionType (ADR-0007) — genuinely functional against any webhook-shaped endpoint (Teams
/// incoming webhooks, Slack, Zapier/n8n relays, most ticketing/email HTTP APIs), not a stub.
/// An action's config may include a `body_template` (ADR-0028) to shape the exact JSON body a
/// specific target requires; without one, the generic envelope below is sent as before.
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

        let body = match action.config.get("body_template") {
            Some(template) => render_body_template(template, event),
            None => serde_json::json!({
                "action_type": action.action_type,
                "action_config": action.config,
                "event": event,
            }),
        };

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
