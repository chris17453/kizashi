#[path = "teams_alert_action_dispatcher_test.rs"]
#[cfg(test)]
mod teams_alert_action_dispatcher_test;

use async_trait::async_trait;
use common::{ActionRef, Event};

use crate::action_dispatcher::DispatchError;
use crate::ActionDispatcher;

/// Sends `ActionType::TeamsAlert` actions as a real Microsoft Teams "Connector Card"
/// (`@type: MessageCard`) — the JSON shape a Teams incoming webhook actually validates and
/// requires (https://learn.microsoft.com/microsoftteams/platform/webhooks-and-connectors/
/// how-to/connectors-using). `HttpActionDispatcher`'s generic `{action_type, action_config,
/// event}` envelope, previously used for every action type including `TeamsAlert` despite its
/// own doc comment claiming Teams support, is rejected by a real Teams webhook (400 — the
/// payload isn't a card Teams recognizes). This is the fix: format the card Teams expects,
/// selected by `RoutingActionDispatcher` for `ActionType::TeamsAlert`, everything else keeps
/// using the generic dispatcher (`Webhook`/`CreateTicket`/`Custom` are intentionally
/// bring-your-own-shape).
pub struct TeamsAlertActionDispatcher {
    egress_proxy_url: Option<String>,
}

impl TeamsAlertActionDispatcher {
    pub fn new(egress_proxy_url: Option<String>) -> Self {
        Self { egress_proxy_url }
    }
}

fn message_card(action: &ActionRef, event: &Event) -> serde_json::Value {
    let title =
        action.config.get("title").and_then(|v| v.as_str()).unwrap_or("Kizashi alert").to_string();
    serde_json::json!({
        "@type": "MessageCard",
        "@context": "http://schema.org/extensions",
        "summary": format!("{title}: {}", event.event_type),
        "themeColor": "E81123",
        "title": title,
        "sections": [{
            "activityTitle": event.event_type,
            "activitySubtitle": event.entity_ref,
            "facts": [
                {"name": "Event type", "value": event.event_type},
                {"name": "Entity", "value": event.entity_ref},
                {"name": "Group key", "value": event.group_key},
                {"name": "Occurred at", "value": event.occurred_at.to_rfc3339()},
                {"name": "Payload", "value": event.payload.to_string()},
            ],
            "markdown": true,
        }],
    })
}

#[async_trait]
impl ActionDispatcher for TeamsAlertActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        let url =
            action.config.get("url").and_then(|v| v.as_str()).ok_or(DispatchError::MissingUrl)?;

        let client = common::build_outbound_client(
            self.egress_proxy_url.as_deref(),
            event.tenant_id,
            "action-executor",
        )
        .map_err(|e| DispatchError::Unreachable(e.to_string()))?;

        let response = client
            .post(url)
            .json(&message_card(action, event))
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
