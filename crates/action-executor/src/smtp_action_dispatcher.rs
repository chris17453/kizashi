#[path = "smtp_action_dispatcher_test.rs"]
#[cfg(test)]
mod smtp_action_dispatcher_test;

use async_trait::async_trait;
use common::{ActionRef, Event};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::action_dispatcher::{ActionDispatcher, DispatchError};

/// Sends an actual SMTP email for `ActionType::Email` actions whose config carries SMTP
/// settings (`smtp_host` present) — unlike `HttpActionDispatcher`, which can only reach
/// webhook-shaped HTTP endpoints, this genuinely composes and sends an RFC 5322 message via
/// `lettre` (ADR-0023). Selected by `RoutingActionDispatcher`, not used directly by
/// `main.rs`.
pub struct SmtpActionDispatcher;

impl SmtpActionDispatcher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmtpActionDispatcher {
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
impl ActionDispatcher for SmtpActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        let config = &action.config;
        let smtp_host = config_str(config, "smtp_host")?;
        let smtp_port = config.get("smtp_port").and_then(|v| v.as_u64()).unwrap_or(587) as u16;
        let use_tls = config.get("smtp_use_tls").and_then(|v| v.as_bool()).unwrap_or(true);
        let from = config_str(config, "from")?;
        let to_addresses = recipients(config)?;
        let subject = config
            .get("subject")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Kizashi alert: {}", event.event_type));

        let body = serde_json::to_string_pretty(event)
            .map_err(|e| DispatchError::InvalidConfig(format!("failed to render event: {e}")))?;

        let from_mailbox: Mailbox = from
            .parse()
            .map_err(|e| DispatchError::InvalidConfig(format!("invalid `from` address: {e}")))?;

        let mut builder = Message::builder().from(from_mailbox).subject(subject);
        for addr in &to_addresses {
            let mailbox: Mailbox = addr
                .parse()
                .map_err(|e| DispatchError::InvalidConfig(format!("invalid `to` address: {e}")))?;
            builder = builder.to(mailbox);
        }
        let message = builder
            .body(body)
            .map_err(|e| DispatchError::InvalidConfig(format!("failed to build message: {e}")))?;

        let mut transport_builder = if use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
                .map_err(|e| DispatchError::Unreachable(e.to_string()))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host)
        };
        transport_builder = transport_builder.port(smtp_port);

        if let (Some(username), Some(password)) = (
            config.get("smtp_username").and_then(|v| v.as_str()),
            config.get("smtp_password").and_then(|v| v.as_str()),
        ) {
            transport_builder =
                transport_builder.credentials(Credentials::new(username.into(), password.into()));
        }

        let transport = transport_builder.build();

        transport
            .send(message)
            .await
            .map_err(|e| DispatchError::Unreachable(format!("SMTP send failed: {e}")))?;

        Ok(serde_json::json!({"sent_to": to_addresses}))
    }
}
