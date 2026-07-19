#[path = "agents_handler_test.rs"]
#[cfg(test)]
mod agents_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, ConnectorStatSummary};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use common::Agent;
use uuid::Uuid;

/// One row of the Agents table: a registered `Agent` joined against Ingestion Service's
/// per-connector stats, matched on `agent.name == connector_stats.connector_id` (the
/// operational convention `AgentsClient` documents — an agent's registered `name` is what the
/// deployed connector's `CONNECTOR_ID` env var is set to). No match means the agent has never
/// ingested anything yet, not an error.
struct AgentRow {
    id: Uuid,
    connector_type: String,
    name: String,
    enabled: bool,
    record_count: Option<i64>,
    last_ingested_at: Option<DateTime<Utc>>,
}

fn join_agent_stats(agents: Vec<Agent>, stats: Vec<ConnectorStatSummary>) -> Vec<AgentRow> {
    agents
        .into_iter()
        .map(|agent| {
            let matched = stats.iter().find(|s| s.connector_id == agent.name);
            AgentRow {
                id: agent.id,
                connector_type: agent.connector_type,
                name: agent.name,
                enabled: agent.enabled,
                record_count: matched.map(|s| s.record_count),
                last_ingested_at: matched.map(|s| s.last_ingested_at),
            }
        })
        .collect()
}

fn default_page() -> i64 {
    0
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct AgentsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
}

#[derive(Template)]
#[template(path = "agents.html")]
struct AgentsTemplate {
    show_nav: bool,
    agents: Vec<AgentRow>,
    page: i64,
    has_more: bool,
    /// RBAC v1 (ADR-0016): hides the register form and enable/disable/remove buttons from a
    /// `Viewer` — the backend doesn't enforce this particular write path yet (only
    /// config-admin-service's trigger/mapping writes and retention-service's policy writes do),
    /// so this is presentation-layer only for now, not a substitute for server-side gating.
    can_write: bool,
    error: Option<String>,
}

pub async fn get_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);

    let page = query.page.max(0);
    let result = state
        .agents_client
        .list_agents(session.tenant_id, DEFAULT_PAGE_SIZE, page * DEFAULT_PAGE_SIZE)
        .await;
    let (agents, has_more) = match result {
        Ok(page_result) => (page_result.agents, page_result.has_more),
        Err(e) => {
            return Html(
                AgentsTemplate {
                    show_nav: true,
                    agents: vec![],
                    page,
                    has_more: false,
                    can_write,
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let stats = state.stats_client.connector_stats(session.tenant_id).await.unwrap_or_default();

    Html(
        AgentsTemplate {
            show_nav: true,
            agents: join_agent_stats(agents, stats),
            page,
            has_more,
            can_write,
            error: None,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct RegisterAgentForm {
    connector_type: String,
    name: String,
    #[serde(default)]
    config: String,
}

async fn rerender_with_error(
    state: &AppState,
    tenant_id: Uuid,
    can_write: bool,
    error: String,
) -> Response {
    let agents = state
        .agents_client
        .list_agents(tenant_id, DEFAULT_PAGE_SIZE, 0)
        .await
        .map(|p| p.agents)
        .unwrap_or_default();
    let stats = state.stats_client.connector_stats(tenant_id).await.unwrap_or_default();
    Html(
        AgentsTemplate {
            show_nav: true,
            agents: join_agent_stats(agents, stats),
            page: 0,
            has_more: false,
            can_write,
            error: Some(error),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn post_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<RegisterAgentForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let config: serde_json::Value = if form.config.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&form.config) {
            Ok(value) => value,
            Err(_) => {
                return rerender_with_error(
                    &state,
                    session.tenant_id,
                    session.role.at_least(common::Role::Operator),
                    "config must be valid JSON".to_string(),
                )
                .await;
            }
        }
    };

    if let Err(e) = state
        .agents_client
        .register_agent(session.tenant_id, &form.connector_type, &form.name, config)
        .await
    {
        return rerender_with_error(
            &state,
            session.tenant_id,
            session.role.at_least(common::Role::Operator),
            e.to_string(),
        )
        .await;
    }

    Redirect::to("/agents").into_response()
}

pub async fn post_delete_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let _ = state.agents_client.delete_agent(session.tenant_id, id).await;
    Redirect::to("/agents").into_response()
}

/// POST /agents/:id/toggle — flips an agent's enabled/disabled status. This is the one place
/// that flag actually does something: Ingestion Gateway checks it on every ingest and rejects
/// a disabled agent's data (previously stored but never enforced anywhere).
pub async fn post_toggle_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    if let Ok(Some(mut agent)) = state.agents_client.get_agent(session.tenant_id, id).await {
        agent.enabled = !agent.enabled;
        let _ = state.agents_client.update_agent(&agent).await;
    }
    Redirect::to("/agents").into_response()
}
