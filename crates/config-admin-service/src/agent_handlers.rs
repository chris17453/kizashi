#[path = "agent_handlers_test.rs"]
#[cfg(test)]
mod agent_handlers_test;

use crate::agent_publisher::AgentPublisher;
use crate::agent_repository::{AgentRepository, AgentRepositoryError};
use crate::handlers::{require_operator, tenant_id_from_headers, tenant_mismatch};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::{Agent, AgentChangeEvent};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AgentState {
    pub agent_repository: Arc<dyn AgentRepository>,
    pub agent_publisher: Arc<dyn AgentPublisher>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn agent_error_response(e: AgentRepositoryError) -> Response {
    match e {
        AgentRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no agent with id {id}"))
        }
        AgentRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

/// POST /v1/agents — registers a new agent (a connector instance for a tenant). This is the
/// entity that never existed before: previously the 6 connector binaries were configured only
/// by env vars, with no service that knew of their existence.
pub async fn create_agent(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Json(agent): Json<Agent>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, agent.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    match state.agent_repository.create(agent).await {
        Ok(created) => {
            let event = AgentChangeEvent::Upserted(created.clone());
            if let Err(e) = state.agent_publisher.publish_agent_changed(&event).await {
                tracing::error!(agent_id = %created.id, error = %e, "failed to publish agent.changed");
            }
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => agent_error_response(e),
    }
}

pub async fn update_agent(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut agent): Json<Agent>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, agent.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    agent.id = id;
    match state.agent_repository.update(agent).await {
        Ok(updated) => {
            let event = AgentChangeEvent::Upserted(updated.clone());
            if let Err(e) = state.agent_publisher.publish_agent_changed(&event).await {
                tracing::error!(agent_id = %updated.id, error = %e, "failed to publish agent.changed");
            }
            Json(updated).into_response()
        }
        Err(e) => agent_error_response(e),
    }
}

pub async fn get_agent(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.agent_repository.get(tenant_id, id).await {
        Ok(Some(agent)) => Json(agent).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("no agent with id {id}")),
        Err(e) => agent_error_response(e),
    }
}

/// GET /v1/agents/by-name/:name — the lookup Ingestion Gateway uses to enforce an agent's
/// enabled/disabled status at ingest time, matched on the same `name == connector_id`
/// convention `AgentsClient` documents.
pub async fn get_agent_by_name(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.agent_repository.find_by_name(tenant_id, &name).await {
        Ok(Some(agent)) => Json(agent).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("no agent named {name}")),
        Err(e) => agent_error_response(e),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ListAgentsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    25
}

#[derive(Debug, serde::Serialize)]
pub struct ListAgentsResponse {
    pub agents: Vec<Agent>,
    pub has_more: bool,
}

pub async fn list_agents(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Query(query): Query<ListAgentsQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.agent_repository.list(tenant_id, query.limit + 1, query.offset).await {
        Ok(mut agents) => {
            let has_more = agents.len() as i64 > query.limit;
            agents.truncate(query.limit as usize);
            Json(ListAgentsResponse { agents, has_more }).into_response()
        }
        Err(e) => agent_error_response(e),
    }
}

pub async fn delete_agent(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    match state.agent_repository.delete(tenant_id, id).await {
        Ok(()) => {
            let event = AgentChangeEvent::Deleted { id, tenant_id };
            if let Err(e) = state.agent_publisher.publish_agent_changed(&event).await {
                tracing::error!(agent_id = %id, error = %e, "failed to publish agent.changed");
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => agent_error_response(e),
    }
}
