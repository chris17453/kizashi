#[path = "agents_handler_test.rs"]
#[cfg(test)]
mod agents_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, ConnectorStatSummary};
use askama::Template;
use axum::extract::{Path, State};
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

#[derive(Template)]
#[template(path = "agents.html")]
struct AgentsTemplate {
    show_nav: bool,
    agents: Vec<AgentRow>,
    error: Option<String>,
}

pub async fn get_agents(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let agents = match state.agents_client.list_agents(session.tenant_id).await {
        Ok(agents) => agents,
        Err(e) => {
            return Html(
                AgentsTemplate { show_nav: true, agents: vec![], error: Some(e.to_string()) }
                    .render()
                    .unwrap(),
            )
            .into_response();
        }
    };

    let stats = state.stats_client.connector_stats(session.tenant_id).await.unwrap_or_default();

    Html(
        AgentsTemplate { show_nav: true, agents: join_agent_stats(agents, stats), error: None }
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
                let agents =
                    state.agents_client.list_agents(session.tenant_id).await.unwrap_or_default();
                let stats =
                    state.stats_client.connector_stats(session.tenant_id).await.unwrap_or_default();
                return Html(
                    AgentsTemplate {
                        show_nav: true,
                        agents: join_agent_stats(agents, stats),
                        error: Some("config must be valid JSON".to_string()),
                    }
                    .render()
                    .unwrap(),
                )
                .into_response();
            }
        }
    };

    if let Err(e) = state
        .agents_client
        .register_agent(session.tenant_id, &form.connector_type, &form.name, config)
        .await
    {
        let agents = state.agents_client.list_agents(session.tenant_id).await.unwrap_or_default();
        let stats = state.stats_client.connector_stats(session.tenant_id).await.unwrap_or_default();
        return Html(
            AgentsTemplate {
                show_nav: true,
                agents: join_agent_stats(agents, stats),
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response();
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

    if let Ok(agents) = state.agents_client.list_agents(session.tenant_id).await {
        if let Some(mut agent) = agents.into_iter().find(|a| a.id == id) {
            agent.enabled = !agent.enabled;
            let _ = state.agents_client.update_agent(&agent).await;
        }
    }
    Redirect::to("/agents").into_response()
}
