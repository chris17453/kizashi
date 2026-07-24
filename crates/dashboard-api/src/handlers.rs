#[path = "handlers_test.rs"]
#[cfg(test)]
mod handlers_test;

use crate::event_query_repository::{DailyEventCount, EventFilter, EventQueryRepository};
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
    pub record_id: Option<Uuid>,
    /// Case-insensitive search across event type, group key, and lifecycle status.
    pub search: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    100
}

#[derive(serde::Serialize)]
struct ListEventsResponse {
    events: Vec<common::Event>,
    has_more: bool,
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

fn status_str(status: common::EventStatus) -> &'static str {
    match status {
        common::EventStatus::New => "new",
        common::EventStatus::Triggered => "triggered",
        common::EventStatus::Actioned => "actioned",
        common::EventStatus::Dismissed => "dismissed",
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
        record_id: query.record_id,
        search: query.search,
        limit: query.limit + 1,
        offset: query.offset,
    };

    match state.event_query_repository.list_events(tenant_id, &filter).await {
        Ok(mut events) => {
            let has_more = events.len() as u32 > query.limit;
            events.truncate(query.limit as usize);
            Json(ListEventsResponse { events, has_more }).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "event query repository error");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
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
        Err(e) => {
            tracing::error!(error = %e, "event query repository error");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

#[derive(serde::Deserialize)]
pub struct UpdateEventStatusRequest {
    pub status: String,
}

/// PATCH /v1/events/:id — advances the operator lifecycle projection without changing the
/// original signal payload or timestamps. Tenant scope comes exclusively from the gateway.
pub async fn update_event_status(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateEventStatusRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return error_response(status, message),
    };
    let status = match parse_status(&request.status.to_ascii_lowercase()) {
        Ok(status) => status,
        Err(message) => return error_response(StatusCode::BAD_REQUEST, message),
    };
    let current_status = match state.event_query_repository.get_event(tenant_id, id).await {
        Ok(None) => return error_response(StatusCode::NOT_FOUND, format!("no event with id {id}")),
        Err(e) => {
            tracing::error!(error = %e, "event lookup failed before lifecycle update");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "event lookup failed");
        }
        Ok(Some(event)) => event.status,
    };
    if current_status == status {
        return Json(serde_json::json!({"id": id, "status": status_str(status), "changed": false}))
            .into_response();
    }
    let actor = headers
        .get("x-actor")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("operator");
    match state
        .event_query_repository
        .update_event_status(tenant_id, id, current_status, status, actor)
        .await
    {
        Ok(()) => Json(serde_json::json!({"id": id, "status": status_str(status)})).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "event lifecycle update failed");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "event lifecycle update failed")
        }
    }
}

/// GET /v1/events/:id/status-history — immutable operator disposition history for one signal.
pub async fn list_event_status_history(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return error_response(status, message),
    };
    match state.event_query_repository.list_status_history(tenant_id, id).await {
        Ok(history) => Json(history).into_response(),
        Err(error) => {
            tracing::error!(%error, "event status history query failed");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "event status history unavailable")
        }
    }
}

#[derive(serde::Deserialize)]
pub struct DailyEventCountsQuery {
    pub event_type: Option<String>,
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

#[derive(serde::Serialize)]
struct DailyEventCountView {
    date: String,
    count: u64,
}

impl From<DailyEventCount> for DailyEventCountView {
    fn from(c: DailyEventCount) -> Self {
        Self { date: c.date.format("%Y-%m-%d").to_string(), count: c.count }
    }
}

#[derive(serde::Serialize)]
struct DailyEventCountsResponse {
    counts: Vec<DailyEventCountView>,
}

/// GET /v1/events/daily-counts — powers the Events page's over-time chart (spec §7's
/// dashboard requirement, previously unimplemented — the Events page was a flat table with no
/// trend visibility at all).
pub async fn daily_event_counts(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Query(query): Query<DailyEventCountsQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return error_response(status, message),
    };

    match state
        .event_query_repository
        .count_by_day(tenant_id, query.event_type.as_deref(), query.since, query.until)
        .await
    {
        Ok(counts) => Json(DailyEventCountsResponse {
            counts: counts.into_iter().map(DailyEventCountView::from).collect(),
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "event query repository error");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}
