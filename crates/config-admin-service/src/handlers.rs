#[path = "handlers_test.rs"]
#[cfg(test)]
mod handlers_test;

#[path = "audit_log_handlers_test.rs"]
#[cfg(test)]
mod audit_log_handlers_test;

use crate::audit_log::AuditLogReader;
use crate::mapping_publisher::MappingPublisher;
use crate::normalization_mapping_repository::{
    NormalizationMappingRepository, NormalizationMappingRepositoryError,
};
use crate::trigger_definition_repository::{
    TriggerDefinitionRepository, TriggerDefinitionRepositoryError,
};
use crate::trigger_publisher::TriggerPublisher;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::{NormalizationMapping, Role, TriggerDefinition};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AdminState {
    pub trigger_repository: Arc<dyn TriggerDefinitionRepository>,
    pub mapping_repository: Arc<dyn NormalizationMappingRepository>,
    pub audit_reader: Arc<dyn AuditLogReader>,
    pub trigger_publisher: Arc<dyn TriggerPublisher>,
    pub mapping_publisher: Arc<dyn MappingPublisher>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Every handler trusts `X-Tenant-Id` as set by whatever gateway sits in front of this service
/// (spec §8) — Config Admin Service never derives identity itself, matching Dashboard API's
/// convention.
pub(crate) fn tenant_id_from_headers(
    headers: &HeaderMap,
) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

/// Every write handler that records an audit-log entry trusts `X-Username` for the real actor
/// identity, forwarded by Console UI's session alongside `X-Tenant-Id`/`X-Role` — without it the
/// audit trail (CLAUDE.md §5) can only prove *which tenant* changed something, never *who*.
pub(crate) fn username_from_headers(
    headers: &HeaderMap,
) -> Result<String, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))?;
    Ok(raw.to_string())
}

