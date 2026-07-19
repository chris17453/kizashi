#[path = "routing_action_dispatcher_test.rs"]
#[cfg(test)]
mod routing_action_dispatcher_test;

use async_trait::async_trait;
use common::{ActionRef, ActionType, Event};

use crate::action_dispatcher::{ActionDispatcher, DispatchError, HttpActionDispatcher};
use crate::graph_send_mail_action_dispatcher::GraphSendMailActionDispatcher;
use crate::smtp_action_dispatcher::SmtpActionDispatcher;
use crate::teams_alert_action_dispatcher::TeamsAlertActionDispatcher;

/// Routes each action to the dispatcher that can actually fulfill it (ADR-0023/ADR-0024): an
/// `ActionType::Email` action whose config carries `smtp_host` is a real SMTP send via
/// `SmtpActionDispatcher`; one carrying `graph_client_id` is a real Microsoft Graph
/// send-as-user via `GraphSendMailActionDispatcher`; `ActionType::TeamsAlert` is a real Teams
/// "Connector Card" via `TeamsAlertActionDispatcher` (the generic dispatcher's raw JSON
/// envelope is rejected by a real Teams incoming webhook); everything else (including `Email`
/// actions still configured as a webhook, for backward compatibility with ADR-0007's original
/// "everything is HTTP POST" model) goes to `HttpActionDispatcher`. This is the dispatcher
/// `main.rs` actually wires up.
pub struct RoutingActionDispatcher {
    http: HttpActionDispatcher,
    smtp: SmtpActionDispatcher,
    graph: GraphSendMailActionDispatcher,
    teams: TeamsAlertActionDispatcher,
}

impl RoutingActionDispatcher {
    pub fn new(egress_proxy_url: Option<String>) -> Self {
        Self {
            http: HttpActionDispatcher::new(egress_proxy_url.clone()),
            smtp: SmtpActionDispatcher::new(),
            graph: GraphSendMailActionDispatcher::new(),
            teams: TeamsAlertActionDispatcher::new(egress_proxy_url),
        }
    }
}

fn is_smtp_email(action: &ActionRef) -> bool {
    action.action_type == ActionType::Email && action.config.get("smtp_host").is_some()
}

fn is_graph_email(action: &ActionRef) -> bool {
    action.action_type == ActionType::Email && action.config.get("graph_client_id").is_some()
}

#[async_trait]
impl ActionDispatcher for RoutingActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        if is_smtp_email(action) {
            self.smtp.dispatch(action, event).await
        } else if is_graph_email(action) {
            self.graph.dispatch(action, event).await
        } else if action.action_type == ActionType::TeamsAlert {
            self.teams.dispatch(action, event).await
        } else {
            self.http.dispatch(action, event).await
        }
    }
}
