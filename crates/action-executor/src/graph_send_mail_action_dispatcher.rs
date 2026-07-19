#[path = "graph_send_mail_action_dispatcher_test.rs"]
#[cfg(test)]
mod graph_send_mail_action_dispatcher_test;

use async_trait::async_trait;
use common::{ActionRef, Event};
use connector_runtime::fetch_access_token;

use crate::action_dispatcher::{ActionDispatcher, DispatchError};

const GRAPH_SCOPE: &str = "https://graph.microsoft.com/.default";
const DEFAULT_GRAPH_BASE_URL: &str = "https://graph.microsoft.com/v1.0";

/// Sends an email as a real mailbox user via Microsoft Graph's `POST /users/{id}/sendMail`
/// (ADR-0024) for `ActionType::Email` actions whose config carries `graph_client_id` —
/// selected by `RoutingActionDispatcher` alongside `SmtpActionDispatcher`. Reuses
/// `connector_runtime::fetch_access_token`, the same Entra ID app-only (client-credentials)
/// flow ADR-0003 built for the `graph-mail`/`graph-teams`/`fabric` connectors — the cheapest of
/// the three Phase 5 send actions since the auth plumbing already exists.
pub struct GraphSendMailActionDispatcher {
    client: reqwest::Client,
}

impl GraphSendMailActionDispatcher {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

impl Default for GraphSendMailActionDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn config_str<'a>(config: &'a serde_json::Value, field: &str) -> Result<&'a str, DispatchError> {
    config
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| DispatchError::InvalidConfig(format!("missing `{field}` field")))
}

fn recipients(config: &serde_json::Value) -> Result<Vec<String>, DispatchError> {
    match config.get("to") {
        Some(serde_json::Value::String(s)) => Ok(vec![s.clone()]),
        Some(serde_json::Value::Array(items)) => {
            let addrs: Vec<String> =
                items.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            if addrs.is_empty() {
                Err(DispatchError::InvalidConfig("`to` must contain at least one address".into()))
            } else {
                Ok(addrs)
            }
        }
        _ => Err(DispatchError::InvalidConfig("missing `to` field".into())),
    }
}

#[async_trait]
impl ActionDispatcher for GraphSendMailActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        let config = &action.config;
        let token_url = config_str(config, "graph_token_url")?;
        let client_id = config_str(config, "graph_client_id")?;
        let client_secret = config_str(config, "graph_client_secret")?;
        let from_user_id = config_str(config, "graph_from_user_id")?;
        let base_url =
            config.get("graph_base_url").and_then(|v| v.as_str()).unwrap_or(DEFAULT_GRAPH_BASE_URL);
        let to_addresses = recipients(config)?;
        let subject = config
            .get("subject")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Kizashi alert: {}", event.event_type));

        let access_token = fetch_access_token(
            token_url,
            client_id,
            client_secret,
            GRAPH_SCOPE,
            self.client.clone(),
        )
        .await
        .map_err(|e| DispatchError::Unreachable(format!("Entra token fetch failed: {e}")))?;

        let body = serde_json::to_string_pretty(event)
            .map_err(|e| DispatchError::InvalidConfig(format!("failed to render event: {e}")))?;

        let to_recipients: Vec<serde_json::Value> =
            to_addresses.iter().map(|addr| json_recipient(addr)).collect();

        let payload = serde_json::json!({
            "message": {
                "subject": subject,
                "body": {"contentType": "Text", "content": body},
                "toRecipients": to_recipients,
            },
            "saveToSentItems": "false",
        });

        let response = self
            .client
            .post(format!("{base_url}/users/{from_user_id}/sendMail"))
            .bearer_auth(&access_token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| DispatchError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(DispatchError::Unreachable(format!(
                "Graph rejected the request: HTTP {}",
                response.status()
            )));
        }
        if !response.status().is_success() {
            return Err(DispatchError::Rejected(response.status().as_u16()));
        }

        Ok(serde_json::json!({"sent_to": to_addresses}))
    }
}

fn json_recipient(address: &str) -> serde_json::Value {
    serde_json::json!({"emailAddress": {"address": address}})
}
