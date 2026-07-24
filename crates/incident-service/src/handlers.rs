#[path = "handlers_test.rs"]
#[cfg(test)]
mod handlers_test;

use crate::audit_log::{AuditLogError, AuditLogReader};
use crate::incident_repository::{IncidentRepository, IncidentRepositoryError};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::{Incident, IncidentNote, IncidentStatus, Role};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct IncidentState {
    pub incident_repository: Arc<dyn IncidentRepository>,
    pub audit_log_reader: Arc<dyn AuditLogReader>,
}

#[derive(serde::Deserialize)]
pub struct ListAuditLogQuery {
    #[serde(default = "default_audit_limit")]
    limit: u32,
    before: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_audit_limit() -> u32 {
    50
}

pub async fn list_audit_log(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Query(query): Query<ListAuditLogQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.audit_log_reader.list_recent(tenant_id, query.limit.min(200), query.before).await {
        Ok(entries) => Json(entries).into_response(),
        Err(AuditLogError::Backend(message)) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

pub async fn list_entity_audit_log(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Path(entity_id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.audit_log_reader.list_for_entity(tenant_id, entity_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(AuditLogError::Backend(message)) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Every handler trusts `X-Tenant-Id` as set by whatever gateway sits in front of this service
/// (spec §8) — same trust boundary as config-admin-service.
fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))?;
    Ok(raw.to_string())
}

fn role_from_headers(headers: &HeaderMap) -> Result<Role, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Role header"))?;
    raw.parse().map_err(|_| (StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
}

/// Rejects the request unless the caller's role is at least `Operator` — every write-path
/// handler runs this before touching the repository (RBAC v1, ADR-0016 scope extended here).
fn require_operator(headers: &HeaderMap) -> Option<Response> {
    match role_from_headers(headers) {
        Ok(role) if role.at_least(Role::Operator) => None,
        Ok(_) => Some(error_response(
            StatusCode::FORBIDDEN,
            "role does not have permission to perform this action",
        )),
        Err((status, msg)) => Some(error_response(status, msg)),
    }
}

fn incident_error_response(e: IncidentRepositoryError) -> Response {
    match e {
        IncidentRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no incident with id {id}"))
        }
        IncidentRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

#[derive(serde::Serialize)]
struct IncidentDetailResponse {
    #[serde(flatten)]
    incident: Incident,
    event_ids: Vec<Uuid>,
    notes: Vec<IncidentNote>,
}

async fn detail_response(
    state: &IncidentState,
    incident: Incident,
) -> Result<IncidentDetailResponse, IncidentRepositoryError> {
    let event_ids = state.incident_repository.list_linked_event_ids(incident.id).await?;
    let notes = state.incident_repository.list_notes(incident.tenant_id, incident.id).await?;
    Ok(IncidentDetailResponse { incident, event_ids, notes })
}

#[derive(serde::Deserialize)]
pub struct AddIncidentNoteRequest {
    body: String,
}

pub async fn add_incident_note(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<AddIncidentNoteRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let body = request.body.trim();
    if body.is_empty() || body.len() > 10_000 {
        return error_response(
            StatusCode::BAD_REQUEST,
            "note body must contain 1-10000 characters",
        );
    }
    let actor = match username_from_headers(&headers) {
        Ok(actor) => actor,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.incident_repository.add_note(tenant_id, id, &actor, body).await {
        Ok(note) => (StatusCode::CREATED, Json(note)).into_response(),
        Err(e) => incident_error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateIncidentRequest {
    #[serde(flatten)]
    incident: Incident,
    #[serde(default)]
    initial_event_ids: Vec<Uuid>,
}

pub async fn create_incident(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Json(request): Json<CreateIncidentRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if tenant_id != request.incident.tenant_id {
        return error_response(StatusCode::FORBIDDEN, "tenant_id does not match X-Tenant-Id");
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state
        .incident_repository
        .create(request.incident, &request.initial_event_ids, &actor)
        .await
    {
        Ok(created) => match detail_response(&state, created).await {
            Ok(detail) => (StatusCode::CREATED, Json(detail)).into_response(),
            Err(e) => incident_error_response(e),
        },
        Err(e) => incident_error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct ListIncidentsQuery {
    #[serde(default)]
    status: Option<String>,
}

pub async fn list_incidents(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Query(query): Query<ListIncidentsQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let status_filter = match query.status.as_deref().map(IncidentStatus::from_str) {
        Some(Ok(status)) => Some(status),
        Some(Err(_)) => {
            return error_response(StatusCode::BAD_REQUEST, "status is not a recognized value")
        }
        None => None,
    };

    match state.incident_repository.list(tenant_id, status_filter).await {
        Ok(incidents) => {
            let mut details = Vec::with_capacity(incidents.len());
            for incident in incidents {
                match detail_response(&state, incident).await {
                    Ok(detail) => details.push(detail),
                    Err(e) => return incident_error_response(e),
                }
            }
            Json(details).into_response()
        }
        Err(e) => incident_error_response(e),
    }
}

pub async fn get_incident(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.incident_repository.get(tenant_id, id).await {
        Ok(Some(incident)) => match detail_response(&state, incident).await {
            Ok(detail) => Json(detail).into_response(),
            Err(e) => incident_error_response(e),
        },
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("no incident with id {id}")),
        Err(e) => incident_error_response(e),
    }
}

pub async fn update_incident(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut incident): Json<Incident>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if tenant_id != incident.tenant_id {
        return error_response(StatusCode::FORBIDDEN, "tenant_id does not match X-Tenant-Id");
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    incident.id = id;
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.incident_repository.update(incident, &actor).await {
        Ok(updated) => match detail_response(&state, updated).await {
            Ok(detail) => Json(detail).into_response(),
            Err(e) => incident_error_response(e),
        },
        Err(e) => incident_error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct LinkEventRequest {
    event_id: Uuid,
}

pub async fn link_event(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Path(incident_id): Path<Uuid>,
    Json(request): Json<LinkEventRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state
        .incident_repository
        .link_event(tenant_id, incident_id, request.event_id, &actor)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => incident_error_response(e),
    }
}

pub async fn unlink_event(
    State(state): State<IncidentState>,
    headers: HeaderMap,
    Path((incident_id, event_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.incident_repository.unlink_event(tenant_id, incident_id, event_id, &actor).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => incident_error_response(e),
    }
}
