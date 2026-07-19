#[path = "agent_handlers_test.rs"]
#[cfg(test)]
mod agent_handlers_test;

use crate::agent_repository::{AgentRepository, AgentRepositoryError};
use crate::handlers::{tenant_id_from_headers, tenant_mismatch};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Agent;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AgentState {
    pub agent_repository: Arc<dyn AgentRepository>,
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
    match state.agent_repository.create(agent).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
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
    agent.id = id;
    match state.agent_repository.update(agent).await {
        Ok(updated) => Json(updated).into_response(),
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

pub async fn list_agents(State(state): State<AgentState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.agent_repository.list(tenant_id).await {
        Ok(agents) => Json(agents).into_response(),
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
    match state.agent_repository.delete(tenant_id, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => agent_error_response(e),
    }
}
