#[path = "handlers_test.rs"]
#[cfg(test)]
mod handlers_test;

use crate::event_query_repository::{EventFilter, EventQueryRepository};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct DashboardState {
    pub event_query_repository: Arc<dyn EventQueryRepository>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Every handler trusts `X-Tenant-Id` as set by Query Gateway after resolving the caller's
/// token (spec §8: "gateway layer: auth context scopes all downstream queries") — Dashboard
/// API never re-derives identity itself, and refuses to serve a request missing it.
fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

#[derive(serde::Deserialize)]
pub struct ListEventsQuery {
    pub event_type: Option<String>,
    pub group_key: Option<String>,
    pub status: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    100
}

fn parse_status(raw: &str) -> Result<common::EventStatus, String> {
    match raw {
        "new" => Ok(common::EventStatus::New),
        "triggered" => Ok(common::EventStatus::Triggered),
        "actioned" => Ok(common::EventStatus::Actioned),
        "dismissed" => Ok(common::EventStatus::Dismissed),
        _ => Err(format!("unknown status `{raw}`")),
    }
}

pub async fn list_events(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Query(query): Query<ListEventsQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return error_response(status, message),
    };

    let status = match query.status.as_deref().map(parse_status).transpose() {
        Ok(status) => status,
        Err(message) => return error_response(StatusCode::BAD_REQUEST, message),
    };

    let filter = EventFilter {
        event_type: query.event_type,
        group_key: query.group_key,
        status,
        since: query.since,
        until: query.until,
        limit: query.limit,
    };

    match state.event_query_repository.list_events(tenant_id, &filter).await {
        Ok(events) => Json(events).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_event(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return error_response(status, message),
    };

    match state.event_query_repository.get_event(tenant_id, id).await {
        Ok(Some(event)) => Json(event).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("no event with id {id}")),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
