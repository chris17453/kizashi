#[path = "agent_detail_handler_test.rs"]
#[cfg(test)]
mod agent_detail_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use common::Agent;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "agent_detail.html")]
struct AgentDetailTemplate {
    show_nav: bool,
    agent: Option<Agent>,
    records: Vec<RecordSummary>,
    error: Option<String>,
}

/// GET /agents/:id — the per-agent data drill-down: the agent's own registration plus the
/// most recent raw records its connector has ingested (matched on `agent.name ==
/// record.connector_id`, same convention as the Agents list's status column).
pub async fn get_agent_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let agents = match state.agents_client.list_agents(session.tenant_id).await {
        Ok(agents) => agents,
        Err(e) => {
            return Html(
                AgentDetailTemplate {
                    show_nav: true,
                    agent: None,
                    records: vec![],
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let Some(agent) = agents.into_iter().find(|a| a.id == id) else {
        return Html(
            AgentDetailTemplate {
                show_nav: true,
                agent: None,
                records: vec![],
                error: Some("no agent with that id".to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response();
    };

    let records = state
        .stats_client
        .records_by_connector(session.tenant_id, &agent.name)
        .await
        .unwrap_or_default();

    Html(
        AgentDetailTemplate { show_nav: true, agent: Some(agent), records, error: None }
            .render()
            .unwrap(),
    )
    .into_response()
}