fn trigger_error_response(e: TriggerDefinitionRepositoryError) -> Response {
    match e {
        TriggerDefinitionRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no trigger definition with id {id}"))
        }
        TriggerDefinitionRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

fn mapping_error_response(e: NormalizationMappingRepositoryError) -> Response {
    match e {
        NormalizationMappingRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no normalization mapping with id {id}"))
        }
        NormalizationMappingRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

pub(crate) fn tenant_mismatch(headers: &HeaderMap, entity_tenant_id: Uuid) -> Option<Response> {
    match tenant_id_from_headers(headers) {
        Ok(tenant_id) if tenant_id == entity_tenant_id => None,
        Ok(_) => {
            Some(error_response(StatusCode::FORBIDDEN, "tenant_id does not match X-Tenant-Id"))
        }
        Err((status, msg)) => Some(error_response(status, msg)),
    }
}

/// RBAC v1 (ADR-0016): every write handler trusts `X-Role`, forwarded by Console UI's session
/// alongside `X-Tenant-Id`, the same trust boundary already established for tenant identity —
/// this service has no gateway in front of it (ADR-0010) to enforce roles at a proxy layer.
fn role_from_headers(headers: &HeaderMap) -> Result<Role, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Role header"))?;
    raw.parse().map_err(|_| (StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
}

/// Rejects the request unless the caller's role is at least `Operator` — the check every
/// create/update write-path handler runs before touching a repository (ADR-0016 v1 scope:
/// trigger-definition and normalization-mapping writes only).
pub(crate) fn require_operator(headers: &HeaderMap) -> Option<Response> {
    match role_from_headers(headers) {
        Ok(role) if role.at_least(Role::Operator) => None,
        Ok(_) => Some(error_response(
            StatusCode::FORBIDDEN,
            "role does not have permission to perform this action",
        )),
        Err((status, msg)) => Some(error_response(status, msg)),
    }
}

pub async fn create_trigger(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(trigger): Json<TriggerDefinition>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, trigger.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.trigger_repository.create(trigger, &actor).await {
        Ok(created) => {
            if let Err(e) = state.trigger_publisher.publish_trigger_changed(&created).await {
                tracing::error!(trigger_id = %created.id, error = %e, "failed to publish trigger.changed");
            }
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => trigger_error_response(e),
    }
}

pub async fn update_trigger(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut trigger): Json<TriggerDefinition>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, trigger.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    trigger.id = id;
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.trigger_repository.update(trigger, &actor).await {
        Ok(updated) => {
            if let Err(e) = state.trigger_publisher.publish_trigger_changed(&updated).await {
                tracing::error!(trigger_id = %updated.id, error = %e, "failed to publish trigger.changed");
            }
            Json(updated).into_response()
        }
        Err(e) => trigger_error_response(e),
    }
}

pub async fn get_trigger(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.trigger_repository.get(tenant_id, id).await {
        Ok(Some(trigger)) => Json(trigger).into_response(),
        Ok(None) => {
            error_response(StatusCode::NOT_FOUND, format!("no trigger definition with id {id}"))
        }
        Err(e) => trigger_error_response(e),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ListTriggersQuery {
    #[serde(default = "default_list_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_list_limit() -> i64 {
    25
}

#[derive(Debug, serde::Serialize)]
pub struct ListTriggersResponse {
    pub triggers: Vec<TriggerDefinition>,
    pub has_more: bool,
}

pub async fn list_triggers(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<ListTriggersQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.trigger_repository.list(tenant_id, query.limit + 1, query.offset).await {
        Ok(mut triggers) => {
            let has_more = triggers.len() as i64 > query.limit;
            triggers.truncate(query.limit as usize);
            Json(ListTriggersResponse { triggers, has_more }).into_response()
        }
        Err(e) => trigger_error_response(e),
    }
}

pub async fn create_mapping(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(mapping): Json<NormalizationMapping>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, mapping.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.mapping_repository.create(mapping, &actor).await {
        Ok(created) => {
            if let Err(e) = state.mapping_publisher.publish_mapping_changed(&created).await {
                tracing::error!(mapping_id = %created.id, error = %e, "failed to publish mapping.changed");
            }
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => mapping_error_response(e),
    }
}

pub async fn update_mapping(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut mapping): Json<NormalizationMapping>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, mapping.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    mapping.id = id;
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.mapping_repository.update(mapping, &actor).await {
        Ok(updated) => {
            if let Err(e) = state.mapping_publisher.publish_mapping_changed(&updated).await {
                tracing::error!(mapping_id = %updated.id, error = %e, "failed to publish mapping.changed");
            }
            Json(updated).into_response()
        }
        Err(e) => mapping_error_response(e),
    }
}

pub async fn get_mapping(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.mapping_repository.get(tenant_id, id).await {
        Ok(Some(mapping)) => Json(mapping).into_response(),
        Ok(None) => {
            error_response(StatusCode::NOT_FOUND, format!("no normalization mapping with id {id}"))
        }
        Err(e) => mapping_error_response(e),
    }
}

pub async fn list_mappings(State(state): State<AdminState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.mapping_repository.list(tenant_id).await {
        Ok(mappings) => Json(mappings).into_response(),
        Err(e) => mapping_error_response(e),
    }
}

/// GET /v1/audit-log/:entity_id — the audit trail CLAUDE.md §5 requires exists for every
/// admin/config entity, regardless of type (trigger definition or normalization mapping share
/// the same `config_audit_log` table, keyed by `entity_id`).
pub async fn get_audit_log(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(entity_id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.audit_reader.list_for_entity(tenant_id, entity_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "audit log lookup failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

fn default_recent_audit_log_limit() -> u32 {
    50
}

/// Callers can ask for more, but never get an unbounded query — a large tenant's audit table
/// could otherwise turn one request into a full table scan back to day one.
const MAX_RECENT_AUDIT_LOG_LIMIT: u32 = 200;

#[derive(Debug, serde::Deserialize)]
pub struct RecentAuditLogQuery {
    #[serde(default = "default_recent_audit_log_limit")]
    pub limit: u32,
    #[serde(default)]
    pub before: Option<chrono::DateTime<chrono::Utc>>,
}

/// GET /v1/audit-log — the chronological, cross-entity audit feed CLAUDE.md §5 / SOC2-style
/// compliance review needs ("show me every admin action in the last N days"), as opposed to
/// `get_audit_log`'s single-entity history. Deliberately no role check (read-only, same
/// convention as the entity-scoped endpoint) — only `X-Tenant-Id` scoping.
pub async fn get_recent_audit_log(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<RecentAuditLogQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let limit = query.limit.min(MAX_RECENT_AUDIT_LOG_LIMIT) as i64;
    match state.audit_reader.list_recent(tenant_id, limit, query.before).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "recent audit log lookup failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}
