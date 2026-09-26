#[path = "api_v1_handler_test.rs"]
#[cfg(test)]
mod api_v1_handler_test;

use crate::attention_summary_handler::attention_summary_for_tenant;
use crate::connector_field_catalog::{display_name, fields_for, CONNECTOR_CATALOG};
use crate::security_overview_handler::security_overview_for_tenant;
use crate::session_guard::require_session;
use crate::{AppState, EventSummary, IncidentDetail};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use common::{
    Incident, IncidentSeverity, IncidentStatus, ReportRun, Role, SavedSearchQuery,
    TriggerDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct ApiPrincipal {
    tenant_id: Uuid,
    role: Role,
    username: String,
    query_token: String,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, axum::Json(ApiError { error: message.into() })).into_response()
}

async fn principal(state: &AppState, headers: &HeaderMap) -> Result<ApiPrincipal, Response> {
    let Some(value) = headers.get("authorization").and_then(|v| v.to_str().ok()) else {
        let session = require_session(state.session_store.as_ref(), headers).await?;
        return Ok(ApiPrincipal {
            tenant_id: session.tenant_id,
            role: session.role,
            username: session.username,
            query_token: session.bearer_token,
        });
    };
    let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))
    else {
        return Err(api_error(StatusCode::UNAUTHORIZED, "expected bearer token"));
    };
    let account = state.auth_client.introspect_service_account(token).await.map_err(|error| {
        let status = match error {
            crate::AuthClientError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            _ => StatusCode::BAD_GATEWAY,
        };
        api_error(status, error.to_string())
    })?;
    Ok(ApiPrincipal {
        tenant_id: account.tenant_id,
        role: account.role,
        username: format!("service-account:{}", account.label),
        query_token: account.query_token,
    })
}

fn require_operator(principal: &ApiPrincipal) -> Result<(), Response> {
    if principal.role.at_least(Role::Operator) {
        Ok(())
    } else {
        Err(api_error(StatusCode::FORBIDDEN, "operator role required"))
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct EventsApiQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub search: Option<String>,
    pub status: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

fn parse_time(value: Option<&String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, Response> {
    value
        .map(|value| {
            value
                .parse()
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, "timestamp must be RFC3339"))
        })
        .transpose()
}

#[derive(Debug, Serialize)]
struct EventsApiResponse {
    events: Vec<EventSummary>,
    has_more: bool,
    limit: u32,
    offset: u32,
}

pub async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let offset = query.offset.unwrap_or(0);
    let since = match parse_time(query.since.as_ref()) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let until = match parse_time(query.until.as_ref()) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state
        .events_client
        .list_events_filtered(
            &principal.query_token,
            limit,
            offset,
            since,
            until,
            query.search,
            query.status,
        )
        .await
    {
        Ok(page) => axum::Json(EventsApiResponse {
            events: page.events,
            has_more: page.has_more,
            limit,
            offset,
        })
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn export_events_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let since = match parse_time(query.since.as_ref()) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let until = match parse_time(query.until.as_ref()) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let limit = 1000u32;
    let mut events = Vec::new();
    let mut has_more = false;
    for page in 0..2u32 {
        let result = match state
            .events_client
            .list_events_filtered(
                &principal.query_token,
                limit,
                page * limit,
                since,
                until,
                query.search.clone(),
                query.status.clone(),
            )
            .await
        {
            Ok(result) => result,
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
        has_more = result.has_more;
        events.extend(result.events);
        if !has_more {
            break;
        }
    }
    let mut csv = String::from("id,event_type,status,group_key,occurred_at,record_ids\n");
    for event in events {
        let record_ids = event.record_ids.iter().map(Uuid::to_string).collect::<Vec<_>>().join(";");
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            event.id,
            api_csv_escape(&event.event_type),
            api_csv_escape(&event.status),
            api_csv_escape(&event.group_key),
            event.occurred_at.to_rfc3339(),
            api_csv_escape(&record_ids),
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"events-export-{}.csv\"", principal.tenant_id)
            .parse()
            .unwrap(),
    );
    if has_more {
        response_headers.insert("x-export-truncated", "true".parse().unwrap());
    }
    (response_headers, csv).into_response()
}

pub async fn get_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.events_client.get_event(&principal.query_token, id).await {
        Ok(Some(event)) => axum::Json(event).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "event not found"),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// GET /api/v1/events/:id/status-history — returns the immutable lifecycle transitions for one
/// signal through the same tenant-bound Query Gateway client used by the Event Detail page.
pub async fn list_event_status_history_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.events_client.get_event(&principal.query_token, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return api_error(StatusCode::NOT_FOUND, "event not found"),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
    match state.events_client.list_status_history(&principal.query_token, id).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(crate::events_client::EventsClientError::Rejected(404)) => {
            api_error(StatusCode::NOT_FOUND, "event not found")
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct Event360ApiResponse {
    event: crate::events_client::EventDetail,
    status_history: Vec<crate::events_client::StatusHistoryEntry>,
    records: Vec<crate::RecordSummary>,
    incidents: Vec<IncidentDetail>,
    executions: Vec<crate::execution_client::ActionExecutionSummary>,
    modeled_objects: Vec<common::ontology::Object>,
    object_types: Vec<common::ontology::ObjectType>,
    action_types: Vec<common::ontology::ActionType>,
    invocations: Vec<common::ontology::ActionInvocation>,
    reviews: Vec<common::ontology::ActionReview>,
    errors: Vec<String>,
}

fn ontology_object_matches_event_api(
    object: &common::ontology::Object,
    entity_ref: &str,
    record_ids: &[Uuid],
) -> bool {
    object.properties.get("id").and_then(serde_json::Value::as_str) == Some(entity_ref)
        || object.id.to_string() == entity_ref
        || object.source_lineage.as_array().is_some_and(|lineage| {
            record_ids.iter().any(|record_id| {
                let record_id = record_id.to_string();
                lineage.iter().any(|item| item.as_str() == Some(record_id.as_str()))
            })
        })
}

fn event_ref_api(value: &serde_json::Value) -> Option<Uuid> {
    value
        .get("event_id")
        .or_else(|| value.get("id"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

/// Returns the bounded, event-centric investigation context used by the Console event detail
/// page. Optional downstream joins are reported in `errors` so a degraded service does not turn
/// an otherwise useful signal into a failed request.
pub async fn get_event_360(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let event = match state.events_client.get_event(&principal.query_token, id).await {
        Ok(Some(event)) => event,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, "event not found"),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut errors = Vec::new();
    let status_history =
        match state.events_client.list_status_history(&principal.query_token, id).await {
            Ok(history) => history,
            Err(error) => {
                errors.push(format!("status history: {error}"));
                Vec::new()
            }
        };
    // Keep record retrieval deterministic and bounded without exposing a second unbounded query
    // surface.
    let mut records = Vec::new();
    for record_id in event.record_ids.iter().take(100) {
        match state.stats_client.get_record(principal.tenant_id, *record_id).await {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(error) => errors.push(format!("record {record_id}: {error}")),
        }
    }
    let incidents = match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(incidents) => incidents
            .into_iter()
            .filter(|incident| incident.event_ids.contains(&id))
            .take(100)
            .collect(),
        Err(error) => {
            errors.push(format!("incidents: {error}"));
            Vec::new()
        }
    };
    let executions =
        match state.execution_client.list_executions_for_event(principal.tenant_id, id).await {
            Ok(executions) => executions.into_iter().take(100).collect(),
            Err(error) => {
                errors.push(format!("executions: {error}"));
                Vec::new()
            }
        };
    let (modeled_objects, object_types, action_types, invocations, reviews) = if let Some(client) =
        crate::ontology_client::global()
    {
        let (types, objects, actions, invocations, reviews) = tokio::join!(
            client.list_object_types(&principal.query_token),
            client.list_objects(&principal.query_token, None),
            client.list_action_types(&principal.query_token),
            client.list_action_invocations(&principal.query_token),
            client.list_action_reviews(&principal.query_token),
        );
        let object_types = match types {
            Ok(types) => types,
            Err(error) => {
                errors.push(format!("object types: {error}"));
                Vec::new()
            }
        };
        let modeled_objects = match objects {
            Ok(objects) => objects
                .into_iter()
                .filter(|object| {
                    ontology_object_matches_event_api(object, &event.entity_ref, &event.record_ids)
                })
                .take(100)
                .collect(),
            Err(error) => {
                errors.push(format!("modeled objects: {error}"));
                Vec::new()
            }
        };
        let action_types = match actions {
            Ok(actions) => actions,
            Err(error) => {
                errors.push(format!("action types: {error}"));
                Vec::new()
            }
        };
        let invocations = match invocations {
            Ok(invocations) => invocations
                .into_iter()
                .filter(|invocation| event_ref_api(&invocation.triggering_event_ref) == Some(id))
                .take(100)
                .collect(),
            Err(error) => {
                errors.push(format!("action invocations: {error}"));
                Vec::new()
            }
        };
        let reviews = match reviews {
            Ok(reviews) => reviews
                .into_iter()
                .filter(|review| invocations.iter().any(|item| item.id == review.invocation_id))
                .take(100)
                .collect(),
            Err(error) => {
                errors.push(format!("action reviews: {error}"));
                Vec::new()
            }
        };
        (modeled_objects, object_types, action_types, invocations, reviews)
    } else {
        errors.push("ontology client unavailable".to_string());
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
    };
    axum::Json(Event360ApiResponse {
        event,
        status_history,
        records,
        incidents,
        executions,
        modeled_objects,
        object_types,
        action_types,
        invocations,
        reviews,
        errors,
    })
    .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct IncidentsApiQuery {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub assigned_to: Option<String>,
    pub q: Option<String>,
}

fn incident_api_matches(incident: &IncidentDetail, query: &IncidentsApiQuery) -> bool {
    let q = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let severity = query.severity.as_deref().unwrap_or_default().trim();
    let owner = query.assigned_to.as_deref().unwrap_or_default().trim();
    (q.is_empty()
        || incident.incident.id.to_string().contains(&q)
        || incident.incident.title.to_ascii_lowercase().contains(&q)
        || incident.incident.summary.to_ascii_lowercase().contains(&q))
        && (severity.is_empty()
            || incident.incident.severity.to_string().eq_ignore_ascii_case(severity))
        && (owner.is_empty()
            || (owner.eq_ignore_ascii_case("unassigned")
                && incident.incident.assigned_to.is_none())
            || incident
                .incident
                .assigned_to
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(owner)))
}

#[derive(Debug, Serialize)]
struct IncidentsApiResponse {
    incidents: Vec<IncidentDetail>,
}

pub async fn list_incidents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<IncidentsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let status = match query.status.as_deref() {
        Some(value) => match value.parse::<IncidentStatus>() {
            Ok(status) => Some(status),
            Err(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "status must be open, acknowledged, or resolved",
                )
            }
        },
        None => None,
    };
    match state.incidents_client.list_incidents(principal.tenant_id, status).await {
        Ok(incidents) => axum::Json(IncidentsApiResponse {
            incidents: incidents
                .into_iter()
                .filter(|incident| incident_api_matches(incident, &query))
                .collect(),
        })
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn export_incidents_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<IncidentsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let status = match query.status.as_deref() {
        Some(value) => match value.parse::<IncidentStatus>() {
            Ok(status) => Some(status),
            Err(_) => return api_error(StatusCode::BAD_REQUEST, "unknown incident status"),
        },
        None => None,
    };
    let mut incidents =
        match state.incidents_client.list_incidents(principal.tenant_id, status).await {
            Ok(incidents) => incidents,
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
    incidents.retain(|incident| incident_api_matches(incident, &query));
    let has_more = incidents.len() > 5_000;
    incidents.truncate(5_000);
    let mut csv = String::from(
        "id,title,summary,severity,status,assigned_to,event_count,created_at,updated_at,resolved_at\n",
    );
    for detail in incidents {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            detail.incident.id,
            api_csv_escape(&detail.incident.title),
            api_csv_escape(&detail.incident.summary),
            detail.incident.severity,
            detail.incident.status,
            api_csv_escape(detail.incident.assigned_to.as_deref().unwrap_or("")),
            detail.event_ids.len(),
            detail.incident.created_at.to_rfc3339(),
            detail.incident.updated_at.to_rfc3339(),
            detail.incident.resolved_at.map(|value| value.to_rfc3339()).unwrap_or_default(),
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"incidents-export-{}.csv\"", principal.tenant_id)
            .parse()
            .unwrap(),
    );
    if has_more {
        response_headers.insert("x-export-truncated", "true".parse().unwrap());
    }
    (response_headers, csv).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct GlobalSearchApiQuery {
    pub q: String,
    #[serde(default)]
    pub scope: String,
}

#[derive(Debug, Serialize)]
struct SearchAuditApiHit {
    service: &'static str,
    entry: crate::audit_log_client::AuditLogEntry,
}

#[derive(Debug, Serialize)]
struct GlobalSearchApiResponse {
    query: String,
    scope: String,
    records: Vec<crate::RecordSummary>,
    sensors: Vec<common::Sensor>,
    identities: Vec<crate::UiUser>,
    entities: Vec<common::ontology::Object>,
    incidents: Vec<IncidentDetail>,
    actions: Vec<common::ontology::ActionInvocation>,
    events: Vec<EventSummary>,
    audits: Vec<SearchAuditApiHit>,
    templates: Vec<common::ActionTemplate>,
    annotations: Vec<common::ontology::ObjectAnnotation>,
    object_types: Vec<common::ontology::ObjectType>,
    action_types: Vec<common::ontology::ActionType>,
    errors: Vec<String>,
}

fn global_search_scope(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "records" | "sensors" | "identities" | "entities" | "incidents" | "events" | "actions"
        | "audit" | "templates" | "annotations" | "object_types" | "action_types" => {
            value.trim().to_ascii_lowercase()
        }
        _ => "all".to_string(),
    }
}

fn search_scope_matches(scope: &str, category: &str) -> bool {
    scope == "all" || scope == category
}

pub fn search_object_type_matches(object_type: &common::ontology::ObjectType, term: &str) -> bool {
    format!(
        "{} {} {} {}",
        object_type.id, object_type.name, object_type.property_schema, object_type.mapping_rules
    )
    .to_ascii_lowercase()
    .contains(&term.trim().to_ascii_lowercase())
}

pub fn search_action_type_matches(
    action_type: &common::ontology::ActionType,
    target_type: Option<&common::ontology::ObjectType>,
    term: &str,
) -> bool {
    format!(
        "{} {} {} {} {} {} {}",
        action_type.id,
        action_type.name,
        action_type.parameter_schema,
        action_type.preconditions,
        action_type.effect_definition,
        action_type.target_object_type_id.map(|id| id.to_string()).unwrap_or_default(),
        target_type.map(|item| item.name.as_str()).unwrap_or_default(),
    )
    .to_ascii_lowercase()
    .contains(&term.trim().to_ascii_lowercase())
}

pub async fn global_search_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GlobalSearchApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let term = query.q.trim().to_string();
    let scope = global_search_scope(&query.scope);
    if term.is_empty() {
        return axum::Json(GlobalSearchApiResponse {
            query: term,
            scope,
            records: Vec::new(),
            sensors: Vec::new(),
            identities: Vec::new(),
            entities: Vec::new(),
            incidents: Vec::new(),
            actions: Vec::new(),
            events: Vec::new(),
            audits: Vec::new(),
            templates: Vec::new(),
            annotations: Vec::new(),
            object_types: Vec::new(),
            action_types: Vec::new(),
            errors: Vec::new(),
        })
        .into_response();
    }
    let term_lower = term.to_ascii_lowercase();
    let mut errors = Vec::new();
    let mut records = Vec::new();
    let mut sensors = Vec::new();
    let mut identities = Vec::new();
    let mut entities = Vec::new();
    let mut incidents = Vec::new();
    let mut actions = Vec::new();
    let mut events = Vec::new();
    let mut audits = Vec::new();
    let mut templates = Vec::new();
    let mut annotations = Vec::new();
    let mut object_types = Vec::new();
    let mut action_types = Vec::new();

    if search_scope_matches(&scope, "records") {
        let filter = crate::RecordSearchFilter {
            query: Some(term.clone()),
            limit: 24,
            ..Default::default()
        };
        match state.stats_client.search_records(principal.tenant_id, &filter).await {
            Ok(result) => records = result.records,
            Err(error) => errors.push(format!("records: {error}")),
        }
    }
    if search_scope_matches(&scope, "sensors") {
        match state.sensors_client.list_sensors(principal.tenant_id, 1000, 0).await {
            Ok(page) => {
                sensors = page
                    .sensors
                    .into_iter()
                    .filter(|sensor| {
                        sensor.name.to_ascii_lowercase().contains(&term_lower)
                            || sensor.connector_type.to_ascii_lowercase().contains(&term_lower)
                            || sensor.id.to_string().contains(&term_lower)
                    })
                    .take(24)
                    .collect();
            }
            Err(error) => errors.push(format!("sensors: {error}")),
        }
    }
    if search_scope_matches(&scope, "identities") {
        match state.users_client.list_users(principal.tenant_id, principal.role).await {
            Ok(users) => {
                identities = users
                    .into_iter()
                    .filter(|user| {
                        user.username.to_ascii_lowercase().contains(&term_lower)
                            || user.role.to_string().contains(&term_lower)
                            || user.id.to_string().contains(&term_lower)
                    })
                    .take(24)
                    .collect();
            }
            Err(error) => errors.push(format!("identities: {error}")),
        }
    }
    if search_scope_matches(&scope, "incidents") {
        match state.incidents_client.list_incidents(principal.tenant_id, None).await {
            Ok(items) => {
                incidents = items
                    .into_iter()
                    .filter(|item| {
                        format!(
                            "{} {} {}",
                            item.incident.title, item.incident.summary, item.incident.id
                        )
                        .to_ascii_lowercase()
                        .contains(&term_lower)
                    })
                    .take(24)
                    .collect();
            }
            Err(error) => errors.push(format!("incidents: {error}")),
        }
    }
    if search_scope_matches(&scope, "events") {
        match state.events_client.list_events(&principal.query_token, 1000, 0, None, None).await {
            Ok(page) => {
                events = page
                    .events
                    .into_iter()
                    .filter(|event| {
                        format!(
                            "{} {} {} {}",
                            event.id, event.event_type, event.group_key, event.status
                        )
                        .to_ascii_lowercase()
                        .contains(&term_lower)
                            || event
                                .record_ids
                                .iter()
                                .any(|id| id.to_string().contains(&term_lower))
                    })
                    .take(24)
                    .collect();
            }
            Err(error) => errors.push(format!("events: {error}")),
        }
    }
    if let Some(client) = crate::ontology_client::global() {
        if search_scope_matches(&scope, "annotations") {
            match client.list_all_object_annotations(&principal.query_token).await {
                Ok(items) => {
                    annotations = items
                        .into_iter()
                        .filter(|annotation| {
                            format!(
                                "{} {} {} {}",
                                annotation.id,
                                annotation.object_id,
                                annotation.author,
                                annotation.body
                            )
                            .to_ascii_lowercase()
                            .contains(&term_lower)
                        })
                        .take(24)
                        .collect();
                }
                Err(error) => errors.push(format!("annotations: {error}")),
            }
        }
        if search_scope_matches(&scope, "entities") {
            match client.list_objects(&principal.query_token, None).await {
                Ok(objects) => {
                    entities = objects
                        .into_iter()
                        .filter(|object| {
                            object.properties.to_string().to_ascii_lowercase().contains(&term_lower)
                                || object.id.to_string().contains(&term_lower)
                        })
                        .take(24)
                        .collect();
                }
                Err(error) => errors.push(format!("entities: {error}")),
            }
        }
        if search_scope_matches(&scope, "actions") {
            match client.list_action_invocations(&principal.query_token).await {
                Ok(invocations) => {
                    actions = invocations
                        .into_iter()
                        .filter(|action| {
                            format!(
                                "{} {} {} {}",
                                action.id, action.action_type_id, action.outcome, action.parameters
                            )
                            .to_ascii_lowercase()
                            .contains(&term_lower)
                        })
                        .take(24)
                        .collect();
                }
                Err(error) => errors.push(format!("actions: {error}")),
            }
        }
        if search_scope_matches(&scope, "object_types")
            || search_scope_matches(&scope, "action_types")
        {
            match client.list_object_types(&principal.query_token).await {
                Ok(items) => object_types = items,
                Err(error) => errors.push(format!("object types: {error}")),
            }
        }
        if search_scope_matches(&scope, "action_types") {
            match client.list_action_types(&principal.query_token).await {
                Ok(items) => {
                    action_types = items
                        .into_iter()
                        .filter(|action| {
                            let target = action.target_object_type_id.and_then(|id| {
                                object_types.iter().find(|object_type| object_type.id == id)
                            });
                            search_action_type_matches(action, target, &term)
                        })
                        .take(24)
                        .collect();
                }
                Err(error) => errors.push(format!("action types: {error}")),
            }
        }
        if search_scope_matches(&scope, "object_types") {
            object_types = object_types
                .into_iter()
                .filter(|object_type| search_object_type_matches(object_type, &term))
                .take(24)
                .collect();
        } else {
            // Object types are an internal lookup table when matching action targets; do not
            // leak them into an explicitly scoped action-type response.
            object_types.clear();
        }
    }
    if search_scope_matches(&scope, "audit") {
        let (entries, audit_errors) = crate::recent_audit_log_handler::fetch_merged_page(
            &state,
            principal.tenant_id,
            &principal.query_token,
            None,
            80,
        )
        .await;
        errors.extend(audit_errors.into_iter().map(|error| format!("audit: {error}")));
        audits = entries
            .into_iter()
            .filter(|(_, entry)| {
                format!(
                    "{} {} {} {} {} {}",
                    entry.id,
                    entry.entity_id,
                    entry.entity_type,
                    entry.change_type,
                    entry.actor,
                    entry.after
                )
                .to_ascii_lowercase()
                .contains(&term_lower)
            })
            .take(24)
            .map(|(service, entry)| SearchAuditApiHit { service, entry })
            .collect();
    }
    if search_scope_matches(&scope, "templates") {
        match crate::action_templates_client::global() {
            Some(client) => match client.list(principal.tenant_id).await {
                Ok(items) => {
                    templates = items
                        .into_iter()
                        .filter(|template| {
                            format!(
                                "{} {} {} {} {}",
                                template.id,
                                template.name,
                                template.description,
                                serde_json::to_value(template.action_type).unwrap_or_default(),
                                template.config
                            )
                            .to_ascii_lowercase()
                            .contains(&term_lower)
                        })
                        .take(24)
                        .collect();
                }
                Err(error) => errors.push(format!("templates: {error}")),
            },
            None => errors.push("templates: client unavailable".to_string()),
        }
    }
    axum::Json(GlobalSearchApiResponse {
        query: term,
        scope,
        records,
        sensors,
        identities,
        entities,
        incidents,
        actions,
        events,
        audits,
        templates,
        annotations,
        object_types,
        action_types,
        errors,
    })
    .into_response()
}

pub async fn get_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.incidents_client.get_incident(principal.tenant_id, id).await {
        Ok(Some(incident)) => axum::Json(incident).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "incident not found"),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// POST /api/v1/incidents/:id/brief — regenerates the same bounded, evidence-backed incident
/// brief exposed by the browser Console. The incident is re-read under the caller's tenant,
/// linked events are fetched through the query boundary, and the resulting summary is persisted
/// through the normal audited incident update path.
pub async fn generate_incident_brief_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let detail = match incident_for_api(&state, principal.tenant_id, id).await {
        Ok(detail) => detail,
        Err(error) => return error,
    };
    let mut events = Vec::new();
    for event_id in detail.event_ids.iter().take(100) {
        match state.events_client.get_event(&principal.query_token, *event_id).await {
            Ok(Some(event)) => events.push(crate::incident_handlers::LinkedEventRow {
                event,
                correlation: "Manual link".to_string(),
            }),
            Ok(None) => {}
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        }
    }
    let mut incident = detail.incident;
    let deterministic_brief = crate::incident_handlers::evidence_brief(&incident, &events);
    let evidence = crate::incident_handlers::evidence_brief_payload(&incident, &events);
    incident.summary = match crate::incident_brief_client::global() {
        Some(client) => {
            client.generate(principal.tenant_id, evidence).await.unwrap_or(deterministic_brief)
        }
        None => deterministic_brief,
    };
    incident.updated_at = Utc::now();
    match state
        .incidents_client
        .update_incident(principal.role, &principal.username, incident)
        .await
    {
        Ok(updated) => axum::Json(updated).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
pub struct IncidentTimelineApiEntry {
    pub kind: String,
    pub actor: String,
    pub at: chrono::DateTime<chrono::Utc>,
    pub summary: String,
    pub detail: String,
    pub href: Option<String>,
    pub failure: bool,
}

/// Builds the bounded chronology shared by the browser incident page and external 360 clients.
/// Inputs are already tenant-scoped by their owning clients; this function only composes and
/// orders them into one operator-facing investigation stream.
pub fn build_incident_timeline_api(
    incident: &IncidentDetail,
    events: &[crate::events_client::EventDetail],
    audit_log: &[crate::audit_log_client::AuditLogEntry],
    invocations: &[common::ontology::ActionInvocation],
) -> Vec<IncidentTimelineApiEntry> {
    let mut timeline = vec![IncidentTimelineApiEntry {
        kind: "Case opened".to_string(),
        actor: incident.incident.assigned_to.clone().unwrap_or_else(|| "system".to_string()),
        at: incident.incident.created_at,
        summary: incident.incident.title.clone(),
        detail: format!("{} severity · {}", incident.incident.severity, incident.incident.status),
        href: None,
        failure: false,
    }];
    timeline.extend(events.iter().map(|event| IncidentTimelineApiEntry {
        kind: "Signal linked".to_string(),
        actor: "signal pipeline".to_string(),
        at: event.occurred_at,
        summary: event.event_type.clone(),
        detail: format!("{} · {}", event.group_key, event.status),
        href: Some(format!("/events/{}", event.id)),
        failure: event.status.eq_ignore_ascii_case("dismissed"),
    }));
    timeline.extend(audit_log.iter().map(|entry| IncidentTimelineApiEntry {
        kind: entry.change_type.clone(),
        actor: entry.actor.clone(),
        at: entry.changed_at,
        summary: format!("{} {}", entry.entity_type, entry.change_type),
        detail: format!("Entity {}", entry.entity_id),
        href: None,
        failure: false,
    }));
    timeline.extend(incident.notes.iter().map(|note| IncidentTimelineApiEntry {
        kind: "Investigation note".to_string(),
        actor: note.author.clone(),
        at: note.created_at,
        summary: note.body.clone(),
        detail: "Operator finding".to_string(),
        href: None,
        failure: false,
    }));
    timeline.extend(invocations.iter().map(|invocation| IncidentTimelineApiEntry {
        kind: "Governed response".to_string(),
        actor: "operator".to_string(),
        at: invocation.executed_at,
        summary: format!("Action {}", invocation.action_type_id),
        detail: invocation.outcome.clone(),
        href: Some(format!("/actions/{}", invocation.id)),
        failure: !invocation.outcome.eq_ignore_ascii_case("completed"),
    }));
    timeline.sort_by_key(|entry| std::cmp::Reverse(entry.at));
    timeline.truncate(100);
    timeline
}

#[derive(Debug, Serialize)]
struct Incident360ApiResponse {
    incident: IncidentDetail,
    events: Vec<crate::events_client::EventDetail>,
    records: Vec<crate::RecordSummary>,
    executions: Vec<crate::execution_client::ActionExecutionSummary>,
    audit_log: Vec<crate::audit_log_client::AuditLogEntry>,
    modeled_objects: Vec<common::ontology::Object>,
    object_types: Vec<common::ontology::ObjectType>,
    links: Vec<common::ontology::Link>,
    link_types: Vec<common::ontology::LinkType>,
    action_types: Vec<common::ontology::ActionType>,
    invocations: Vec<common::ontology::ActionInvocation>,
    reviews: Vec<common::ontology::ActionReview>,
    timeline: Vec<IncidentTimelineApiEntry>,
    errors: Vec<String>,
}

fn invocation_targets_any_api(
    invocation: &common::ontology::ActionInvocation,
    object_ids: &std::collections::HashSet<Uuid>,
) -> bool {
    invocation.target_object_ids.as_array().is_some_and(|targets| {
        targets.iter().any(|target| {
            target
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .is_some_and(|id| object_ids.contains(&id))
        })
    })
}

/// Returns the bounded case-centric investigation context used by the Console incident page.
/// This keeps evidence, model impact, governed response, and immutable case activity in one
/// tenant-scoped read for external operator shells.
pub async fn get_incident_360(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let incident = match state.incidents_client.get_incident(principal.tenant_id, id).await {
        Ok(Some(incident)) => incident,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, "incident not found"),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut errors = Vec::new();
    let mut events = Vec::new();
    for event_id in incident.event_ids.iter().take(100) {
        match state.events_client.get_event(&principal.query_token, *event_id).await {
            Ok(Some(event)) => events.push(event),
            Ok(None) => errors.push(format!("event {event_id}: not found")),
            Err(error) => errors.push(format!("event {event_id}: {error}")),
        }
    }
    let mut record_ids = std::collections::HashSet::new();
    for event in &events {
        record_ids.extend(event.record_ids.iter().copied());
    }
    let mut records = Vec::new();
    for record_id in record_ids.into_iter().take(100) {
        match state.stats_client.get_record(principal.tenant_id, record_id).await {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(error) => errors.push(format!("record {record_id}: {error}")),
        }
    }
    let mut executions = Vec::new();
    for event in events.iter().take(100) {
        match state.execution_client.list_executions_for_event(principal.tenant_id, event.id).await
        {
            Ok(items) => executions.extend(items.into_iter().take(100 - executions.len())),
            Err(error) => errors.push(format!("executions for {}: {error}", event.id)),
        }
        if executions.len() >= 100 {
            break;
        }
    }
    let audit_log =
        match state.incidents_client.list_audit_log_for_entity(principal.tenant_id, id).await {
            Ok(entries) => entries.into_iter().take(100).collect(),
            Err(error) => {
                errors.push(format!("audit log: {error}"));
                Vec::new()
            }
        };
    let (modeled_objects, object_types, links, link_types, action_types, invocations, reviews) =
        if let Some(client) = crate::ontology_client::global() {
            let (types, objects, links, link_types, actions, invocations, reviews) = tokio::join!(
                client.list_object_types(&principal.query_token),
                client.list_objects(&principal.query_token, None),
                client.list_links(&principal.query_token),
                client.list_link_types(&principal.query_token),
                client.list_action_types(&principal.query_token),
                client.list_action_invocations(&principal.query_token),
                client.list_action_reviews(&principal.query_token),
            );
            let object_types = match types {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("object types: {error}"));
                    Vec::new()
                }
            };
            let modeled_objects = match objects {
                Ok(objects) => events
                    .iter()
                    .flat_map(|event| {
                        objects.iter().filter(move |object| {
                            ontology_object_matches_event_api(
                                object,
                                &event.entity_ref,
                                &event.record_ids,
                            )
                        })
                    })
                    .map(|object| (object.id, object.clone()))
                    .collect::<std::collections::HashMap<_, _>>()
                    .into_values()
                    .take(100)
                    .collect::<Vec<_>>(),
                Err(error) => {
                    errors.push(format!("modeled objects: {error}"));
                    Vec::new()
                }
            };
            let modeled_ids = modeled_objects
                .iter()
                .map(|object| object.id)
                .collect::<std::collections::HashSet<_>>();
            let links = match links {
                Ok(links) => links
                    .into_iter()
                    .filter(|link| {
                        modeled_ids.contains(&link.source_object_id)
                            || modeled_ids.contains(&link.target_object_id)
                    })
                    .take(100)
                    .collect(),
                Err(error) => {
                    errors.push(format!("relationships: {error}"));
                    Vec::new()
                }
            };
            let link_types = match link_types {
                Ok(types) => types
                    .into_iter()
                    .filter(|link_type| links.iter().any(|link| link.link_type_id == link_type.id))
                    .collect(),
                Err(error) => {
                    errors.push(format!("relationship types: {error}"));
                    Vec::new()
                }
            };
            let action_types = match actions {
                Ok(actions) => actions,
                Err(error) => {
                    errors.push(format!("action types: {error}"));
                    Vec::new()
                }
            };
            let invocations = match invocations {
                Ok(invocations) => invocations
                    .into_iter()
                    .filter(|invocation| {
                        invocation
                            .triggering_event_ref
                            .get("incident_id")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| Uuid::parse_str(value).ok())
                            == Some(id)
                            || invocation_targets_any_api(invocation, &modeled_ids)
                    })
                    .take(100)
                    .collect::<Vec<_>>(),
                Err(error) => {
                    errors.push(format!("action invocations: {error}"));
                    Vec::new()
                }
            };
            let reviews = match reviews {
                Ok(reviews) => reviews
                    .into_iter()
                    .filter(|review| invocations.iter().any(|item| item.id == review.invocation_id))
                    .take(100)
                    .collect(),
                Err(error) => {
                    errors.push(format!("action reviews: {error}"));
                    Vec::new()
                }
            };
            (modeled_objects, object_types, links, link_types, action_types, invocations, reviews)
        } else {
            errors.push("ontology client unavailable".to_string());
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
        };
    let timeline = build_incident_timeline_api(&incident, &events, &audit_log, &invocations);
    axum::Json(Incident360ApiResponse {
        incident,
        events,
        records,
        executions,
        audit_log,
        modeled_objects,
        object_types,
        links,
        link_types,
        action_types,
        invocations,
        reviews,
        timeline,
        errors,
    })
    .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct SavedViewsApiQuery {
    pub surface: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSavedViewApiRequest {
    pub name: String,
    pub surface: String,
    pub filter: serde_json::Value,
}

fn normalize_saved_view_surface(value: &str) -> Result<String, Response> {
    let surface = value.trim().to_ascii_lowercase();
    if matches!(
        surface.as_str(),
        "data" | "events" | "incidents" | "actions" | "work" | "ontology" | "search" | "reports"
    ) {
        Ok(surface)
    } else {
        Err(api_error(
            StatusCode::BAD_REQUEST,
            "surface must be data, events, incidents, actions, work, ontology, search, or reports",
        ))
    }
}

fn saved_view_surface(query: &SavedSearchQuery) -> Option<&str> {
    query.filter.get("surface").and_then(serde_json::Value::as_str)
}

pub async fn list_saved_views_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SavedViewsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let surface = match query.surface.as_deref() {
        Some(value) if !value.trim().is_empty() => match normalize_saved_view_surface(value) {
            Ok(value) => Some(value),
            Err(error) => return error,
        },
        _ => None,
    };
    let text = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    match state.saved_search_queries_client.list(principal.tenant_id).await {
        Ok(views) => axum::Json(
            views
                .into_iter()
                .filter(|view| {
                    surface.as_deref().is_none_or(|value| saved_view_surface(view) == Some(value))
                        && (text.is_empty()
                            || view.name.to_ascii_lowercase().contains(&text)
                            || view.filter.to_string().to_ascii_lowercase().contains(&text))
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn create_saved_view_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateSavedViewApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let name = request.name.trim();
    if name.is_empty() || name.len() > 160 {
        return api_error(StatusCode::BAD_REQUEST, "name must be between 1 and 160 characters");
    }
    let surface = match normalize_saved_view_surface(&request.surface) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !request.filter.is_object() {
        return api_error(StatusCode::BAD_REQUEST, "filter must be a JSON object");
    }
    let mut filter = request.filter;
    filter["surface"] = serde_json::Value::String(surface);
    match state.saved_search_queries_client.create(principal.tenant_id, name, filter).await {
        Ok(view) => (StatusCode::CREATED, axum::Json(view)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_saved_view_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let exists = match state.saved_search_queries_client.list(principal.tenant_id).await {
        Ok(views) => views.into_iter().any(|view| view.id == id),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    if !exists {
        return api_error(StatusCode::NOT_FOUND, "saved view not found");
    }
    match state.saved_search_queries_client.delete(principal.tenant_id, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateSavedViewApiRequest {
    pub name: String,
    pub surface: String,
    pub filter: serde_json::Value,
}

pub async fn update_saved_view_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<UpdateSavedViewApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let name = request.name.trim();
    if name.is_empty() || name.len() > 160 {
        return api_error(StatusCode::BAD_REQUEST, "name must be between 1 and 160 characters");
    }
    let surface = match normalize_saved_view_surface(&request.surface) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !request.filter.is_object() {
        return api_error(StatusCode::BAD_REQUEST, "filter must be a JSON object");
    }
    let exists = match state.saved_search_queries_client.list(principal.tenant_id).await {
        Ok(views) => views.into_iter().any(|view| view.id == id),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    if !exists {
        return api_error(StatusCode::NOT_FOUND, "saved view not found");
    }
    let mut filter = request.filter;
    filter["surface"] = serde_json::Value::String(surface);
    let query =
        SavedSearchQuery { id, tenant_id: principal.tenant_id, name: name.to_string(), filter };
    match state.saved_search_queries_client.update(principal.tenant_id, query).await {
        Ok(view) => axum::Json(view).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateIncidentApiRequest {
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub severity: IncidentSeverity,
    #[serde(default)]
    pub event_ids: Vec<Uuid>,
}

pub async fn create_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateIncidentApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let title = request.title.trim();
    if title.is_empty() || title.len() > 240 {
        return api_error(StatusCode::BAD_REQUEST, "title must be between 1 and 240 characters");
    }
    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: principal.tenant_id,
        title: title.to_string(),
        summary: request.summary,
        severity: request.severity,
        status: IncidentStatus::Open,
        assigned_to: None,
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };
    match state
        .incidents_client
        .create_incident(principal.role, &principal.username, incident, request.event_ids)
        .await
    {
        Ok(created) => (StatusCode::CREATED, axum::Json(created)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct EventStatusApiRequest {
    pub status: String,
}

pub async fn update_event_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<EventStatusApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.status.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "status is required");
    }
    match state
        .events_client
        .update_event_status(&principal.query_token, id, request.status.trim(), &principal.username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkEventStatusApiRequest {
    pub ids: Vec<Uuid>,
    pub status: String,
}

fn valid_event_status(status: &str) -> bool {
    matches!(status, "new" | "triggered" | "actioned" | "dismissed")
}

pub async fn bulk_update_event_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkEventStatusApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let status = request.status.trim().to_ascii_lowercase();
    if !valid_event_status(&status) {
        return api_error(
            StatusCode::BAD_REQUEST,
            "status must be new, triggered, actioned, or dismissed",
        );
    }
    if request.ids.is_empty() || request.ids.len() > 500 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 500 events");
    }
    let mut updated = 0usize;
    let mut failed = 0usize;
    for id in request.ids {
        match state
            .events_client
            .update_event_status(&principal.query_token, id, &status, &principal.username)
            .await
        {
            Ok(()) => updated += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(serde_json::json!({ "updated": updated, "failed": failed, "status": status }))
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct LinkEventsApiRequest {
    pub event_ids: Vec<Uuid>,
}

pub async fn link_events_to_incident_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(incident_id): Path<Uuid>,
    axum::Json(request): axum::Json<LinkEventsApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if request.event_ids.is_empty() || request.event_ids.len() > 500 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "event_ids must contain between 1 and 500 events",
        );
    }
    if let Err(error) = incident_for_api(&state, principal.tenant_id, incident_id).await {
        return error;
    }
    let mut linked = 0usize;
    let mut failed = 0usize;
    for event_id in request.event_ids {
        match state
            .incidents_client
            .link_event(
                principal.role,
                &principal.username,
                principal.tenant_id,
                incident_id,
                event_id,
            )
            .await
        {
            Ok(()) => linked += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(
        serde_json::json!({ "linked": linked, "failed": failed, "incident_id": incident_id }),
    )
    .into_response()
}

pub async fn link_incident_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((incident_id, event_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .incidents_client
        .link_event(principal.role, &principal.username, principal.tenant_id, incident_id, event_id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// Removes one event-to-incident evidence link through the same tenant and operator boundary as
/// the browser's incident detail action. Unlinking changes only the case relationship; the event
/// and its source lineage remain immutable.
pub async fn unlink_incident_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((incident_id, event_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if let Err(error) = incident_for_api(&state, principal.tenant_id, incident_id).await {
        return error;
    }
    match state
        .incidents_client
        .unlink_event(
            principal.role,
            &principal.username,
            principal.tenant_id,
            incident_id,
            event_id,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct TriggersApiQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub q: Option<String>,
    pub enabled: Option<bool>,
}

fn trigger_api_matches(
    trigger: &crate::triggers_client::TriggerSummary,
    query: &TriggersApiQuery,
) -> bool {
    let q = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    (q.is_empty()
        || trigger.name.to_ascii_lowercase().contains(&q)
        || trigger.event_type_match.to_ascii_lowercase().contains(&q)
        || trigger.id.to_string().contains(&q))
        && query.enabled.is_none_or(|enabled| trigger.enabled == enabled)
}

#[derive(Debug, Serialize)]
struct TriggersApiResponse {
    triggers: Vec<crate::triggers_client::TriggerSummary>,
    has_more: bool,
    limit: i64,
    offset: i64,
}

pub async fn list_triggers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TriggersApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let offset = query.offset.unwrap_or(0).max(0);
    match state.triggers_client.list_triggers(principal.tenant_id, limit, offset).await {
        Ok(page) => {
            let triggers = page
                .triggers
                .into_iter()
                .filter(|trigger| trigger_api_matches(trigger, &query))
                .collect();
            axum::Json(TriggersApiResponse { triggers, has_more: page.has_more, limit, offset })
                .into_response()
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.triggers_client.get_trigger(principal.tenant_id, id).await {
        Ok(Some(trigger)) => axum::Json(trigger).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "trigger not found"),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn create_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(mut trigger): axum::Json<TriggerDefinition>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    trigger.tenant_id = principal.tenant_id;
    match state.triggers_client.create_trigger(principal.role, &principal.username, trigger).await {
        Ok(trigger) => (StatusCode::CREATED, axum::Json(trigger)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(mut trigger): axum::Json<TriggerDefinition>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    trigger.id = id;
    trigger.tenant_id = principal.tenant_id;
    match state.triggers_client.update_trigger(principal.role, &principal.username, trigger).await {
        Ok(trigger) => axum::Json(trigger).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkToggleTriggersApiRequest {
    pub ids: Vec<Uuid>,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct BulkToggleTriggersApiResponse {
    pub updated: Vec<Uuid>,
    pub failed: Vec<Uuid>,
    pub enabled: bool,
}

pub async fn bulk_toggle_triggers(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkToggleTriggersApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 triggers");
    }
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        let trigger = match state.triggers_client.get_trigger(principal.tenant_id, id).await {
            Ok(Some(trigger)) => trigger,
            _ => {
                failed.push(id);
                continue;
            }
        };
        let trigger = common::TriggerDefinition { enabled: request.enabled, ..trigger };
        match state
            .triggers_client
            .update_trigger(principal.role, &principal.username, trigger)
            .await
        {
            Ok(_) => updated.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(BulkToggleTriggersApiResponse { updated, failed, enabled: request.enabled })
        .into_response()
}

pub async fn delete_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .triggers_client
        .delete_trigger(principal.role, &principal.username, principal.tenant_id, id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct TriggerTestApiRequest {
    pub group_key: String,
}

pub async fn test_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<TriggerTestApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if request.group_key.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "group_key is required");
    }
    match state
        .triggers_client
        .test_trigger(principal.tenant_id, id, request.group_key.trim())
        .await
    {
        Ok(result) => axum::Json(result).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct EventTypesApiQuery {
    pub all_versions: Option<bool>,
    pub q: Option<String>,
    pub coverage: Option<String>,
}

fn normalize_event_coverage_api(value: Option<&String>) -> Result<String, Response> {
    let raw = value.map(String::as_str).unwrap_or("");
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Ok(String::new()),
        "governed" | "observed_only" | "triggerless" => Ok(raw.trim().to_ascii_lowercase()),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "coverage must be governed, observed_only, or triggerless",
        )),
    }
}

pub async fn list_event_types(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventTypesApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let coverage = match normalize_event_coverage_api(query.coverage.as_ref()) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let definitions = match state
        .saved_search_queries_client
        .list_event_types(principal.tenant_id, query.all_versions.unwrap_or(false))
        .await
    {
        Ok(definitions) => definitions,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let q = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let mut definitions = definitions
        .into_iter()
        .filter(|definition| {
            q.is_empty()
                || definition.name.to_ascii_lowercase().contains(&q)
                || definition.field_schema.to_string().to_ascii_lowercase().contains(&q)
        })
        .collect::<Vec<_>>();
    if !coverage.is_empty() {
        let triggers = match state.triggers_client.list_triggers(principal.tenant_id, 1000, 0).await
        {
            Ok(page) => page.triggers,
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
        let governed = triggers
            .iter()
            .map(|trigger| trigger.event_type_match.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>();
        definitions.retain(|definition| {
            let has_trigger = governed.contains(&definition.name.to_ascii_lowercase());
            match coverage.as_str() {
                "governed" => has_trigger,
                "observed_only" => !has_trigger,
                "triggerless" => !has_trigger,
                _ => true,
            }
        });
    }
    axum::Json(definitions).into_response()
}

pub async fn get_event_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.saved_search_queries_client.list_event_types(principal.tenant_id, true).await {
        Ok(definitions) => match definitions.into_iter().find(|definition| definition.id == id) {
            Some(definition) => axum::Json(definition).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "event type not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateEventTypeApiRequest {
    pub name: String,
    pub field_schema: serde_json::Value,
}

pub async fn create_event_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateEventTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let name = request.name.trim();
    if name.is_empty() || name.len() > 160 {
        return api_error(StatusCode::BAD_REQUEST, "name must be between 1 and 160 characters");
    }
    if !request.field_schema.is_object() {
        return api_error(StatusCode::BAD_REQUEST, "field_schema must be a JSON object");
    }
    let definition =
        common::EventTypeDefinition::new(principal.tenant_id, name, request.field_schema);
    match state
        .saved_search_queries_client
        .create_event_type(principal.role, &principal.username, definition)
        .await
    {
        Ok(definition) => (StatusCode::CREATED, axum::Json(definition)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct EventTypeVersionApiRequest {
    pub field_schema: serde_json::Value,
}

pub async fn create_event_type_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<EventTypeVersionApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if !request.field_schema.is_object() {
        return api_error(StatusCode::BAD_REQUEST, "field_schema must be a JSON object");
    }
    match state
        .saved_search_queries_client
        .create_event_type_version(
            principal.role,
            &principal.username,
            principal.tenant_id,
            id,
            request.field_schema,
        )
        .await
    {
        Ok(definition) => (StatusCode::CREATED, axum::Json(definition)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn parse_data_class(value: &str) -> Option<crate::retention_policies_client::DataClass> {
    match value.trim().to_ascii_lowercase().as_str() {
        "raw" => Some(crate::retention_policies_client::DataClass::Raw),
        "normalized" => Some(crate::retention_policies_client::DataClass::Normalized),
        "event" => Some(crate::retention_policies_client::DataClass::Event),
        _ => None,
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct RetentionPoliciesApiQuery {
    pub data_class: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn list_retention_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RetentionPoliciesApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let data_class = match query.data_class.as_deref() {
        Some(value) if !value.trim().is_empty() => match parse_data_class(value) {
            Some(value) => Some(value),
            None => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "data_class must be raw, normalized, or event",
                )
            }
        },
        _ => None,
    };
    match state.retention_policies_client.list_policies(principal.tenant_id).await {
        Ok(policies) => axum::Json(
            policies
                .into_iter()
                .filter(|policy| data_class.is_none_or(|value| policy.data_class == value))
                .filter(|policy| query.enabled.is_none_or(|value| policy.enabled == value))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct RetentionPolicyApiRequest {
    pub data_class: String,
    pub ttl_days: i32,
    #[serde(default = "default_enabled_api")]
    pub enabled: bool,
}

fn default_enabled_api() -> bool {
    true
}

fn policy_from_request(
    request: RetentionPolicyApiRequest,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<crate::retention_policies_client::RetentionPolicy, Response> {
    let Some(data_class) = parse_data_class(&request.data_class) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "data_class must be raw, normalized, or event",
        ));
    };
    if !(1..=36500).contains(&request.ttl_days) {
        return Err(api_error(StatusCode::BAD_REQUEST, "ttl_days must be between 1 and 36500"));
    }
    Ok(crate::retention_policies_client::RetentionPolicy {
        id,
        tenant_id,
        data_class,
        ttl_days: request.ttl_days,
        enabled: request.enabled,
    })
}

pub async fn create_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<RetentionPolicyApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let policy = match policy_from_request(request, principal.tenant_id, Uuid::new_v4()) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state
        .retention_policies_client
        .create_policy(principal.role, policy, &principal.username)
        .await
    {
        Ok(policy) => (StatusCode::CREATED, axum::Json(policy)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<RetentionPolicyApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let policy = match policy_from_request(request, principal.tenant_id, id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state
        .retention_policies_client
        .update_policy(principal.role, policy, &principal.username)
        .await
    {
        Ok(policy) => axum::Json(policy).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.retention_policies_client.list_policies(principal.tenant_id).await {
        Ok(policies) => match policies.into_iter().find(|policy| policy.id == id) {
            Some(policy) => axum::Json(policy).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "retention policy not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .retention_policies_client
        .delete_policy(principal.role, principal.tenant_id, id, &principal.username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkDeleteRetentionPoliciesApiRequest {
    pub ids: Vec<Uuid>,
}

/// POST /api/v1/retention-policies/bulk-delete — applies the existing audited, tenant-scoped
/// single-policy deletion contract to a bounded set, matching the Console bulk action without
/// introducing a second retention-service mutation path.
pub async fn bulk_delete_retention_policies_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkDeleteRetentionPoliciesApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 policies");
    }
    let mut deleted = 0usize;
    let mut failed = 0usize;
    for id in request.ids {
        match state
            .retention_policies_client
            .delete_policy(principal.role, principal.tenant_id, id, &principal.username)
            .await
        {
            Ok(()) => deleted += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(serde_json::json!({ "deleted": deleted, "failed": failed })).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct ComplianceHoldsApiQuery {
    pub data_class: Option<String>,
    pub active: Option<bool>,
}

pub async fn list_compliance_holds(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ComplianceHoldsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let data_class = match query.data_class.as_deref() {
        Some(value) if !value.trim().is_empty() => match parse_data_class(value) {
            Some(value) => Some(value),
            None => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "data_class must be raw, normalized, or event",
                )
            }
        },
        _ => None,
    };
    match state.retention_policies_client.list_holds(principal.tenant_id).await {
        Ok(holds) => axum::Json(
            holds
                .into_iter()
                .filter(|hold| data_class.is_none_or(|value| hold.data_class == value))
                .filter(|hold| query.active.is_none_or(|value| hold.active == value))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ComplianceHoldApiRequest {
    pub data_class: String,
    pub reason: String,
}

pub async fn create_compliance_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ComplianceHoldApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let Some(data_class) = parse_data_class(&request.data_class) else {
        return api_error(StatusCode::BAD_REQUEST, "data_class must be raw, normalized, or event");
    };
    let reason = request.reason.trim();
    if reason.is_empty() || reason.len() > 1000 {
        return api_error(StatusCode::BAD_REQUEST, "reason must be between 1 and 1000 characters");
    }
    match state
        .retention_policies_client
        .create_hold(principal.role, principal.tenant_id, data_class, reason, &principal.username)
        .await
    {
        Ok(hold) => (StatusCode::CREATED, axum::Json(hold)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_compliance_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.retention_policies_client.list_holds(principal.tenant_id).await {
        Ok(holds) => match holds.into_iter().find(|hold| hold.id == id) {
            Some(hold) => axum::Json(hold).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "compliance hold not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn release_compliance_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .retention_policies_client
        .release_hold(principal.role, principal.tenant_id, id, &principal.username)
        .await
    {
        Ok(hold) => axum::Json(hold).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ReimportApiRequest {
    pub archive_key: String,
}

pub async fn reimport_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ReimportApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.archive_key.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "archive_key is required");
    }
    match state
        .retention_policies_client
        .reimport_archive(principal.tenant_id, request.archive_key.trim())
        .await
    {
        Ok(summary) => axum::Json(summary).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct ReportScheduleApi {
    id: Uuid,
    name: String,
    frequency: String,
    recipient: String,
    from: String,
    to: String,
    format: String,
    enabled: bool,
}

fn schedule_from_query(query: SavedSearchQuery) -> Option<ReportScheduleApi> {
    let filter = query.filter.as_object()?;
    if filter.get("view_kind")?.as_str()? != "report_schedule" {
        return None;
    }
    let frequency = filter.get("frequency")?.as_str()?.to_string();
    if !matches!(frequency.as_str(), "daily" | "weekly" | "monthly") {
        return None;
    }
    Some(ReportScheduleApi {
        id: query.id,
        name: query.name,
        frequency,
        recipient: filter.get("recipient")?.as_str()?.to_string(),
        from: filter
            .get("from")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        to: filter.get("to").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
        format: match filter.get("format").and_then(serde_json::Value::as_str) {
            Some("pdf") => "pdf".to_string(),
            _ => "csv".to_string(),
        },
        enabled: filter.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(true),
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct ReportSchedulesApiQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub enabled: Option<bool>,
    pub format: Option<String>,
}

fn normalize_report_run_status_api(value: Option<&String>) -> Result<String, Response> {
    let status = value.map(String::as_str).unwrap_or("").trim().to_ascii_lowercase();
    match status.as_str() {
        "" => Ok(String::new()),
        "success" | "failed" | "running" => Ok(status),
        _ => Err(api_error(StatusCode::BAD_REQUEST, "status must be success, failed, or running")),
    }
}

fn report_run_status_api(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "generated" | "delivered" | "success" => "success",
        "failed" | "delivery_failed" => "failed",
        "running" | "in_progress" => "running",
        _ => "running",
    }
}

pub async fn list_report_schedules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportSchedulesApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let status = match normalize_report_run_status_api(query.status.as_ref()) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let text = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let format = query.format.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    if !format.is_empty() && !matches!(format.as_str(), "csv" | "pdf") {
        return api_error(StatusCode::BAD_REQUEST, "format must be csv or pdf");
    }
    let schedules = state
        .saved_search_queries_client
        .list(principal.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(schedule_from_query)
        .filter(|schedule| {
            (text.is_empty()
                || schedule.name.to_ascii_lowercase().contains(&text)
                || schedule.recipient.to_ascii_lowercase().contains(&text))
                && query.enabled.is_none_or(|enabled| schedule.enabled == enabled)
                && (format.is_empty() || schedule.format == format)
        })
        .collect::<Vec<_>>();
    let runs = state
        .saved_search_queries_client
        .list_report_runs(principal.tenant_id, None)
        .await
        .unwrap_or_default();
    let runs = runs
        .into_iter()
        .filter(|run| {
            (status.is_empty() || report_run_status_api(&run.status) == status)
                && (text.is_empty()
                    || run.schedule_name.to_ascii_lowercase().contains(&text)
                    || run.recipient.to_ascii_lowercase().contains(&text))
        })
        .collect::<Vec<_>>();
    axum::Json(serde_json::json!({"schedules": schedules, "runs": runs})).into_response()
}

#[derive(Debug, Serialize)]
struct ReportsSummaryApiResponse {
    window: ReportsSummaryWindow,
    connectors: Vec<ReportsConnectorSummary>,
    event_counts: Vec<crate::reports_handler::EventTypeCount>,
    signal_count: usize,
    record_count: usize,
    incidents: ReportsIncidentSummary,
    ontology: ReportsOntologySummary,
    actions: ReportsActionSummary,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReportsSummaryWindow {
    from: String,
    to: String,
}

#[derive(Debug, Serialize)]
struct ReportsConnectorSummary {
    connector_id: String,
    record_count: i64,
    last_ingested_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize)]
struct ReportsIncidentSummary {
    open: usize,
    acknowledged: usize,
    resolved: usize,
    sla_breached: usize,
}

#[derive(Debug, Default, Serialize)]
struct ReportsOntologySummary {
    object_types: usize,
    objects: usize,
}

#[derive(Debug, Default, Serialize)]
struct ReportsActionSummary {
    completed: usize,
    needs_review: usize,
}

/// Returns the platform-wide report aggregate without requiring API consumers to scrape the
/// server-rendered Reports page. The response deliberately includes an `errors` collection: a
/// degraded source is visible to an integration instead of being mistaken for a healthy zero.
pub async fn get_reports_summary_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<crate::reports_handler::ReportsQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (since, until) = crate::reports_handler::parse_date_range(&query.from, &query.to);
    if (!query.from.is_empty() && since.is_none()) || (!query.to.is_empty() && until.is_none()) {
        return api_error(StatusCode::BAD_REQUEST, "from and to must be YYYY-MM-DD dates");
    }
    if let (Some(from), Some(to)) = (since, until) {
        if from > to {
            return api_error(StatusCode::BAD_REQUEST, "from must not be after to");
        }
    }

    let mut errors = Vec::new();
    let (connector_stats, _) = match crate::reports_handler::connector_stats_for_window(
        &state,
        principal.tenant_id,
        since,
        until,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("records: {error}"));
            (Vec::new(), std::collections::HashSet::new())
        }
    };
    let connectors = connector_stats
        .into_iter()
        .map(|stat| ReportsConnectorSummary {
            connector_id: stat.connector_id,
            record_count: stat.record_count,
            last_ingested_at: stat.last_ingested_at,
        })
        .collect::<Vec<_>>();
    let record_count = connectors.iter().map(|stat| stat.record_count.max(0) as usize).sum();

    let events = match state
        .events_client
        .list_events(&principal.query_token, 1000, 0, since, until)
        .await
    {
        Ok(page) => page.events,
        Err(error) => {
            errors.push(format!("events: {error}"));
            Vec::new()
        }
    };
    let event_counts = crate::reports_handler::count_by_event_type(&events);

    let mut incidents_summary = ReportsIncidentSummary::default();
    match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(incidents) => {
            for detail in incidents.into_iter().filter(|detail| {
                let created = detail.incident.created_at;
                since.map(|value| created >= value).unwrap_or(true)
                    && until.map(|value| created <= value).unwrap_or(true)
            }) {
                match detail.incident.status {
                    common::IncidentStatus::Open => incidents_summary.open += 1,
                    common::IncidentStatus::Acknowledged => incidents_summary.acknowledged += 1,
                    common::IncidentStatus::Resolved => incidents_summary.resolved += 1,
                }
                if crate::reports_handler::report_incident_sla_breached(
                    &detail.incident,
                    Utc::now(),
                ) {
                    incidents_summary.sla_breached += 1;
                }
            }
        }
        Err(error) => errors.push(format!("incidents: {error}")),
    }

    let mut ontology_summary = ReportsOntologySummary::default();
    let mut action_summary = ReportsActionSummary::default();
    if let Some(client) = crate::ontology_client::global() {
        match client.list_object_types(&principal.query_token).await {
            Ok(types) => ontology_summary.object_types = types.len(),
            Err(error) => errors.push(format!("ontology object types: {error}")),
        }
        match client.list_objects(&principal.query_token, None).await {
            Ok(objects) => ontology_summary.objects = objects.len(),
            Err(error) => errors.push(format!("ontology objects: {error}")),
        }
        match client.list_action_invocations(&principal.query_token).await {
            Ok(actions) => {
                for action in actions.into_iter().filter(|action| {
                    since.map(|value| action.executed_at >= value).unwrap_or(true)
                        && until.map(|value| action.executed_at <= value).unwrap_or(true)
                }) {
                    if action.outcome.eq_ignore_ascii_case("completed") {
                        action_summary.completed += 1;
                    } else {
                        action_summary.needs_review += 1;
                    }
                }
            }
            Err(error) => errors.push(format!("actions: {error}")),
        }
    } else {
        errors.push("ontology: client unavailable".to_string());
    }

    axum::Json(ReportsSummaryApiResponse {
        window: ReportsSummaryWindow { from: query.from, to: query.to },
        connectors,
        event_counts,
        signal_count: events.len(),
        record_count,
        incidents: incidents_summary,
        ontology: ontology_summary,
        actions: action_summary,
        errors,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateReportScheduleApiRequest {
    pub name: String,
    pub frequency: String,
    pub recipient: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default = "default_report_format")]
    pub format: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

fn default_report_format() -> String {
    "csv".to_string()
}

fn report_schedule_filter(
    request: &CreateReportScheduleApiRequest,
) -> Result<serde_json::Value, Response> {
    if request.name.trim().is_empty() || request.name.len() > 200 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "name must be between 1 and 200 characters",
        ));
    }
    if !matches!(request.frequency.trim(), "daily" | "weekly" | "monthly") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "frequency must be daily, weekly, or monthly",
        ));
    }
    if !request.recipient.contains('@') {
        return Err(api_error(StatusCode::BAD_REQUEST, "recipient must be an email address"));
    }
    if !matches!(request.format.trim(), "csv" | "pdf") {
        return Err(api_error(StatusCode::BAD_REQUEST, "format must be csv or pdf"));
    }
    Ok(
        serde_json::json!({"view_kind":"report_schedule","frequency":request.frequency.trim(),"recipient":request.recipient.trim(),"from":request.from,"to":request.to,"format":request.format.trim(),"enabled":request.enabled.unwrap_or(true)}),
    )
}

pub async fn create_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateReportScheduleApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let filter = match report_schedule_filter(&request) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state
        .saved_search_queries_client
        .create(principal.tenant_id, request.name.trim(), filter)
        .await
    {
        Ok(query) => match schedule_from_query(query) {
            Some(schedule) => (StatusCode::CREATED, axum::Json(schedule)).into_response(),
            None => {
                api_error(StatusCode::BAD_GATEWAY, "invalid schedule returned by config service")
            }
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

async fn find_report_schedule(
    state: &AppState,
    tenant_id: Uuid,
    id: Uuid,
) -> Option<SavedSearchQuery> {
    state
        .saved_search_queries_client
        .list(tenant_id)
        .await
        .ok()?
        .into_iter()
        .find(|query| query.id == id && schedule_from_query(query.clone()).is_some())
}

pub async fn delete_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if find_report_schedule(&state, principal.tenant_id, id).await.is_none() {
        return api_error(StatusCode::NOT_FOUND, "report schedule not found");
    }
    match state.saved_search_queries_client.delete(principal.tenant_id, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<CreateReportScheduleApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if find_report_schedule(&state, principal.tenant_id, id).await.is_none() {
        return api_error(StatusCode::NOT_FOUND, "report schedule not found");
    }
    let filter = match report_schedule_filter(&request) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let query = SavedSearchQuery {
        id,
        tenant_id: principal.tenant_id,
        name: request.name.trim().to_string(),
        filter,
    };
    match state.saved_search_queries_client.update(principal.tenant_id, query).await {
        Ok(updated) => match schedule_from_query(updated) {
            Some(schedule) => axum::Json(schedule).into_response(),
            None => {
                api_error(StatusCode::BAD_GATEWAY, "invalid schedule returned by config service")
            }
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn toggle_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let Some(query) = find_report_schedule(&state, principal.tenant_id, id).await else {
        return api_error(StatusCode::NOT_FOUND, "report schedule not found");
    };
    let mut filter = query.filter.clone();
    let enabled = filter.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(true);
    filter["enabled"] = serde_json::Value::Bool(!enabled);
    match state
        .saved_search_queries_client
        .update(
            principal.tenant_id,
            SavedSearchQuery { id: query.id, tenant_id: query.tenant_id, name: query.name, filter },
        )
        .await
    {
        Ok(query) => match schedule_from_query(query) {
            Some(schedule) => axum::Json(schedule).into_response(),
            None => {
                api_error(StatusCode::BAD_GATEWAY, "invalid schedule returned by config service")
            }
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn run_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let Some(query) = find_report_schedule(&state, principal.tenant_id, id).await else {
        return api_error(StatusCode::NOT_FOUND, "report schedule not found");
    };
    let Some(schedule) = schedule_from_query(query) else {
        return api_error(StatusCode::BAD_REQUEST, "invalid report schedule");
    };
    let extension = if schedule.format == "pdf" { "pdf" } else { "csv" };
    let params =
        serde_urlencoded::to_string([("from", schedule.from.clone()), ("to", schedule.to.clone())])
            .unwrap_or_default();
    let mut run = ReportRun::new(
        principal.tenant_id,
        schedule.id,
        schedule.name,
        schedule.recipient,
        format!("/reports/export.{extension}?{params}"),
    );
    run.format = schedule.format;
    if state
        .saved_search_queries_client
        .create_report_run(principal.role, run.clone())
        .await
        .is_err()
    {
        return api_error(StatusCode::BAD_GATEWAY, "unable to create report run");
    }
    let since = chrono::NaiveDate::parse_from_str(&schedule.from, "%Y-%m-%d")
        .ok()
        .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_utc());
    let until = chrono::NaiveDate::parse_from_str(&schedule.to, "%Y-%m-%d")
        .ok()
        .map(|date| date.and_hms_opt(23, 59, 59).unwrap().and_utc());
    match state.events_client.list_events(&principal.query_token, 1000, 0, since, until).await {
        Ok(_) => {
            run.status = "generated".to_string();
        }
        Err(error) => {
            run.status = "failed".to_string();
            run.error = Some(error.to_string());
            run.artifact_url = None;
        }
    }
    run.completed_at = Some(chrono::Utc::now());
    match state.saved_search_queries_client.update_report_run(principal.role, run.clone()).await {
        Ok(run) => axum::Json(run).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct SensorsApiQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub q: Option<String>,
    pub connector_type: Option<String>,
    pub enabled: Option<bool>,
}

fn sensor_api_matches(sensor: &common::Sensor, query: &SensorsApiQuery) -> bool {
    let q = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let connector_type =
        query.connector_type.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    (q.is_empty()
        || sensor.name.to_ascii_lowercase().contains(&q)
        || sensor.id.to_string().contains(&q))
        && (connector_type.is_empty()
            || sensor.connector_type.eq_ignore_ascii_case(&connector_type))
        && query.enabled.is_none_or(|enabled| sensor.enabled == enabled)
}

#[derive(Debug, Serialize)]
struct SensorsApiResponse {
    sensors: Vec<common::Sensor>,
    has_more: bool,
    limit: i64,
    offset: i64,
}

pub async fn list_sensors(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SensorsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let offset = query.offset.unwrap_or(0).max(0);
    match state.sensors_client.list_sensors(principal.tenant_id, limit, offset).await {
        Ok(page) => {
            let sensors = page
                .sensors
                .into_iter()
                .filter(|sensor| sensor_api_matches(sensor, &query))
                .collect();
            axum::Json(SensorsApiResponse { sensors, has_more: page.has_more, limit, offset })
                .into_response()
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.sensors_client.get_sensor(principal.tenant_id, id).await {
        Ok(Some(sensor)) => axum::Json(sensor).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "sensor not found"),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterSensorApiRequest {
    pub connector_type: String,
    pub name: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

fn valid_connector_type(value: &str) -> bool {
    matches!(value, "zendesk" | "graph-mail" | "graph-teams" | "sql" | "fabric" | "generic")
}

pub async fn register_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<RegisterSensorApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if !valid_connector_type(request.connector_type.trim()) {
        return api_error(StatusCode::BAD_REQUEST, "unsupported connector_type");
    }
    if request.name.trim().is_empty() || request.name.len() > 160 {
        return api_error(StatusCode::BAD_REQUEST, "name must be between 1 and 160 characters");
    }
    match state
        .sensors_client
        .register_sensor(
            principal.role,
            &principal.username,
            principal.tenant_id,
            request.connector_type.trim(),
            request.name.trim(),
            request.config,
        )
        .await
    {
        Ok(sensor) => (StatusCode::CREATED, axum::Json(sensor)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(mut sensor): axum::Json<common::Sensor>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if sensor.id != id {
        sensor.id = id;
    }
    sensor.tenant_id = principal.tenant_id;
    if !valid_connector_type(sensor.connector_type.trim()) {
        return api_error(StatusCode::BAD_REQUEST, "unsupported connector_type");
    }
    match state.sensors_client.update_sensor(principal.role, &principal.username, &sensor).await {
        Ok(sensor) => axum::Json(sensor).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .sensors_client
        .delete_sensor(principal.role, &principal.username, principal.tenant_id, id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkDeleteSensorsApiRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct BulkDeleteSensorsApiResponse {
    pub deleted: Vec<Uuid>,
    pub failed: Vec<Uuid>,
}

pub async fn bulk_delete_sensors_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkDeleteSensorsApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 sensors");
    }
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        match state.sensors_client.get_sensor(principal.tenant_id, id).await {
            Ok(Some(_)) => {}
            _ => {
                failed.push(id);
                continue;
            }
        }
        match state
            .sensors_client
            .delete_sensor(principal.role, &principal.username, principal.tenant_id, id)
            .await
        {
            Ok(()) => deleted.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(BulkDeleteSensorsApiResponse { deleted, failed }).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct NormalizationMappingsApiQuery {
    pub q: Option<String>,
    pub source_type: Option<String>,
}

pub async fn list_normalization_mappings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NormalizationMappingsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let q = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let source_type = query.source_type.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    match state.normalization_mappings_client.list_mappings(principal.tenant_id).await {
        Ok(mappings) => axum::Json(
            mappings
                .into_iter()
                .filter(|mapping| {
                    (q.is_empty() || mapping.source_type.to_ascii_lowercase().contains(&q))
                        && (source_type.is_empty()
                            || mapping.source_type.eq_ignore_ascii_case(&source_type))
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn validate_mapping(mapping: &common::NormalizationMapping) -> Result<(), Response> {
    if mapping.source_type.trim().is_empty() || mapping.source_type.len() > 160 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "source_type must be between 1 and 160 characters",
        ));
    }
    if mapping.field_map.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "field_map must contain at least one mapping",
        ));
    }
    if mapping.dedup_window_seconds.is_some_and(|value| value < 1) {
        return Err(api_error(StatusCode::BAD_REQUEST, "dedup_window_seconds must be positive"));
    }
    Ok(())
}

pub async fn create_normalization_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(mut mapping): axum::Json<common::NormalizationMapping>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    mapping.tenant_id = principal.tenant_id;
    mapping.id = Uuid::new_v4();
    mapping.version = 1;
    if let Err(e) = validate_mapping(&mapping) {
        return e;
    }
    match state
        .normalization_mappings_client
        .create_mapping(principal.role, &principal.username, mapping)
        .await
    {
        Ok(mapping) => (StatusCode::CREATED, axum::Json(mapping)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_normalization_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(mut mapping): axum::Json<common::NormalizationMapping>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    mapping.tenant_id = principal.tenant_id;
    mapping.id = id;
    if let Err(e) = validate_mapping(&mapping) {
        return e;
    }
    match state
        .normalization_mappings_client
        .update_mapping(principal.role, &principal.username, mapping)
        .await
    {
        Ok(mapping) => axum::Json(mapping).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_normalization_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .normalization_mappings_client
        .delete_mapping(principal.role, &principal.username, principal.tenant_id, id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn ontology_client() -> Result<std::sync::Arc<dyn crate::ontology_client::OntologyClient>, Response>
{
    crate::ontology_client::global()
        .ok_or_else(|| api_error(StatusCode::BAD_GATEWAY, "ontology service is not configured"))
}

pub async fn list_ontology_object_types(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_object_types(&principal.query_token).await {
        Ok(types) => axum::Json(types).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_ontology_object_type(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_object_types(&principal.query_token).await {
        Ok(types) => match types.into_iter().find(|value| value.id == id) {
            Some(value) => axum::Json(value).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "ontology object type not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_object_type_history(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_object_type_history(&principal.query_token, id).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// Returns tenant-scoped immutable history for every object-type contract,
/// including object types that have been deleted.
pub async fn list_ontology_object_type_history_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_all_object_type_history(&principal.query_token).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_link_types(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_link_types(&principal.query_token).await {
        Ok(types) => axum::Json(types).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_link_type_history(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_link_type_history(&principal.query_token, id).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// Returns tenant-scoped immutable history for every relationship contract,
/// including relationship types that have been deleted.
pub async fn list_ontology_link_type_history_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_all_link_type_history(&principal.query_token).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_ontology_link_type(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_link_types(&principal.query_token).await {
        Ok(types) => match types.into_iter().find(|value| value.id == id) {
            Some(value) => axum::Json(value).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "ontology link type not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateOntologyObjectTypeApiRequest {
    pub name: String,
    pub version: i32,
    pub property_schema: serde_json::Value,
    pub mapping_rules: serde_json::Value,
}

fn validate_ontology_name(name: &str) -> Result<(), Response> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 160 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "ontology name must be between 1 and 160 characters",
        ));
    }
    Ok(())
}

fn validate_ontology_json(value: &serde_json::Value, field: &str) -> Result<(), Response> {
    if !value.is_object() {
        return Err(api_error(StatusCode::BAD_REQUEST, format!("{field} must be a JSON object")));
    }
    Ok(())
}

pub async fn create_ontology_object_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateOntologyObjectTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if let Err(e) = validate_ontology_name(&request.name)
        .and_then(|_| validate_ontology_json(&request.property_schema, "property_schema"))
        .and_then(|_| validate_ontology_json(&request.mapping_rules, "mapping_rules"))
    {
        return e;
    }
    if request.version < 1 {
        return api_error(StatusCode::BAD_REQUEST, "ontology version must be positive");
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateObjectTypeRequest {
        name: request.name.trim().to_string(),
        version: request.version,
        property_schema: request.property_schema,
        mapping_rules: request.mapping_rules,
    };
    match client.create_object_type(&principal.query_token, &input).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_ontology_object_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<CreateOntologyObjectTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if let Err(e) = validate_ontology_name(&request.name)
        .and_then(|_| validate_ontology_json(&request.property_schema, "property_schema"))
        .and_then(|_| validate_ontology_json(&request.mapping_rules, "mapping_rules"))
    {
        return e;
    }
    if request.version < 1 {
        return api_error(StatusCode::BAD_REQUEST, "ontology version must be positive");
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateObjectTypeRequest {
        name: request.name.trim().to_string(),
        version: request.version,
        property_schema: request.property_schema,
        mapping_rules: request.mapping_rules,
    };
    match client.update_object_type(&principal.query_token, id, &input).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_ontology_object_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if principal.role != Role::Admin {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.delete_object_type(&principal.query_token, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateOntologyLinkTypeApiRequest {
    pub name: String,
    pub source_object_type_id: Uuid,
    pub target_object_type_id: Uuid,
    pub cardinality: String,
    #[serde(default)]
    pub properties_schema: Option<serde_json::Value>,
}

fn validate_link_cardinality(value: &str) -> Result<(), Response> {
    if matches!(value, "many-to-one" | "one-to-many" | "one-to-one") {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::BAD_REQUEST,
            "cardinality must be many-to-one, one-to-many, or one-to-one",
        ))
    }
}

pub async fn create_ontology_link_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateOntologyLinkTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if let Err(e) = validate_ontology_name(&request.name)
        .and_then(|_| validate_link_cardinality(&request.cardinality))
    {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateLinkTypeRequest {
        name: request.name.trim().to_string(),
        source_object_type_id: request.source_object_type_id,
        target_object_type_id: request.target_object_type_id,
        cardinality: request.cardinality,
        properties_schema: request.properties_schema,
    };
    match client.create_link_type(&principal.query_token, &principal.username, &input).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_ontology_link_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<CreateOntologyLinkTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if let Err(e) = validate_ontology_name(&request.name)
        .and_then(|_| validate_link_cardinality(&request.cardinality))
    {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateLinkTypeRequest {
        name: request.name.trim().to_string(),
        source_object_type_id: request.source_object_type_id,
        target_object_type_id: request.target_object_type_id,
        cardinality: request.cardinality,
        properties_schema: request.properties_schema,
    };
    match client.update_link_type(&principal.query_token, &principal.username, id, &input).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_ontology_link_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if principal.role != Role::Admin {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.delete_link_type(&principal.query_token, &principal.username, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateOntologyLinkApiRequest {
    pub link_type_id: Uuid,
    pub source_object_id: Uuid,
    pub target_object_id: Uuid,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OntologyLinksApiQuery {
    pub link_type_id: Option<Uuid>,
    pub source_object_id: Option<Uuid>,
    pub target_object_id: Option<Uuid>,
}

pub async fn list_ontology_links(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyLinksApiQuery>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_links(&principal.query_token).await {
        Ok(links) => axum::Json(
            links
                .into_iter()
                .filter(|link| {
                    query.link_type_id.is_none_or(|id| link.link_type_id == id)
                        && query.source_object_id.is_none_or(|id| link.source_object_id == id)
                        && query.target_object_id.is_none_or(|id| link.target_object_id == id)
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_ontology_link(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_links(&principal.query_token).await {
        Ok(links) => match links.into_iter().find(|link| link.id == id) {
            Some(link) => axum::Json(link).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "ontology link not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn create_ontology_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateOntologyLinkApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateLinkRequest {
        link_type_id: request.link_type_id,
        source_object_id: request.source_object_id,
        target_object_id: request.target_object_id,
        properties: request.properties,
    };
    match client.create_link(&principal.query_token, &principal.username, &input).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_link_history(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_link_history(&principal.query_token, id).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_link_history_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_all_link_history(&principal.query_token).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkOntologyLinkInstancesApiRequest {
    pub link_type_id: Uuid,
    pub source_object_ids: Vec<Uuid>,
    pub target_object_id: Uuid,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
}

/// POST /api/v1/ontology/links/instances/bulk — creates the same bounded source-selection
/// relationship batch offered by the Ontology workbench. Each instance uses the normal ontology
/// service create contract, so cardinality, object type, tenant, and audit rules remain owned by
/// the ontology service.
pub async fn create_bulk_ontology_link_instances_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkOntologyLinkInstancesApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if request.source_object_ids.is_empty() || request.source_object_ids.len() > 25 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "source_object_ids must contain between 1 and 25 objects",
        );
    }
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let mut created = 0usize;
    let mut failed = 0usize;
    for source_object_id in request.source_object_ids {
        let input = crate::ontology_client::CreateLinkRequest {
            link_type_id: request.link_type_id,
            source_object_id,
            target_object_id: request.target_object_id,
            properties: request.properties.clone(),
        };
        match client.create_link(&principal.query_token, &principal.username, &input).await {
            Ok(()) => created += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(serde_json::json!({ "created": created, "failed": failed })).into_response()
}

pub async fn update_ontology_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<CreateOntologyLinkApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateLinkRequest {
        link_type_id: request.link_type_id,
        source_object_id: request.source_object_id,
        target_object_id: request.target_object_id,
        properties: request.properties,
    };
    match client.update_link(&principal.query_token, &principal.username, id, &input).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_ontology_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if principal.role != Role::Admin {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.delete_link(&principal.query_token, &principal.username, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateOntologyObjectApiRequest {
    pub object_type_id: Uuid,
    pub properties: serde_json::Value,
    #[serde(default)]
    pub source_lineage: serde_json::Value,
}

pub async fn create_ontology_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateOntologyObjectApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateObjectRequest {
        object_type_id: request.object_type_id,
        properties: request.properties,
        source_lineage: request.source_lineage,
    };
    match client.create_object(&principal.query_token, &input).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_ontology_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<CreateOntologyObjectApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::CreateObjectRequest {
        object_type_id: request.object_type_id,
        properties: request.properties,
        source_lineage: request.source_lineage,
    };
    match client.update_object(&principal.query_token, id, &input).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkUpdateOntologyObjectsApiRequest {
    pub object_ids: Vec<Uuid>,
    pub property: String,
    pub value: serde_json::Value,
}

/// Applies one validated property change to a bounded object set. Each update uses the normal
/// ontology mutation path, preserving per-object immutable history and actor attribution.
pub async fn bulk_update_ontology_objects(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkUpdateOntologyObjectsApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let property = request.property.trim();
    if property.is_empty() || property.len() > 120 || request.object_ids.is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "property and at least one object_id are required",
        );
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let objects = match client.list_objects(&principal.query_token, None).await {
        Ok(objects) => objects,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for id in request.object_ids.into_iter().take(25) {
        let Some(object) = objects.iter().find(|object| object.id == id) else {
            failed.push(id);
            continue;
        };
        let mut properties = object.properties.clone();
        let Some(map) = properties.as_object_mut() else {
            failed.push(id);
            continue;
        };
        map.insert(property.to_string(), request.value.clone());
        let input = crate::ontology_client::CreateObjectRequest {
            object_type_id: object.object_type_id,
            properties,
            source_lineage: object.source_lineage.clone(),
        };
        match client.update_object(&principal.query_token, id, &input).await {
            Ok(()) => updated.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(serde_json::json!({"updated": updated, "failed": failed})).into_response()
}

pub async fn delete_ontology_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.delete_object(&principal.query_token, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_object_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_object_history(&principal.query_token, id).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_object_history_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_all_object_history(&principal.query_token).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn traverse_ontology_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((object_id, link_type_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.traverse_links(&principal.query_token, object_id, link_type_id).await {
        Ok(result) => axum::Json(result).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_actions(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_action_invocations(&principal.query_token).await {
        Ok(actions) => axum::Json(actions).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ActionsApiQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub q: Option<String>,
    pub outcome: Option<String>,
    pub action: Option<String>,
    pub review: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionApiItem {
    pub invocation: common::ontology::ActionInvocation,
    pub action_type: Option<common::ontology::ActionType>,
    pub review: Option<common::ontology::ActionReview>,
}

#[derive(Debug, Serialize)]
pub struct ActionsApiResponse {
    pub actions: Vec<ActionApiItem>,
    pub has_more: bool,
    pub limit: u32,
    pub offset: u32,
}

fn action_api_date_range(
    from: Option<&String>,
    to: Option<&String>,
) -> Result<(Option<DateTime<Utc>>, Option<DateTime<Utc>>), Response> {
    let parse = |value: Option<&String>, end_of_day: bool| {
        value
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .ok()
                    .and_then(|date| {
                        date.and_hms_opt(
                            if end_of_day { 23 } else { 0 },
                            if end_of_day { 59 } else { 0 },
                            if end_of_day { 59 } else { 0 },
                        )
                    })
                    .map(|date| DateTime::<Utc>::from_naive_utc_and_offset(date, Utc))
                    .ok_or_else(|| {
                        api_error(StatusCode::BAD_REQUEST, "action dates must be YYYY-MM-DD")
                    })
            })
            .transpose()
    };
    let start = parse(from, false)?;
    let end = parse(to, true)?;
    if start.zip(end).is_some_and(|(start, end)| start > end) {
        return Err(api_error(StatusCode::BAD_REQUEST, "action from date must not exceed to date"));
    }
    Ok((start, end))
}

fn action_api_review_matches(
    review: Option<&common::ontology::ActionReview>,
    filter: &str,
) -> bool {
    let filter = filter.trim().to_ascii_lowercase();
    if filter.is_empty() {
        return true;
    }
    let Some(review) = review else {
        return filter == "unreviewed";
    };
    let status = review.status.replace('-', "_").to_ascii_lowercase();
    match filter.as_str() {
        "unreviewed" => false,
        "assigned" => review.assignee.as_deref().is_some_and(|value| !value.trim().is_empty()),
        value => status == value || status.replace('_', " ") == value.replace('_', " "),
    }
}

pub async fn list_actions_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActionsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0);
    let (from, to) = match action_api_date_range(query.from.as_ref(), query.to.as_ref()) {
        Ok(range) => range,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (invocations, action_types, reviews) = tokio::join!(
        client.list_action_invocations(&principal.query_token),
        client.list_action_types(&principal.query_token),
        client.list_action_reviews(&principal.query_token),
    );
    let invocations = match invocations {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let action_types = match action_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let reviews = match reviews {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let type_by_id = action_types.iter().map(|value| (value.id, value)).collect::<HashMap<_, _>>();
    let review_by_id =
        reviews.iter().map(|value| (value.invocation_id, value)).collect::<HashMap<_, _>>();
    let query_text = query.q.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let outcome = query.outcome.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let action = query.action.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let review = query.review.as_deref().unwrap_or("");
    let mut matching = invocations
        .into_iter()
        .filter(|invocation| {
            let action_type = type_by_id.get(&invocation.action_type_id).copied();
            let action_name = action_type.map(|value| value.name.as_str()).unwrap_or("");
            let review_entry = review_by_id.get(&invocation.id).copied();
            let searchable = format!(
                "{} {} {} {} {} {}",
                invocation.id,
                action_name,
                invocation.outcome,
                invocation.action_type_id,
                invocation.parameters,
                invocation.target_object_ids
            )
            .to_ascii_lowercase();
            let outcome_match =
                outcome.is_empty() || invocation.outcome.eq_ignore_ascii_case(&outcome);
            let action_match = action.is_empty()
                || action_name.eq_ignore_ascii_case(&action)
                || invocation.action_type_id.to_string() == action;
            let date_match = from.is_none_or(|value| invocation.executed_at >= value)
                && to.is_none_or(|value| invocation.executed_at <= value);
            !(!query_text.is_empty() && !searchable.contains(&query_text))
                && outcome_match
                && action_match
                && date_match
                && action_api_review_matches(review_entry, review)
        })
        .collect::<Vec<_>>();
    matching.sort_by_key(|invocation| std::cmp::Reverse(invocation.executed_at));
    let total = matching.len();
    let actions = matching
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|invocation| ActionApiItem {
            action_type: type_by_id.get(&invocation.action_type_id).cloned().cloned(),
            review: review_by_id.get(&invocation.id).cloned().cloned(),
            invocation,
        })
        .collect::<Vec<_>>();
    axum::Json(ActionsApiResponse {
        actions,
        has_more: (offset as usize).saturating_add(limit as usize) < total,
        limit,
        offset,
    })
    .into_response()
}

/// Returns one governed action invocation with its contract and human-review state. All three
/// records are resolved through the tenant-scoped ontology client using the caller's token.
pub async fn get_action_api(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (invocations, action_types, reviews) = tokio::join!(
        client.list_action_invocations(&principal.query_token),
        client.list_action_types(&principal.query_token),
        client.list_action_reviews(&principal.query_token),
    );
    let invocations = match invocations {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let action_types = match action_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let reviews = match reviews {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let Some(invocation) = invocations.into_iter().find(|value| value.id == id) else {
        return api_error(StatusCode::NOT_FOUND, "action invocation not found");
    };
    axum::Json(ActionApiItem {
        action_type: action_types.into_iter().find(|value| value.id == invocation.action_type_id),
        review: reviews.into_iter().find(|value| value.invocation_id == id),
        invocation,
    })
    .into_response()
}

#[derive(Debug, Serialize)]
struct Action360ApiResponse {
    invocation: common::ontology::ActionInvocation,
    action_type: Option<common::ontology::ActionType>,
    review: Option<common::ontology::ActionReview>,
    targets: Vec<common::ontology::Object>,
    object_types: Vec<common::ontology::ObjectType>,
    event: Option<crate::events_client::EventDetail>,
    incident: Option<IncidentDetail>,
    records: Vec<crate::RecordSummary>,
    errors: Vec<String>,
}

/// Returns the complete governed-decision context for one immutable invocation, including its
/// contract, human review, modeled targets, source evidence, and causal event/case handoff.
pub async fn get_action_360(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (invocations, action_types, reviews, objects, object_types) = tokio::join!(
        client.list_action_invocations(&principal.query_token),
        client.list_action_types(&principal.query_token),
        client.list_action_reviews(&principal.query_token),
        client.list_objects(&principal.query_token, None),
        client.list_object_types(&principal.query_token),
    );
    let invocation = match invocations {
        Ok(items) => match items.into_iter().find(|item| item.id == id) {
            Some(item) => item,
            None => return api_error(StatusCode::NOT_FOUND, "action invocation not found"),
        },
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut errors = Vec::new();
    let action_type = match action_types {
        Ok(items) => items.into_iter().find(|item| item.id == invocation.action_type_id),
        Err(error) => {
            errors.push(format!("action type: {error}"));
            None
        }
    };
    let review = match reviews {
        Ok(items) => items.into_iter().find(|item| item.invocation_id == id),
        Err(error) => {
            errors.push(format!("review: {error}"));
            None
        }
    };
    let target_ids = invocation
        .target_object_ids
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|value| Uuid::parse_str(value).ok())
        .collect::<std::collections::HashSet<_>>();
    let targets = match objects {
        Ok(objects) => {
            objects.into_iter().filter(|object| target_ids.contains(&object.id)).take(100).collect()
        }
        Err(error) => {
            errors.push(format!("targets: {error}"));
            Vec::new()
        }
    };
    let object_types = match object_types {
        Ok(types) => types
            .into_iter()
            .filter(|object_type| {
                targets.iter().any(|object| object.object_type_id == object_type.id)
            })
            .collect(),
        Err(error) => {
            errors.push(format!("target types: {error}"));
            Vec::new()
        }
    };
    let event_id = event_ref_api(&invocation.triggering_event_ref);
    let incident_id = invocation
        .triggering_event_ref
        .get("incident_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let event = if let Some(event_id) = event_id {
        match state.events_client.get_event(&principal.query_token, event_id).await {
            Ok(event) => event,
            Err(error) => {
                errors.push(format!("event: {error}"));
                None
            }
        }
    } else {
        None
    };
    let incident = if let Some(incident_id) = incident_id {
        match state.incidents_client.get_incident(principal.tenant_id, incident_id).await {
            Ok(incident) => incident,
            Err(error) => {
                errors.push(format!("incident: {error}"));
                None
            }
        }
    } else {
        None
    };
    let mut records = Vec::new();
    if let Some(event) = &event {
        for record_id in event.record_ids.iter().take(100) {
            match state.stats_client.get_record(principal.tenant_id, *record_id).await {
                Ok(Some(record)) => records.push(record),
                Ok(None) => {}
                Err(error) => errors.push(format!("record {record_id}: {error}")),
            }
        }
    }
    axum::Json(Action360ApiResponse {
        invocation,
        action_type,
        review,
        targets,
        object_types,
        event,
        incident,
        records,
        errors,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct UpsertActionReviewApiRequest {
    pub status: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
}

/// Updates the human-review state from an action-centric URL, matching the browser detail page
/// while retaining operator-only governance and the ontology service as the source of truth.
pub async fn upsert_action_review_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<UpsertActionReviewApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let input = crate::ontology_client::ActionReviewRequest {
        invocation_id: id,
        status: request.status,
        assignee: request.assignee,
        note: request.note,
        due_at: request.due_at,
    };
    match client.upsert_action_review(&principal.query_token, &input).await {
        Ok(review) => axum::Json(review).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkActionReviewApiRequest {
    pub ids: Vec<Uuid>,
    pub status: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
}

fn validate_action_review_transition(
    principal: &ApiPrincipal,
    status: &str,
) -> Result<String, Response> {
    let status = status.trim().to_ascii_lowercase();
    if !matches!(status.as_str(), "open" | "in_progress" | "approved" | "declined" | "handed_off") {
        return Err(api_error(StatusCode::BAD_REQUEST, "unknown action review status"));
    }
    if matches!(status.as_str(), "approved" | "declined") && !principal.role.at_least(Role::Admin) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "admin role required for final action review transitions",
        ));
    }
    Ok(status)
}

pub async fn bulk_action_review_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkActionReviewApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 actions");
    }
    let status = match validate_action_review_transition(&principal, &request.status) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if request.note.len() > 20_000 {
        return api_error(StatusCode::BAD_REQUEST, "note must not exceed 20000 characters");
    }
    if request.assignee.as_deref().is_some_and(|value| value.len() > 254) {
        return api_error(StatusCode::BAD_REQUEST, "assignee must not exceed 254 characters");
    }
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let known_ids = match client.list_action_invocations(&principal.query_token).await {
        Ok(invocations) => {
            invocations.into_iter().map(|value| value.id).collect::<std::collections::HashSet<_>>()
        }
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let mut saved = 0usize;
    let mut failed = 0usize;
    for id in ids.iter().copied() {
        if !known_ids.contains(&id) {
            failed += 1;
            continue;
        }
        let input = crate::ontology_client::ActionReviewRequest {
            invocation_id: id,
            status: status.clone(),
            assignee: request.assignee.clone(),
            note: request.note.clone(),
            due_at: request.due_at,
        };
        match client.upsert_action_review(&principal.query_token, &input).await {
            Ok(_) => saved += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(serde_json::json!({
        "requested": ids.len(),
        "saved": saved,
        "failed": failed,
        "status": status,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct BulkActionRetryApiRequest {
    pub ids: Vec<Uuid>,
}

pub async fn bulk_retry_actions_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkActionRetryApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 actions");
    }
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let invocations = match client.list_action_invocations(&principal.query_token).await {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let mut retried = 0usize;
    let mut failed = 0usize;
    for id in ids.iter().copied() {
        let Some(invocation) = invocations
            .iter()
            .find(|value| value.id == id && !value.outcome.eq_ignore_ascii_case("completed"))
        else {
            failed += 1;
            continue;
        };
        let target_object_ids = invocation
            .target_object_ids
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter_map(|value| value.parse::<Uuid>().ok())
            .collect::<Vec<_>>();
        if target_object_ids.is_empty() {
            failed += 1;
            continue;
        }
        let input = crate::ontology_client::InvokeActionRequest {
            action_type_id: invocation.action_type_id,
            target_object_ids,
            parameters: invocation.parameters.clone(),
            triggering_event_ref: Some(invocation.triggering_event_ref.clone()),
        };
        match client.invoke_action(&principal.query_token, &input).await {
            Ok(_) => retried += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(serde_json::json!({ "requested": ids.len(), "retried": retried, "failed": failed }))
        .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct ReplayDeadLetterApiRequest {
    #[serde(default = "default_action_executor_service")]
    pub service: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct PeekDeadLetterApiRequest {
    #[serde(default = "default_action_executor_service")]
    pub service: String,
}

fn default_action_executor_service() -> String {
    "action-executor".to_string()
}

pub async fn replay_dead_letter_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ReplayDeadLetterApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let service = request.service.trim();
    if service.is_empty() || service.len() > 120 {
        return api_error(StatusCode::BAD_REQUEST, "service must be between 1 and 120 characters");
    }
    let queues = match state.execution_client.dead_letter_queues().await {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    if !queues.iter().any(|queue| queue.service == service && queue.has_messages) {
        return api_error(StatusCode::NOT_FOUND, "dead-letter queue is empty or unknown");
    }
    match state.execution_client.replay_dead_letter_queue(service).await {
        Ok(replayed) => axum::Json(serde_json::json!({ "service": service, "replayed": replayed }))
            .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// Returns a bounded, non-consuming preview of the oldest message in an operator-visible
/// pipeline dead-letter queue. Queue payloads are intentionally not tenant-filtered because the
/// underlying operational queues mix tenants; access is restricted to operator principals.
pub async fn peek_dead_letter_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<PeekDeadLetterApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let service = request.service.trim();
    if service.is_empty() || service.len() > 120 {
        return api_error(StatusCode::BAD_REQUEST, "service must be between 1 and 120 characters");
    }
    match state.execution_client.dead_letter_preview(service).await {
        Ok(Some(preview)) => axum::Json(serde_json::json!({
            "service": service,
            "preview": preview,
        }))
        .into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "dead-letter queue is empty or unknown"),
        Err(crate::execution_client::ExecutionClientError::Rejected(404)) => {
            api_error(StatusCode::NOT_FOUND, "dead-letter queue is empty or unknown")
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn export_actions_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActionsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (from, to) = match action_api_date_range(query.from.as_ref(), query.to.as_ref()) {
        Ok(range) => range,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (invocations, action_types, reviews) = tokio::join!(
        client.list_action_invocations(&principal.query_token),
        client.list_action_types(&principal.query_token),
        client.list_action_reviews(&principal.query_token),
    );
    let invocations = match invocations {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let action_types = match action_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let reviews = match reviews {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let type_by_id = action_types.iter().map(|value| (value.id, value)).collect::<HashMap<_, _>>();
    let review_by_id =
        reviews.iter().map(|value| (value.invocation_id, value)).collect::<HashMap<_, _>>();
    let query_text = query.q.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let outcome = query.outcome.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let action = query.action.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let review = query.review.as_deref().unwrap_or("");
    let mut csv = String::from(
        "id,action_type_id,action_name,outcome,review_status,assignee,executed_at,target_object_ids,parameters,triggering_event_ref\n",
    );
    let mut rows = invocations
        .into_iter()
        .filter_map(|invocation| {
            let action_type = type_by_id.get(&invocation.action_type_id).copied();
            let action_name = action_type.map(|value| value.name.as_str()).unwrap_or("");
            let review_entry = review_by_id.get(&invocation.id).copied();
            let searchable = format!(
                "{} {} {} {} {} {}",
                invocation.id,
                action_name,
                invocation.outcome,
                invocation.action_type_id,
                invocation.parameters,
                invocation.target_object_ids
            )
            .to_ascii_lowercase();
            let outcome_match =
                outcome.is_empty() || invocation.outcome.eq_ignore_ascii_case(&outcome);
            let action_match = action.is_empty()
                || action_name.eq_ignore_ascii_case(&action)
                || invocation.action_type_id.to_string() == action;
            let date_match = from.is_none_or(|value| invocation.executed_at >= value)
                && to.is_none_or(|value| invocation.executed_at <= value);
            if (!query_text.is_empty() && !searchable.contains(&query_text))
                || !outcome_match
                || !action_match
                || !date_match
                || !action_api_review_matches(review_entry, review)
            {
                return None;
            }
            Some((invocation, action_name.to_string(), review_entry))
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|(invocation, _, _)| std::cmp::Reverse(invocation.executed_at));
    for (invocation, action_name, review_entry) in rows {
        let review_status = review_entry.map(|value| value.status.as_str()).unwrap_or("unreviewed");
        let assignee = review_entry.and_then(|value| value.assignee.as_deref()).unwrap_or("");
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            invocation.id,
            invocation.action_type_id,
            api_csv_escape(&action_name),
            api_csv_escape(&invocation.outcome),
            api_csv_escape(review_status),
            api_csv_escape(assignee),
            invocation.executed_at.to_rfc3339(),
            api_csv_escape(&invocation.target_object_ids.to_string()),
            api_csv_escape(&invocation.parameters.to_string()),
            api_csv_escape(&invocation.triggering_event_ref.to_string()),
        ));
    }
    let mut response = (StatusCode::OK, csv).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_static("attachment; filename=actions.csv"),
    );
    response
}

pub async fn list_ontology_action_types(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_action_types(&principal.query_token).await {
        Ok(types) => axum::Json(types).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ActionLibraryApiQuery {
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionLibraryContractApi {
    pub action_type: common::ontology::ActionType,
    pub target_type: String,
    pub target_count: usize,
    pub eligible_target_count: usize,
    pub invocation_count: usize,
    pub completed_invocation_count: usize,
    pub review_invocation_count: usize,
    pub last_outcome: String,
    pub history: Vec<common::ontology::ActionTypeHistory>,
}

#[derive(Debug, Serialize)]
pub struct ActionLibraryApiResponse {
    pub contracts: Vec<ActionLibraryContractApi>,
    pub total_invocations: usize,
    pub eligible_contract_count: usize,
    pub blocked_contract_count: usize,
}

fn action_preconditions_satisfied(
    properties: &serde_json::Value,
    preconditions: &serde_json::Value,
) -> bool {
    let Some(preconditions) = preconditions.as_object() else {
        return preconditions.is_null() || preconditions == &serde_json::json!({});
    };
    let Some(properties) = properties.as_object() else {
        return false;
    };
    preconditions.iter().all(|(key, expected)| properties.get(key) == Some(expected))
}

/// GET /api/v1/actions/library — returns the operational read model behind the governed Action
/// Library: contract definitions, target eligibility, execution posture, and contract history.
/// Lower-level action-type and invocation endpoints remain available for narrow clients.
pub async fn get_action_library_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActionLibraryApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (action_types, object_types, invocations, objects) = tokio::join!(
        client.list_action_types(&principal.query_token),
        client.list_object_types(&principal.query_token),
        client.list_action_invocations(&principal.query_token),
        client.list_objects(&principal.query_token, None),
    );
    let action_types = match action_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let object_types = match object_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let invocations = match invocations {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let objects = match objects {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let type_names =
        object_types.iter().map(|item| (item.id, item.name.as_str())).collect::<HashMap<_, _>>();
    let query = query.q.unwrap_or_default().trim().to_ascii_lowercase();
    let mut contracts = Vec::new();
    for action_type in action_types {
        let target_type = action_type
            .target_object_type_id
            .and_then(|id| type_names.get(&id).copied())
            .unwrap_or("Any object type")
            .to_string();
        if !query.is_empty()
            && !format!("{} {} {}", action_type.name, target_type, action_type.parameter_schema)
                .to_ascii_lowercase()
                .contains(&query)
        {
            continue;
        }
        let targets = objects.iter().filter(|object| {
            action_type.target_object_type_id.is_none_or(|id| object.object_type_id == id)
        });
        let target_count = targets.clone().count();
        let eligible_target_count = targets
            .filter(|object| {
                action_preconditions_satisfied(&object.properties, &action_type.preconditions)
            })
            .count();
        let related = invocations
            .iter()
            .filter(|item| item.action_type_id == action_type.id)
            .collect::<Vec<_>>();
        let completed_invocation_count =
            related.iter().filter(|item| item.outcome.eq_ignore_ascii_case("completed")).count();
        let last_outcome = related
            .first()
            .map(|item| item.outcome.clone())
            .unwrap_or_else(|| "No executions".to_string());
        let history =
            match client.list_action_type_history(&principal.query_token, action_type.id).await {
                Ok(value) => value,
                Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
            };
        contracts.push(ActionLibraryContractApi {
            action_type,
            target_type,
            target_count,
            eligible_target_count,
            invocation_count: related.len(),
            completed_invocation_count,
            review_invocation_count: related.len().saturating_sub(completed_invocation_count),
            last_outcome,
            history,
        });
    }
    contracts.sort_by(|left, right| {
        left.action_type.name.to_ascii_lowercase().cmp(&right.action_type.name.to_ascii_lowercase())
    });
    let eligible_contract_count =
        contracts.iter().filter(|item| item.eligible_target_count > 0).count();
    axum::Json(ActionLibraryApiResponse {
        total_invocations: contracts.iter().map(|item| item.invocation_count).sum(),
        eligible_contract_count,
        blocked_contract_count: contracts.len().saturating_sub(eligible_contract_count),
        contracts,
    })
    .into_response()
}

pub async fn get_ontology_action_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_action_types(&principal.query_token).await {
        Ok(types) => match types.into_iter().find(|value| value.id == id) {
            Some(value) => axum::Json(value).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "ontology action type not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ActionTypeApiRequest {
    pub name: String,
    #[serde(default)]
    pub target_object_type_id: Option<Uuid>,
    pub parameter_schema: serde_json::Value,
    #[serde(default)]
    pub preconditions: serde_json::Value,
    #[serde(default)]
    pub effect_definition: serde_json::Value,
}

fn action_type_input(
    request: ActionTypeApiRequest,
) -> Result<crate::ontology_client::CreateActionTypeRequest, Response> {
    let name = request.name.trim().to_string();
    if name.is_empty() || name.len() > 160 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "name must be between 1 and 160 characters",
        ));
    }
    if !request.parameter_schema.is_object()
        || !request.preconditions.is_object()
        || !request.effect_definition.is_object()
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "parameter_schema, preconditions, and effect_definition must be JSON objects",
        ));
    }
    Ok(crate::ontology_client::CreateActionTypeRequest {
        name,
        target_object_type_id: request.target_object_type_id,
        parameter_schema: request.parameter_schema,
        preconditions: request.preconditions,
        effect_definition: request.effect_definition,
    })
}

pub async fn create_ontology_action_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ActionTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let input = match action_type_input(request) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.create_action_type(&principal.query_token, &principal.username, &input).await {
        Ok(()) => (StatusCode::CREATED, axum::Json(serde_json::json!({"status": "created"})))
            .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_ontology_action_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<ActionTypeApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let input = match action_type_input(request) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.update_action_type(&principal.query_token, &principal.username, id, &input).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_ontology_action_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.delete_action_type(&principal.query_token, &principal.username, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_action_type_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_action_type_history(&principal.query_token, id).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// Returns the tenant-scoped immutable history for every governed action
/// contract, including contracts that have since been deleted.
pub async fn list_ontology_action_type_history_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    match client.list_all_action_type_history(&principal.query_token).await {
        Ok(history) => axum::Json(history).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_action_reviews(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_action_reviews(&principal.query_token).await {
        Ok(reviews) => axum::Json(reviews).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct InvokeOntologyActionApiRequest {
    pub action_type_id: Uuid,
    pub target_object_ids: Vec<Uuid>,
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub triggering_event_ref: Option<serde_json::Value>,
}

pub async fn invoke_ontology_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<InvokeOntologyActionApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.target_object_ids.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "target_object_ids must not be empty");
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::InvokeActionRequest {
        action_type_id: request.action_type_id,
        target_object_ids: request.target_object_ids,
        parameters: request.parameters,
        triggering_event_ref: request.triggering_event_ref,
    };
    match client.invoke_action(&principal.query_token, &input).await {
        Ok(action) => (StatusCode::CREATED, axum::Json(action)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpsertOntologyReviewApiRequest {
    pub invocation_id: Uuid,
    pub status: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub due_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn upsert_ontology_action_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<UpsertOntologyReviewApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let input = crate::ontology_client::ActionReviewRequest {
        invocation_id: request.invocation_id,
        status: request.status,
        assignee: request.assignee,
        note: request.note,
        due_at: request.due_at,
    };
    match client.upsert_action_review(&principal.query_token, &input).await {
        Ok(review) => axum::Json(review).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn require_admin(principal: &ApiPrincipal) -> Result<(), Response> {
    if principal.role.at_least(Role::Admin) {
        Ok(())
    } else {
        Err(api_error(StatusCode::FORBIDDEN, "admin role required"))
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateServiceAccountApiRequest {
    pub label: String,
    pub role: Role,
}

pub async fn list_service_accounts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state.auth_client.list_service_accounts(principal.tenant_id, principal.role).await {
        Ok(accounts) => axum::Json(accounts).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn create_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateServiceAccountApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    if request.label.trim().is_empty() || request.label.trim().len() > 120 {
        return api_error(StatusCode::BAD_REQUEST, "label must be between 1 and 120 characters");
    }
    match state
        .auth_client
        .create_service_account(
            principal.tenant_id,
            principal.role,
            request.label.trim(),
            request.role,
            &principal.username,
        )
        .await
    {
        Ok((account, token)) => (
            StatusCode::CREATED,
            axum::Json(serde_json::json!({"account": account, "token": token})),
        )
            .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn revoke_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state
        .auth_client
        .revoke_service_account(principal.tenant_id, principal.role, id, &principal.username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

const DASHBOARD_WIDGET_IDS: &[&str] = &[
    "signal-trend",
    "governed-decisions",
    "knowledge-model",
    "pipeline-status",
    "recent-activity",
    "decision-queue",
];

#[derive(Debug, Serialize)]
pub struct DashboardLayoutApiResponse {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
pub struct DashboardLayoutApiRequest {
    pub order: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub scope: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct DashboardScopeApiQuery {
    #[serde(default)]
    pub scope: String,
}

fn normalize_dashboard_scope(raw: &str) -> Result<&'static str, Response> {
    match raw.trim() {
        "" | "personal" => Ok("personal"),
        "workspace" => Ok("workspace"),
        _ => Err(api_error(StatusCode::BAD_REQUEST, "scope must be personal or workspace")),
    }
}

fn dashboard_owner(scope: &str, username: &str) -> String {
    if scope == "workspace" {
        "workspace".to_string()
    } else {
        username.to_string()
    }
}

fn validate_dashboard_layout(
    request: DashboardLayoutApiRequest,
) -> Result<DashboardLayoutApiRequest, Response> {
    let unique = |values: &[String]| {
        values.len() == values.iter().collect::<std::collections::HashSet<_>>().len()
    };
    if request.order.len() != DASHBOARD_WIDGET_IDS.len()
        || !unique(&request.order)
        || !request.order.iter().all(|id| DASHBOARD_WIDGET_IDS.contains(&id.as_str()))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "order must contain each dashboard widget exactly once",
        ));
    }
    if !unique(&request.hidden)
        || !request.hidden.iter().all(|id| DASHBOARD_WIDGET_IDS.contains(&id.as_str()))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "hidden may only contain known dashboard widgets without duplicates",
        ));
    }
    Ok(request)
}

async fn dashboard_queries(
    state: &AppState,
    principal: &ApiPrincipal,
) -> Result<Vec<common::SavedSearchQuery>, Response> {
    state
        .saved_search_queries_client
        .list(principal.tenant_id)
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))
}

pub async fn get_dashboard_layout(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(scope_query): axum::extract::Query<DashboardScopeApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let queries = match dashboard_queries(&state, &principal).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let scope = match normalize_dashboard_scope(&scope_query.scope) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let layout = queries.into_iter().rev().find_map(|query| {
        let filter = query.filter.as_object()?;
        if filter.get("view_kind")?.as_str()? != "dashboard_layout"
            || filter.get("scope").and_then(serde_json::Value::as_str).unwrap_or("personal")
                != scope
            || filter.get("owner")?.as_str()? != dashboard_owner(scope, principal.username.as_str())
        {
            return None;
        }
        let order = filter.get("order")?.as_array()?;
        Some(DashboardLayoutApiResponse {
            order: order.iter().filter_map(serde_json::Value::as_str).map(str::to_string).collect(),
            hidden: filter
                .get("hidden")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items.iter().filter_map(serde_json::Value::as_str).map(str::to_string).collect()
                })
                .unwrap_or_default(),
            scope: filter
                .get("scope")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("personal")
                .to_string(),
        })
    });
    match layout {
        Some(layout) => axum::Json(layout).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn put_dashboard_layout(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(scope_query): axum::extract::Query<DashboardScopeApiQuery>,
    axum::Json(request): axum::Json<DashboardLayoutApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut request = match validate_dashboard_layout(request) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let scope = match normalize_dashboard_scope(if request.scope.is_empty() {
        &scope_query.scope
    } else {
        &request.scope
    }) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if scope == "workspace" && !principal.role.at_least(Role::Operator) {
        return api_error(StatusCode::FORBIDDEN, "operator role required for workspace layouts");
    }
    request.scope = scope.to_string();
    let queries = match dashboard_queries(&state, &principal).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    for query in queries {
        let owned = query.filter.get("view_kind").and_then(serde_json::Value::as_str)
            == Some("dashboard_layout")
            && query.filter.get("scope").and_then(serde_json::Value::as_str).unwrap_or("personal")
                == scope
            && query.filter.get("owner").and_then(serde_json::Value::as_str)
                == Some(dashboard_owner(scope, principal.username.as_str()).as_str());
        if owned {
            let _ = state.saved_search_queries_client.delete(principal.tenant_id, query.id).await;
        }
    }
    let filter = serde_json::json!({
        "view_kind": "dashboard_layout",
        "scope": scope,
        "owner": dashboard_owner(scope, principal.username.as_str()),
        "order": request.order,
        "hidden": request.hidden,
    });
    match state
        .saved_search_queries_client
        .create(
            principal.tenant_id,
            if scope == "workspace" {
                "Workspace dashboard layout"
            } else {
                "Personal dashboard layout"
            },
            filter,
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_dashboard_layout(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(scope_query): axum::extract::Query<DashboardScopeApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let scope = match normalize_dashboard_scope(&scope_query.scope) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if scope == "workspace" && !principal.role.at_least(Role::Operator) {
        return api_error(StatusCode::FORBIDDEN, "operator role required for workspace layouts");
    }
    let queries = match dashboard_queries(&state, &principal).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    for query in queries {
        let owned = query.filter.get("view_kind").and_then(serde_json::Value::as_str)
            == Some("dashboard_layout")
            && query.filter.get("scope").and_then(serde_json::Value::as_str).unwrap_or("personal")
                == scope
            && query.filter.get("owner").and_then(serde_json::Value::as_str)
                == Some(dashboard_owner(scope, principal.username.as_str()).as_str());
        if owned {
            let _ = state.saved_search_queries_client.delete(principal.tenant_id, query.id).await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct DataApiQuery {
    pub connector_id: Option<String>,
    pub source_type: Option<String>,
    pub q: Option<String>,
    pub subject: Option<String>,
    pub email_from: Option<String>,
    pub attachment_filename: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub normalized: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DataApiResponse {
    pub records: Vec<crate::RecordSummary>,
    pub has_more: bool,
    pub limit: i64,
    pub offset: i64,
}

fn data_filter(query: &DataApiQuery) -> Result<crate::RecordSearchFilter, Response> {
    let parse = |value: &Option<String>| {
        value.as_ref().map(|value| {
            value
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, "date values must be RFC3339"))
        })
    };
    Ok(crate::RecordSearchFilter {
        connector_id: query.connector_id.clone().filter(|v| !v.is_empty()),
        source_type: query.source_type.clone().filter(|v| !v.is_empty()),
        query: query.q.clone().filter(|v| !v.is_empty()),
        subject: query.subject.clone().filter(|v| !v.is_empty()),
        email_from: query.email_from.clone().filter(|v| !v.is_empty()),
        attachment_filename: query.attachment_filename.clone().filter(|v| !v.is_empty()),
        from: parse(&query.from).transpose()?,
        to: parse(&query.to).transpose()?,
        normalized: query.normalized,
        limit: query.limit.unwrap_or(25).clamp(1, 1000),
        offset: query.offset.unwrap_or(0).max(0),
    })
}

pub async fn list_data_records(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let filter = match data_filter(&query) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.stats_client.search_records(principal.tenant_id, &filter).await {
        Ok(result) => axum::Json(DataApiResponse {
            records: result.records,
            has_more: result.has_more,
            limit: filter.limit,
            offset: filter.offset,
        })
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_data_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.stats_client.get_record(principal.tenant_id, id).await {
        Ok(Some(record)) => axum::Json(record).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "record not found"),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct DataCompareApiQuery {
    #[serde(default)]
    pub ids: String,
}

#[derive(Debug, Serialize)]
struct DataCompareObjectApi {
    id: Uuid,
    type_name: String,
    label: String,
}

#[derive(Debug, Serialize)]
struct DataCompareFieldApi {
    name: String,
    raw: bool,
    normalized: bool,
    state: String,
}

#[derive(Debug, Serialize)]
struct DataCompareRecordApi {
    record: crate::RecordSummary,
    preview: String,
    raw_field_count: usize,
    normalized_field_count: usize,
    fields: Vec<DataCompareFieldApi>,
    signals: Vec<EventSummary>,
    modeled_objects: Vec<DataCompareObjectApi>,
}

#[derive(Debug, Serialize)]
struct DataCompareMatrixApi {
    name: String,
    cells: Vec<bool>,
    present_count: usize,
    disposition: String,
    distinct_value_count: usize,
}

/// Compares up to four tenant-scoped records, including raw/normalized field presence and
/// downstream signal/model lineage. This is the API projection of the browser Data Compare
/// workbench, not a second data model.
pub async fn get_data_compare_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataCompareApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let requested_values =
        query.ids.split(',').filter(|value| !value.trim().is_empty()).collect::<Vec<_>>();
    if requested_values.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain at least one record UUID");
    }
    let mut ids = Vec::new();
    for value in requested_values.iter().take(4) {
        match Uuid::parse_str(value.trim()) {
            Ok(id) => ids.push(id),
            Err(_) => return api_error(StatusCode::BAD_REQUEST, "ids must contain UUIDs"),
        }
    }

    let mut errors = Vec::new();
    let (type_names, ontology_objects) = if let Some(client) = crate::ontology_client::global() {
        let (types, objects) = tokio::join!(
            client.list_object_types(&principal.query_token),
            client.list_objects(&principal.query_token, None),
        );
        let type_names = match types {
            Ok(types) => types
                .into_iter()
                .map(|item| (item.id, item.name))
                .collect::<std::collections::HashMap<_, _>>(),
            Err(error) => {
                errors.push(format!("ontology types: {error}"));
                std::collections::HashMap::new()
            }
        };
        let objects = match objects {
            Ok(objects) => objects,
            Err(error) => {
                errors.push(format!("ontology objects: {error}"));
                Vec::new()
            }
        };
        (type_names, objects)
    } else {
        errors.push("ontology: client unavailable".to_string());
        (std::collections::HashMap::new(), Vec::new())
    };

    let mut records = Vec::new();
    let mut field_values = std::collections::BTreeMap::<String, Vec<String>>::new();
    for id in ids {
        let Some(record) = (match state.stats_client.get_record(principal.tenant_id, id).await {
            Ok(record) => record,
            Err(error) => {
                errors.push(format!("record {id}: {error}"));
                None
            }
        }) else {
            errors.push(format!("record {id}: not found"));
            continue;
        };
        let raw = record.raw_payload.as_object().cloned().unwrap_or_default();
        let normalized = record
            .normalized_payload
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut names = raw.keys().cloned().collect::<std::collections::BTreeSet<_>>();
        names.extend(normalized.keys().cloned());
        let fields = names
            .into_iter()
            .map(|name| {
                let has_raw = raw.contains_key(&name);
                let has_normalized = normalized.contains_key(&name);
                let state = match (has_raw, has_normalized) {
                    (true, true) => "preserved",
                    (true, false) => "raw-only",
                    (false, true) => "normalized-only",
                    (false, false) => "unknown",
                }
                .to_string();
                let value = normalized
                    .get(&name)
                    .or_else(|| raw.get(&name))
                    .map(|value| serde_json::to_string(value).unwrap_or_default())
                    .unwrap_or_default();
                field_values.entry(name.clone()).or_default().push(value);
                DataCompareFieldApi { name, raw: has_raw, normalized: has_normalized, state }
            })
            .collect::<Vec<_>>();
        let signals = match state
            .events_client
            .list_events_for_record(&principal.query_token, record.id)
            .await
        {
            Ok(signals) => signals,
            Err(error) => {
                errors.push(format!("record {} signals: {error}", record.id));
                Vec::new()
            }
        };
        let record_id = record.id.to_string();
        let modeled_objects = ontology_objects
            .iter()
            .filter(|object| {
                object
                    .source_lineage
                    .as_array()
                    .map(|lineage| lineage.iter().any(|value| value.as_str() == Some(&record_id)))
                    .unwrap_or(false)
            })
            .map(|object| {
                let label = object
                    .properties
                    .get("name")
                    .or_else(|| object.properties.get("subject"))
                    .or_else(|| object.properties.get("title"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled object")
                    .to_string();
                DataCompareObjectApi {
                    id: object.id,
                    type_name: type_names
                        .get(&object.object_type_id)
                        .cloned()
                        .unwrap_or_else(|| "Modeled object".to_string()),
                    label,
                }
            })
            .collect();
        records.push(DataCompareRecordApi {
            preview: record.preview(),
            raw_field_count: raw.len(),
            normalized_field_count: normalized.len(),
            fields,
            signals,
            modeled_objects,
            record,
        });
    }
    let comparison_fields = field_values
        .into_iter()
        .map(|(name, values)| {
            let cells = records
                .iter()
                .map(|record| record.fields.iter().any(|field| field.name == name))
                .collect::<Vec<_>>();
            let present_count = cells.iter().filter(|present| **present).count();
            let distinct_value_count =
                values.iter().collect::<std::collections::HashSet<_>>().len();
            let disposition = if present_count == records.len() {
                "shared"
            } else if present_count == 1 {
                "unique"
            } else {
                "partial"
            };
            DataCompareMatrixApi {
                name,
                cells,
                present_count,
                disposition: disposition.to_string(),
                distinct_value_count,
            }
        })
        .collect::<Vec<_>>();
    let shared_field_count =
        comparison_fields.iter().filter(|field| field.disposition == "shared").count();
    let differing_field_count =
        comparison_fields.iter().filter(|field| field.distinct_value_count > 1).count();
    axum::Json(serde_json::json!({
        "requested_count": requested_values.len().min(4),
        "matched_count": records.len(),
        "records": records,
        "comparison_fields": comparison_fields,
        "shared_field_count": shared_field_count,
        "differing_field_count": differing_field_count,
        "errors": errors,
    }))
    .into_response()
}

#[derive(Debug, Serialize)]
pub struct RecordJourneyApiIncident {
    pub id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct RecordJourneyApiExecution {
    pub execution: crate::ActionExecutionSummary,
    pub latency_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct RecordJourneyApiEvent {
    pub event: EventSummary,
    pub executions: Vec<RecordJourneyApiExecution>,
    pub incident: Option<RecordJourneyApiIncident>,
}

#[derive(Debug, Serialize)]
pub struct RecordJourneyApiCounts {
    pub events: usize,
    pub executions: usize,
    pub incidents: usize,
}

#[derive(Debug, Serialize)]
pub struct RecordJourneyApiResponse {
    pub record: crate::RecordSummary,
    pub events: Vec<RecordJourneyApiEvent>,
    pub counts: RecordJourneyApiCounts,
}

fn journey_latency_ms(from: DateTime<Utc>, to: DateTime<Utc>) -> i64 {
    (to - from).num_milliseconds().max(0)
}

/// Returns the tenant-scoped source-record lineage used by the browser's Record Journey view.
pub async fn get_record_journey_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let record = match state.stats_client.get_record(principal.tenant_id, id).await {
        Ok(Some(record)) => record,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, "record not found"),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let events = match state.events_client.list_events_for_record(&principal.query_token, id).await
    {
        Ok(events) => events,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let incidents = match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(incidents) => incidents,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut event_links = Vec::with_capacity(events.len());
    for event in events {
        let executions = match state
            .execution_client
            .list_executions_for_event(principal.tenant_id, event.id)
            .await
        {
            Ok(executions) => executions
                .into_iter()
                .map(|execution| RecordJourneyApiExecution {
                    latency_ms: journey_latency_ms(event.occurred_at, execution.executed_at),
                    execution,
                })
                .collect(),
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
        let incident =
            incidents.iter().find(|item| item.event_ids.contains(&event.id)).map(|item| {
                RecordJourneyApiIncident {
                    id: item.incident.id,
                    title: item.incident.title.clone(),
                    severity: item.incident.severity.to_string(),
                    status: item.incident.status.to_string(),
                }
            });
        event_links.push(RecordJourneyApiEvent { event, executions, incident });
    }
    let counts = RecordJourneyApiCounts {
        events: event_links.len(),
        executions: event_links.iter().map(|item| item.executions.len()).sum(),
        incidents: event_links.iter().filter(|item| item.incident.is_some()).count(),
    };
    axum::Json(RecordJourneyApiResponse { record, events: event_links, counts }).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct ReprocessDataApiRequest {
    #[serde(default)]
    pub connector_id: Option<String>,
}

pub async fn reprocess_data_records(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ReprocessDataApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .stats_client
        .reprocess_for_connector(principal.tenant_id, request.connector_id.as_deref())
        .await
    {
        Ok(count) => axum::Json(serde_json::json!({"reprocessed": count})).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn reprocess_data_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state.stats_client.reprocess_record(principal.tenant_id, id).await {
        Ok(count) => axum::Json(serde_json::json!({"reprocessed": count})).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ModelDataApiRequest {
    pub object_type_id: Uuid,
    #[serde(default)]
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ModelDataApiResponse {
    pub modeled: Vec<Uuid>,
    pub failed: Vec<Uuid>,
}

async fn model_data_records(
    state: &AppState,
    principal: &ApiPrincipal,
    object_type_id: Uuid,
    ids: Vec<Uuid>,
) -> Result<ModelDataApiResponse, Response> {
    if ids.is_empty() || ids.len() > 25 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "ids must contain between 1 and 25 records",
        ));
    }
    let client = ontology_client()?;
    let mut modeled = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        let Ok(Some(record)) = state.stats_client.get_record(principal.tenant_id, id).await else {
            failed.push(id);
            continue;
        };
        let payload = record.normalized_payload.unwrap_or(record.raw_payload);
        let properties =
            if payload.is_object() { payload } else { serde_json::json!({ "value": payload }) };
        let input = crate::ontology_client::CreateObjectRequest {
            object_type_id,
            properties,
            source_lineage: serde_json::json!([id]),
        };
        match client.create_object(&principal.query_token, &input).await {
            Ok(()) => modeled.push(id),
            Err(_) => failed.push(id),
        }
    }
    Ok(ModelDataApiResponse { modeled, failed })
}

pub async fn model_data_records_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ModelDataApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match model_data_records(&state, &principal, request.object_type_id, request.ids).await {
        Ok(result) => axum::Json(result).into_response(),
        Err(error) => error,
    }
}

pub async fn model_data_record_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<ModelDataApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let ids = if request.ids.is_empty() { vec![id] } else { request.ids };
    if ids.iter().any(|value| *value != id) {
        return api_error(StatusCode::BAD_REQUEST, "path id must match every request id");
    }
    match model_data_records(&state, &principal, request.object_type_id, ids).await {
        Ok(result) => axum::Json(result).into_response(),
        Err(error) => error,
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkApiQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub assigned_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkApiResponse {
    pub incidents: Vec<IncidentDetail>,
    pub action_invocations: Vec<common::ontology::ActionInvocation>,
    pub reviews: Vec<common::ontology::ActionReview>,
}

fn work_incident_matches(incident: &IncidentDetail, query: &WorkApiQuery) -> bool {
    let text = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let matches_text = text.is_empty()
        || incident.incident.title.to_ascii_lowercase().contains(&text)
        || incident.incident.summary.to_ascii_lowercase().contains(&text)
        || incident.incident.id.to_string().contains(&text);
    let matches_status = query
        .status
        .as_deref()
        .map(|status| incident.incident.status.to_string().eq_ignore_ascii_case(status))
        .unwrap_or(true);
    let matches_severity = query
        .severity
        .as_deref()
        .map(|severity| incident.incident.severity.to_string().eq_ignore_ascii_case(severity))
        .unwrap_or(true);
    let matches_owner = query
        .assigned_to
        .as_deref()
        .map_or(true, |owner| incident.incident.assigned_to.as_deref() == Some(owner));
    matches_text && matches_status && matches_severity && matches_owner
}

pub async fn get_work_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let incidents = match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(items) => {
            items.into_iter().filter(|incident| work_incident_matches(incident, &query)).collect()
        }
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut action_invocations = match client.list_action_invocations(&principal.query_token).await
    {
        Ok(items) => items,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut reviews = match client.list_action_reviews(&principal.query_token).await {
        Ok(items) => items,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let text = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    if !text.is_empty() {
        action_invocations.retain(|invocation| {
            format!(
                "{} {} {} {} {}",
                invocation.id,
                invocation.action_type_id,
                invocation.outcome,
                invocation.target_object_ids,
                invocation.parameters
            )
            .to_ascii_lowercase()
            .contains(&text)
        });
        let invocation_ids =
            action_invocations.iter().map(|item| item.id).collect::<std::collections::HashSet<_>>();
        reviews.retain(|review| invocation_ids.contains(&review.invocation_id));
    }
    axum::Json(WorkApiResponse { incidents, action_invocations, reviews }).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ClaimWorkApiRequest {
    pub incident_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ClaimWorkApiResponse {
    pub claimed: Vec<Uuid>,
    pub failed: Vec<Uuid>,
}

pub async fn claim_work_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ClaimWorkApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.incident_ids.is_empty() || request.incident_ids.len() > 100 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "incident_ids must contain between 1 and 100 cases",
        );
    }
    let mut requested_ids = request.incident_ids;
    requested_ids.sort_unstable();
    requested_ids.dedup();
    let mut claimed = Vec::new();
    let mut failed = Vec::new();
    for id in requested_ids {
        let incident = match state.incidents_client.get_incident(principal.tenant_id, id).await {
            Ok(Some(item)) => item.incident,
            _ => {
                failed.push(id);
                continue;
            }
        };
        if incident.assigned_to.is_some() || matches!(incident.status, IncidentStatus::Resolved) {
            failed.push(id);
            continue;
        }
        let updated = common::Incident {
            assigned_to: Some(principal.username.clone()),
            updated_at: chrono::Utc::now(),
            ..incident
        };
        match state
            .incidents_client
            .update_incident(principal.role, &principal.username, updated)
            .await
        {
            Ok(_) => claimed.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(ClaimWorkApiResponse { claimed, failed }).into_response()
}

pub async fn export_work_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let incidents = match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(items) => items,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut csv = String::from("queue,id,title,severity,status,executed_at,source\n");
    for incident in incidents {
        if incident.incident.status == IncidentStatus::Resolved
            || !work_incident_matches(&incident, &query)
        {
            continue;
        }
        let queue = if incident.incident.assigned_to.as_deref() == Some(principal.username.as_str())
        {
            "assigned"
        } else if incident.incident.assigned_to.is_none() {
            "unassigned"
        } else {
            "other"
        };
        csv.push_str(&format!(
            "{},{},{},{},{},{},incident\n",
            queue,
            incident.incident.id,
            api_csv_escape(&incident.incident.title),
            incident.incident.severity,
            incident.incident.status,
            ""
        ));
    }
    if let Some(client) = crate::ontology_client::global() {
        let (types, invocations) = tokio::join!(
            client.list_action_types(&principal.query_token),
            client.list_action_invocations(&principal.query_token),
        );
        let names = types
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.id, item.name))
            .collect::<std::collections::HashMap<_, _>>();
        for invocation in invocations
            .unwrap_or_default()
            .into_iter()
            .filter(|item| !item.outcome.eq_ignore_ascii_case("completed"))
        {
            let name = names
                .get(&invocation.action_type_id)
                .map(String::as_str)
                .unwrap_or("Unknown governed action");
            let q = query.q.as_deref().unwrap_or_default().to_ascii_lowercase();
            if !q.is_empty()
                && !format!("{name} {}", invocation.outcome).to_ascii_lowercase().contains(&q)
            {
                continue;
            }
            csv.push_str(&format!(
                "review,{},{},{},{},{},action\n",
                invocation.id,
                api_csv_escape(name),
                "",
                api_csv_escape(&invocation.outcome),
                invocation.executed_at.to_rfc3339()
            ));
        }
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"work-{}.csv\"", principal.tenant_id).parse().unwrap(),
    );
    (response_headers, csv).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct OntologyObjectsApiQuery {
    pub type_id: Option<Uuid>,
    pub q: Option<String>,
    pub property: Option<String>,
    pub value: Option<String>,
}

pub type OntologyExportApiQuery = OntologyObjectsApiQuery;

fn ontology_object_api_matches(
    object: &common::ontology::Object,
    query: &OntologyObjectsApiQuery,
) -> bool {
    let text = query.q.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let property = query.property.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let value = query.value.as_deref().unwrap_or_default().trim().to_ascii_lowercase();
    let text_matches = text.is_empty()
        || object.id.to_string().contains(&text)
        || object.object_type_id.to_string().contains(&text)
        || object.properties.to_string().to_ascii_lowercase().contains(&text);
    if !text_matches {
        return false;
    }
    if property.is_empty() && value.is_empty() {
        return true;
    }
    object.properties.as_object().is_some_and(|properties| {
        properties.iter().any(|(key, item)| {
            (property.is_empty() || key.to_ascii_lowercase().contains(&property))
                && (value.is_empty()
                    || item.to_string().to_ascii_lowercase().trim_matches('"').contains(&value))
        })
    })
}

pub async fn export_ontology_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyExportApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let client = match ontology_client() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (types, objects) = tokio::join!(
        client.list_object_types(&principal.query_token),
        client.list_objects(&principal.query_token, query.type_id),
    );
    let types = match types {
        Ok(types) => types,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let objects = match objects {
        Ok(objects) => objects,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let type_names = types
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let mut matching = objects
        .into_iter()
        .filter(|object| ontology_object_api_matches(object, &query))
        .collect::<Vec<_>>();
    let has_more = matching.len() > 5_000;
    matching.truncate(5_000);
    let mut csv = String::from(
        "object_id,object_type_id,object_type,created_at,updated_at,properties,source_lineage\n",
    );
    for object in matching {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            object.id,
            object.object_type_id,
            api_csv_escape(
                type_names
                    .get(&object.object_type_id)
                    .map(String::as_str)
                    .unwrap_or("Unknown type")
            ),
            object.created_at.to_rfc3339(),
            object.updated_at.to_rfc3339(),
            api_csv_escape(&object.properties.to_string()),
            api_csv_escape(&object.source_lineage.to_string()),
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"ontology-export-{}.csv\"", principal.tenant_id)
            .parse()
            .unwrap(),
    );
    if has_more {
        response_headers.insert("x-export-truncated", "true".parse().unwrap());
    }
    (response_headers, csv).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ActionTemplateApiRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub action_type: common::ActionType,
    pub config: serde_json::Value,
}

fn action_template_from_request(
    request: ActionTemplateApiRequest,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<common::ActionTemplate, Response> {
    let name = request.name.trim().to_string();
    if name.is_empty() || name.len() > 160 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "name must be between 1 and 160 characters",
        ));
    }
    if request.description.len() > 2000 || !request.config.is_object() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "description is too long or config is not a JSON object",
        ));
    }
    let now = chrono::Utc::now();
    Ok(common::ActionTemplate {
        id,
        tenant_id,
        name,
        description: request.description,
        action_type: request.action_type,
        config: request.config,
        version: 1,
        created_at: now,
        updated_at: now,
    })
}

pub async fn list_action_templates_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(client) = crate::action_templates_client::global() else {
        return api_error(StatusCode::BAD_GATEWAY, "action template service is not configured");
    };
    match client.list(principal.tenant_id).await {
        Ok(templates) => axum::Json(templates).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn create_action_template_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ActionTemplateApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let template = match action_template_from_request(request, principal.tenant_id, Uuid::new_v4())
    {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(client) = crate::action_templates_client::global() else {
        return api_error(StatusCode::BAD_GATEWAY, "action template service is not configured");
    };
    match client.create(principal.role, &principal.username, template).await {
        Ok(template) => (StatusCode::CREATED, axum::Json(template)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn update_action_template_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<ActionTemplateApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let template = match action_template_from_request(request, principal.tenant_id, id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(client) = crate::action_templates_client::global() else {
        return api_error(StatusCode::BAD_GATEWAY, "action template service is not configured");
    };
    match client.update(principal.role, &principal.username, template).await {
        Ok(template) => axum::Json(template).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_action_template_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    let Some(client) = crate::action_templates_client::global() else {
        return api_error(StatusCode::BAD_GATEWAY, "action template service is not configured");
    };
    match client.delete(principal.role, &principal.username, principal.tenant_id, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_api_keys_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.api_keys_client.list_api_keys(principal.tenant_id).await {
        Ok(keys) => axum::Json(keys).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyApiRequest {
    pub label: String,
}

fn validate_api_key_label(label: &str) -> Result<&str, Response> {
    let label = label.trim();
    if label.is_empty() || label.len() > 120 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "label must be between 1 and 120 characters",
        ));
    }
    Ok(label)
}

pub async fn create_api_key_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateApiKeyApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    let label = match validate_api_key_label(&request.label) {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state
        .api_keys_client
        .create_api_key(principal.tenant_id, principal.role, label, &principal.username)
        .await
    {
        Ok(api_key) => (
            StatusCode::CREATED,
            axum::Json(serde_json::json!({"label": label, "api_key": api_key})),
        )
            .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn revoke_api_key_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    match state
        .api_keys_client
        .revoke_api_key(principal.tenant_id, principal.role, id, &principal.username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkRevokeApiKeysApiRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct BulkRevokeApiKeysApiResponse {
    pub revoked: Vec<Uuid>,
    pub failed: Vec<Uuid>,
}

pub async fn bulk_revoke_api_keys_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkRevokeApiKeysApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_operator(&principal) {
        return e;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 API keys");
    }
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let tenant_keys = match state.api_keys_client.list_api_keys(principal.tenant_id).await {
        Ok(keys) => keys.into_iter().map(|key| key.id).collect::<std::collections::HashSet<_>>(),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut revoked = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        if !tenant_keys.contains(&id) {
            failed.push(id);
            continue;
        }
        match state
            .api_keys_client
            .revoke_api_key(principal.tenant_id, principal.role, id, &principal.username)
            .await
        {
            Ok(()) => revoked.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(BulkRevokeApiKeysApiResponse { revoked, failed }).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct AuditLogApiQuery {
    pub limit: Option<u32>,
    pub before: Option<DateTime<Utc>>,
    pub q: Option<String>,
    pub service: Option<String>,
    pub change_type: Option<String>,
    pub date: Option<String>,
}

const AUDIT_API_FILTER_MAX_PAGES: usize = 10;

#[derive(Debug, Serialize)]
struct AuditLogApiEntry {
    service: &'static str,
    #[serde(flatten)]
    entry: crate::audit_log_client::AuditLogEntry,
}

#[derive(Debug, Serialize)]
struct AuditLogApiResponse {
    entries: Vec<AuditLogApiEntry>,
    has_more: bool,
    limit: u32,
    next_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct EntityAuditLogApiResponse {
    service: String,
    entity_id: Uuid,
    entries: Vec<crate::audit_log_client::AuditLogEntry>,
}

async fn entity_audit_entries(
    state: &AppState,
    service: &str,
    tenant_id: Uuid,
    entity_id: Uuid,
    query_token: &str,
) -> Result<Vec<crate::audit_log_client::AuditLogEntry>, String> {
    match service {
        "config" => state
            .config_audit_log_client
            .list_for_entity(tenant_id, entity_id)
            .await
            .map_err(|error| error.to_string()),
        "retention" => state
            .retention_audit_log_client
            .list_for_entity(tenant_id, entity_id)
            .await
            .map_err(|error| error.to_string()),
        "auth" => state
            .auth_audit_log_client
            .list_for_entity(tenant_id, entity_id)
            .await
            .map_err(|error| error.to_string()),
        "ingestion" => state
            .ingestion_audit_log_client
            .list_for_entity(tenant_id, entity_id)
            .await
            .map_err(|error| error.to_string()),
        "egress" => state
            .egress_audit_log_client
            .list_for_entity(tenant_id, entity_id)
            .await
            .map_err(|error| error.to_string()),
        "incident" => state
            .incidents_client
            .list_audit_log_for_entity(tenant_id, entity_id)
            .await
            .map_err(|error| error.to_string()),
        "ontology" => {
            let Some(client) = crate::ontology_client::global() else {
                return Err("ontology audit service is unavailable".to_string());
            };
            let invocations = client
                .list_action_invocations(query_token)
                .await
                .map_err(|error| error.to_string())?;
            Ok(invocations
                .into_iter()
                .filter(|invocation| {
                    invocation.target_object_ids.as_array().into_iter().flatten().any(|target| {
                        target.as_str().and_then(|value| value.parse::<Uuid>().ok())
                            == Some(entity_id)
                    })
                })
                .map(|invocation| crate::audit_log_client::AuditLogEntry {
                    id: invocation.id,
                    entity_type: "ontology_object".to_string(),
                    entity_id,
                    change_type: "invoked".to_string(),
                    actor: invocation
                        .triggering_event_ref
                        .get("actor")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("system")
                        .to_string(),
                    before: None,
                    after: serde_json::json!({
                        "action_type_id": invocation.action_type_id,
                        "target_object_ids": invocation.target_object_ids,
                        "parameters": invocation.parameters,
                        "outcome": invocation.outcome,
                        "triggering_event_ref": invocation.triggering_event_ref,
                    }),
                    changed_at: invocation.executed_at,
                })
                .collect())
        }
        _ => Err("unknown audit log service".to_string()),
    }
}

/// Returns the immutable, tenant-scoped history for one entity. This mirrors the browser's
/// `/audit-log/:service/:entity_id` page for API consumers such as operator shells and reports.
pub async fn get_entity_audit_log_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((service, entity_id)): Path<(String, Uuid)>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !matches!(
        service.as_str(),
        "config" | "retention" | "auth" | "ingestion" | "egress" | "incident" | "ontology"
    ) {
        return api_error(StatusCode::BAD_REQUEST, "unknown audit log service");
    }
    match entity_audit_entries(
        &state,
        &service,
        principal.tenant_id,
        entity_id,
        &principal.query_token,
    )
    .await
    {
        Ok(entries) => {
            axum::Json(EntityAuditLogApiResponse { service, entity_id, entries }).into_response()
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error),
    }
}

/// Returns the tenant-scoped, most-recent-first control-plane audit feed. Each backend is
/// queried with the same cursor and the merged result is bounded after sorting, so callers get
/// a stable page without receiving an unbounded cross-service response.
pub async fn list_audit_log_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let filter_query = AuditLogExportApiQuery {
        before: query.before,
        q: query.q.clone(),
        service: query.service.clone(),
        change_type: query.change_type.clone(),
        date: query.date.clone(),
    };
    let filtering = query.q.as_deref().is_some_and(|value| !value.trim().is_empty())
        || query.service.as_deref().is_some_and(|value| !value.trim().is_empty())
        || query.change_type.as_deref().is_some_and(|value| !value.trim().is_empty())
        || query.date.as_deref().is_some_and(|value| !value.trim().is_empty());
    let page_budget = if filtering { AUDIT_API_FILTER_MAX_PAGES } else { 1 };
    let mut before = query.before;
    let mut filtered_entries = Vec::with_capacity(limit as usize);
    let mut errors = Vec::new();
    let mut exhausted = false;
    for _ in 0..page_budget {
        let (entries, page_errors) = crate::recent_audit_log_handler::fetch_merged_page(
            &state,
            principal.tenant_id,
            &principal.query_token,
            before,
            // Fetch one sentinel row per source so `has_more` remains meaningful even when a
            // single backend has exactly `limit` rows available in this page.
            limit.saturating_add(1),
        )
        .await;
        errors.extend(page_errors);
        if entries.is_empty() {
            exhausted = true;
            break;
        }
        before = entries.last().map(|(_, entry)| entry.changed_at);
        for (service, entry) in entries {
            match audit_export_matches(service, &entry, &filter_query) {
                Ok(true) => filtered_entries.push((service, entry)),
                Ok(false) => {}
                Err(error) => return error,
            }
        }
        if filtered_entries.len() > limit as usize {
            break;
        }
    }
    if filtered_entries.is_empty() && !errors.is_empty() {
        return api_error(StatusCode::BAD_GATEWAY, errors.join("; "));
    }
    filtered_entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.changed_at));
    let has_more = filtered_entries.len() > limit as usize;
    let mut entries = filtered_entries;
    entries.truncate(limit as usize);
    let next_before = if has_more {
        entries.last().map(|(_, entry)| entry.changed_at)
    } else if !exhausted {
        before
    } else {
        None
    };
    axum::Json(AuditLogApiResponse {
        entries: entries
            .into_iter()
            .map(|(service, entry)| AuditLogApiEntry { service, entry })
            .collect(),
        has_more,
        limit,
        next_before,
    })
    .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct AuditLogExportApiQuery {
    pub before: Option<DateTime<Utc>>,
    pub q: Option<String>,
    pub service: Option<String>,
    pub change_type: Option<String>,
    pub date: Option<String>,
}

fn audit_export_matches(
    service: &str,
    entry: &crate::audit_log_client::AuditLogEntry,
    query: &AuditLogExportApiQuery,
) -> Result<bool, Response> {
    let date = query.date.as_deref().unwrap_or("").trim();
    if !date.is_empty() && chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        return Err(api_error(StatusCode::BAD_REQUEST, "audit date must be YYYY-MM-DD"));
    }
    let q = query.q.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let service_filter = query.service.as_deref().unwrap_or("").trim();
    let change_filter = query.change_type.as_deref().unwrap_or("").trim();
    let searchable = format!(
        "{} {} {} {} {} {} {}",
        service,
        entry.entity_type,
        entry.change_type,
        entry.actor,
        entry.id,
        entry.entity_id,
        entry.after
    )
    .to_ascii_lowercase();
    Ok((q.is_empty() || searchable.contains(&q))
        && (service_filter.is_empty() || service.eq_ignore_ascii_case(service_filter))
        && (change_filter.is_empty() || entry.change_type.eq_ignore_ascii_case(change_filter))
        && (date.is_empty() || entry.changed_at.format("%Y-%m-%d").to_string() == date))
}

pub async fn export_audit_log_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogExportApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let mut before = query.before;
    let mut rows = Vec::new();
    let mut exhausted = false;
    for _ in 0..10 {
        let (page, errors) = crate::recent_audit_log_handler::fetch_merged_page(
            &state,
            principal.tenant_id,
            &principal.query_token,
            before,
            200,
        )
        .await;
        if page.is_empty() {
            if !errors.is_empty() && rows.is_empty() {
                return api_error(StatusCode::BAD_GATEWAY, errors.join("; "));
            }
            exhausted = true;
            break;
        }
        before = page.last().map(|(_, entry)| entry.changed_at);
        for (service, entry) in page {
            match audit_export_matches(service, &entry, &query) {
                Ok(true) => rows.push((service, entry)),
                Ok(false) => {}
                Err(error) => return error,
            }
        }
    }
    rows.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.changed_at));
    let mut csv = String::from("changed_at,service,entity_type,change_type,actor,entity_id\n");
    for (service, entry) in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            entry.changed_at.to_rfc3339(),
            api_csv_escape(service),
            api_csv_escape(&entry.entity_type),
            api_csv_escape(&entry.change_type),
            api_csv_escape(&entry.actor),
            entry.entity_id,
        ));
    }
    let mut response = (StatusCode::OK, csv).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_static("attachment; filename=audit-log.csv"),
    );
    if !exhausted {
        if let Some(next_before) = before {
            if let Ok(value) = next_before.to_rfc3339().parse() {
                response
                    .headers_mut()
                    .insert(axum::http::HeaderName::from_static("x-next-before"), value);
            }
        }
    }
    response
}

/// Returns platform dependency health through the authenticated versioned API. Health itself
/// is platform-wide; authentication still prevents exposing deployment topology anonymously.
pub async fn get_health_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(error) = principal(&state, &headers).await {
        return error;
    }
    match state.health_client.platform_health().await {
        Ok(summary) => axum::Json(summary).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
pub struct OverviewApiCounts {
    pub records: i64,
    pub events: usize,
    pub open_incidents: usize,
    pub active_triggers: usize,
    pub object_types: usize,
    pub objects: usize,
    pub actions: usize,
    pub completed_actions: usize,
}

#[derive(Debug, Serialize)]
pub struct OverviewApiResponse {
    pub context: crate::session_context_handler::SessionContext,
    pub attention: crate::attention_summary_handler::AttentionSummary,
    pub health: crate::health_client::PlatformHealthSummary,
    pub counts: OverviewApiCounts,
    pub errors: Vec<String>,
}

/// Returns the compact command-center read model used by external operator shells. The domain
/// endpoints remain available for drill-down; this endpoint provides one authenticated request
/// for the shell's initial posture without forcing clients to reconstruct the dashboard's KPI
/// joins themselves.
pub async fn get_overview_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (stats, events, incidents, triggers, health) = tokio::join!(
        state.stats_client.connector_stats(principal.tenant_id),
        state.events_client.list_events(&principal.query_token, 1000, 0, None, None),
        state.incidents_client.list_incidents(principal.tenant_id, None),
        state.triggers_client.list_triggers(principal.tenant_id, 1000, 0),
        state.health_client.platform_health(),
    );
    let mut errors = Vec::new();
    let records = match stats {
        Ok(value) => value.iter().map(|item| item.record_count).sum(),
        Err(error) => {
            errors.push(format!("records: {error}"));
            0
        }
    };
    let events = match events {
        Ok(page) => page.events.len(),
        Err(error) => {
            errors.push(format!("events: {error}"));
            0
        }
    };
    let open_incidents = match incidents {
        Ok(value) => value
            .iter()
            .filter(|item| item.incident.status != common::IncidentStatus::Resolved)
            .count(),
        Err(error) => {
            errors.push(format!("incidents: {error}"));
            0
        }
    };
    let active_triggers = match triggers {
        Ok(page) => page.triggers.iter().filter(|item| item.enabled).count(),
        Err(error) => {
            errors.push(format!("triggers: {error}"));
            0
        }
    };
    let (object_types, objects, actions) = match crate::ontology_client::global() {
        Some(client) => tokio::join!(
            async {
                client.list_object_types(&principal.query_token).await.map(|items| items.len())
            },
            async {
                client.list_objects(&principal.query_token, None).await.map(|items| items.len())
            },
            async {
                client.list_action_invocations(&principal.query_token).await.map(|items| {
                    let completed = items
                        .iter()
                        .filter(|item| item.outcome.eq_ignore_ascii_case("completed"))
                        .count();
                    (items.len(), completed)
                })
            },
        ),
        None => (
            Err(crate::ontology_client::OntologyClientError::Unreachable(
                "client unavailable".to_string(),
            )),
            Err(crate::ontology_client::OntologyClientError::Unreachable(
                "client unavailable".to_string(),
            )),
            Err(crate::ontology_client::OntologyClientError::Unreachable(
                "client unavailable".to_string(),
            )),
        ),
    };
    let object_types = object_types.unwrap_or_else(|error| {
        errors.push(format!("ontology object types: {error}"));
        0
    });
    let objects = objects.unwrap_or_else(|error| {
        errors.push(format!("ontology objects: {error}"));
        0
    });
    let (actions, completed_actions) = actions.unwrap_or_else(|error| {
        errors.push(format!("ontology actions: {error}"));
        (0, 0)
    });
    let health = match health {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let context = crate::session_context_handler::SessionContext {
        username: principal.username,
        role: principal.role,
        tenant_id: principal.tenant_id,
        workspace: crate::session_context_handler::workspace_from_cookie(
            &headers,
            principal.tenant_id,
        ),
    };
    let attention =
        attention_summary_for_tenant(&state, principal.tenant_id, &principal.query_token).await;
    axum::Json(OverviewApiResponse {
        context,
        attention,
        health,
        counts: OverviewApiCounts {
            records,
            events,
            open_incidents,
            active_triggers,
            object_types,
            objects,
            actions,
            completed_actions,
        },
        errors,
    })
    .into_response()
}

#[derive(Debug, Serialize)]
pub struct PipelineApiConnector {
    pub id: Uuid,
    pub name: String,
    pub connector_type: String,
    pub enabled: bool,
    pub record_count: i64,
    pub last_ingested_at: Option<DateTime<Utc>>,
    pub health: String,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiStage {
    pub service: String,
    pub label: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiQueue {
    pub stage: String,
    pub label: String,
    pub queue_name: String,
    pub messages: u64,
    pub severity: String,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiGraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub href: String,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiGraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
    pub queue_name: Option<String>,
    pub messages: Option<u64>,
    pub severity: String,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiGraph {
    pub nodes: Vec<PipelineApiGraphNode>,
    pub edges: Vec<PipelineApiGraphEdge>,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiCounts {
    pub records: i64,
    pub events: usize,
    pub open_incidents: usize,
    pub active_triggers: usize,
}

#[derive(Debug, Serialize)]
pub struct PipelineApiResponse {
    pub health: crate::health_client::PlatformHealthSummary,
    pub connectors: Vec<PipelineApiConnector>,
    pub stages: Vec<PipelineApiStage>,
    pub queues: Vec<PipelineApiQueue>,
    pub graph: PipelineApiGraph,
    pub counts: PipelineApiCounts,
}

/// Returns the authenticated Pipeline Map read model. Connector configuration is deliberately
/// omitted; this exposes only operational posture and counts needed to render topology or drive
/// an external operator console.
pub async fn get_pipeline_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (sensors, stats, events, incidents, triggers, health, backlog) = tokio::join!(
        state.sensors_client.list_sensors(principal.tenant_id, 1000, 0),
        state.stats_client.connector_stats(principal.tenant_id),
        state.events_client.list_events(&principal.query_token, 1000, 0, None, None),
        state.incidents_client.list_incidents(principal.tenant_id, None),
        state.triggers_client.list_triggers(principal.tenant_id, 1000, 0),
        state.health_client.platform_health(),
        state.backlog_client.queue_depths(),
    );
    let sensors = match sensors {
        Ok(page) => page.sensors,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let stats = match stats {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let events = match events {
        Ok(page) => page.events,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let incidents = match incidents {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let triggers = match triggers {
        Ok(page) => page.triggers,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let health = match health {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let backlog = match backlog {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let stats_by_connector =
        stats.iter().map(|item| (item.connector_id.as_str(), item)).collect::<HashMap<_, _>>();
    let connectors: Vec<PipelineApiConnector> = sensors
        .into_iter()
        .map(|sensor| {
            let stat = stats_by_connector.get(sensor.name.as_str()).copied();
            let last_ingested_at = stat.map(|item| item.last_ingested_at);
            let health = if !sensor.enabled {
                "disabled"
            } else if let Some(last) = last_ingested_at {
                if Utc::now() - last <= chrono::Duration::hours(1) {
                    "healthy"
                } else {
                    "stale"
                }
            } else {
                "no_data"
            };
            PipelineApiConnector {
                id: sensor.id,
                name: sensor.name,
                connector_type: sensor.connector_type,
                enabled: sensor.enabled,
                record_count: stat.map(|item| item.record_count).unwrap_or(0),
                last_ingested_at,
                health: health.to_string(),
            }
        })
        .collect();
    let stages: Vec<PipelineApiStage> = crate::topology::PIPELINE_STAGES
        .iter()
        .map(|(service, label)| PipelineApiStage {
            service: (*service).to_string(),
            label: (*label).to_string(),
            status: health
                .services
                .iter()
                .find(|item| item.name == *service)
                .map(|item| item.status.clone())
                .unwrap_or_else(|| "unknown".to_string()),
        })
        .collect();
    let queues: Vec<PipelineApiQueue> = crate::topology::PIPELINE_EDGES
        .iter()
        .map(|(stage, label)| {
            let depth = backlog.iter().find(|item| item.stage == *stage);
            let messages = depth.map(|item| item.messages).unwrap_or(0);
            PipelineApiQueue {
                stage: (*stage).to_string(),
                label: (*label).to_string(),
                queue_name: depth
                    .map(|item| item.queue_name.clone())
                    .unwrap_or_else(|| (*stage).to_string()),
                messages,
                severity: crate::topology::severity_for(messages).to_string(),
            }
        })
        .collect();
    let mut graph_nodes = connectors
        .iter()
        .map(|connector| PipelineApiGraphNode {
            id: format!("connector:{}", connector.id),
            kind: "connector".to_string(),
            label: connector.name.clone(),
            status: connector.health.clone(),
            href: format!("/sensors/{}", connector.id),
        })
        .collect::<Vec<_>>();
    graph_nodes.extend(stages.iter().map(|stage| PipelineApiGraphNode {
        id: format!("service:{}", stage.service),
        kind: "service".to_string(),
        label: stage.label.clone(),
        status: stage.status.clone(),
        href: match stage.service.as_str() {
            "ingestion-service" => "/sensors".to_string(),
            "normalization-service" => "/normalization-mappings".to_string(),
            "analysis-service" => "/analysis-config".to_string(),
            "trigger-engine" => "/triggers".to_string(),
            "action-executor" => "/actions".to_string(),
            _ => "/pipeline".to_string(),
        },
    }));
    graph_nodes.extend([
        PipelineApiGraphNode {
            id: "signals".to_string(),
            kind: "signal-aggregation".to_string(),
            label: "Signal aggregation".to_string(),
            status: "observed".to_string(),
            href: "/events".to_string(),
        },
        PipelineApiGraphNode {
            id: "response".to_string(),
            kind: "governed-response".to_string(),
            label: "Governed response".to_string(),
            status: "observed".to_string(),
            href: "/actions".to_string(),
        },
    ]);
    let mut graph_edges = connectors
        .iter()
        .map(|connector| PipelineApiGraphEdge {
            id: format!("connector-to-ingestion:{}", connector.id),
            source: format!("connector:{}", connector.id),
            target: "service:ingestion-service".to_string(),
            label: "source intake".to_string(),
            queue_name: None,
            messages: None,
            severity: match connector.health.as_str() {
                "healthy" => "ok",
                "stale" => "warn",
                _ => "unknown",
            }
            .to_string(),
        })
        .collect::<Vec<_>>();
    for (index, queue) in queues.iter().enumerate() {
        // The final `event.created` boundary is represented by the signal-aggregation node
        // before it reaches Action Executor. This keeps the rendered graph ordered left-to-right
        // even though the queue itself is consumed by the executor service.
        let (source, target) = if index + 1 < queues.len() {
            (
                format!("service:{}", stages[index].service),
                format!("service:{}", stages[index + 1].service),
            )
        } else {
            (format!("service:{}", stages[index].service), "signals".to_string())
        };
        graph_edges.push(PipelineApiGraphEdge {
            id: format!("queue:{}", queue.stage),
            source,
            target,
            label: queue.label.clone(),
            queue_name: Some(queue.queue_name.clone()),
            messages: Some(queue.messages),
            severity: queue.severity.clone(),
        });
    }
    graph_edges.push(PipelineApiGraphEdge {
        id: "signals-to-action".to_string(),
        source: "signals".to_string(),
        target: "service:action-executor".to_string(),
        label: "governed action dispatch".to_string(),
        queue_name: None,
        messages: None,
        severity: "ok".to_string(),
    });
    graph_edges.push(PipelineApiGraphEdge {
        id: "action-to-response".to_string(),
        source: "service:action-executor".to_string(),
        target: "response".to_string(),
        label: "response recorded".to_string(),
        queue_name: None,
        messages: None,
        severity: "ok".to_string(),
    });
    axum::Json(PipelineApiResponse {
        health,
        connectors,
        stages,
        queues,
        graph: PipelineApiGraph { nodes: graph_nodes, edges: graph_edges },
        counts: PipelineApiCounts {
            records: stats.iter().map(|item| item.record_count).sum(),
            events: events.len(),
            open_incidents: incidents
                .iter()
                .filter(|item| item.incident.status != common::IncidentStatus::Resolved)
                .count(),
            active_triggers: triggers.iter().filter(|item| item.enabled).count(),
        },
    })
    .into_response()
}

/// Returns the same command-center pressure summary used by `/work/summary`, for API clients
/// and service accounts that need to drive an external operator console.
pub async fn get_attention_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let summary =
        attention_summary_for_tenant(&state, principal.tenant_id, &principal.query_token).await;
    axum::Json(summary).into_response()
}

/// Returns the tenant-scoped security posture aggregation used by the Security overview page.
/// The underlying user listing keeps its existing role boundary, so API consumers see the same
/// partial/error posture as an authenticated browser user rather than a privileged snapshot.
pub async fn get_security_overview_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let summary = security_overview_for_tenant(&state, principal.tenant_id, principal.role).await;
    axum::Json(summary).into_response()
}

#[derive(Debug, Serialize)]
struct ConfigurationDomainApi {
    key: &'static str,
    label: &'static str,
    total: usize,
    healthy: usize,
    attention: usize,
    state: &'static str,
    href: &'static str,
}

/// Returns the unified Configuration Center read model: each control domain carries its live
/// count, readiness posture, and Console handoff so an external control plane can present the
/// same Connect → Normalize → Understand → Model → Detect → Respond operating picture.
pub async fn get_configuration_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let tenant_id = principal.tenant_id;
    let (sensors, sensor_stats, triggers, mappings, retention, egress, analysis) = tokio::join!(
        state.sensors_client.list_sensors(tenant_id, 1000, 0),
        state.stats_client.connector_stats(tenant_id),
        state.triggers_client.list_triggers(tenant_id, 1000, 0),
        state.normalization_mappings_client.list_mappings(tenant_id),
        state.retention_policies_client.list_policies(tenant_id),
        state.egress_allowlist_client.get_allowlist(tenant_id),
        state.analysis_config_client.get_analysis_config(tenant_id),
    );
    let mut errors = Vec::new();
    let sensors = match sensors {
        Ok(page) => page.sensors,
        Err(error) => {
            errors.push(format!("sensors: {error}"));
            Vec::new()
        }
    };
    let sensor_stats = match sensor_stats {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("connector health: {error}"));
            Vec::new()
        }
    };
    let triggers = match triggers {
        Ok(page) => page.triggers,
        Err(error) => {
            errors.push(format!("triggers: {error}"));
            Vec::new()
        }
    };
    let mappings = match mappings {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("field mappings: {error}"));
            Vec::new()
        }
    };
    let retention = match retention {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("retention: {error}"));
            Vec::new()
        }
    };
    let egress = match egress {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("egress: {error}"));
            Vec::new()
        }
    };
    let analysis_provider = match analysis {
        Ok(Some(config)) => match config.provider {
            common::AnalysisProvider::AzureFoundry => "azure_foundry",
            common::AnalysisProvider::OpenAiCompatible => "openai_compatible",
        },
        Ok(None) => "not_configured",
        Err(error) => {
            errors.push(format!("AI analysis: {error}"));
            "unavailable"
        }
    };
    let now = Utc::now();
    let enabled_sensors = sensors.iter().filter(|sensor| sensor.enabled).count();
    let stale_sensors = sensors
        .iter()
        .filter(|sensor| sensor.enabled)
        .filter(|sensor| {
            sensor_stats
                .iter()
                .find(|stat| stat.connector_id == sensor.name)
                .map(|stat| now - stat.last_ingested_at > chrono::Duration::hours(1))
                .unwrap_or(true)
        })
        .count();
    let enabled_triggers = triggers.iter().filter(|trigger| trigger.enabled).count();
    let enabled_retention = retention.iter().filter(|policy| policy.enabled).count();
    let (object_types, objects, action_contracts) = match crate::ontology_client::global() {
        Some(client) => tokio::join!(
            client.list_object_types(&principal.query_token),
            client.list_objects(&principal.query_token, None),
            client.list_action_types(&principal.query_token),
        ),
        None => {
            errors.push("ontology: client unavailable".to_string());
            (
                Err(crate::ontology_client::OntologyClientError::Unreachable(
                    "client unavailable".into(),
                )),
                Err(crate::ontology_client::OntologyClientError::Unreachable(
                    "client unavailable".into(),
                )),
                Err(crate::ontology_client::OntologyClientError::Unreachable(
                    "client unavailable".into(),
                )),
            )
        }
    };
    let object_type_count = match object_types {
        Ok(items) => items.len(),
        Err(error) => {
            errors.push(format!("ontology types: {error}"));
            0
        }
    };
    let object_count = match objects {
        Ok(items) => items.len(),
        Err(error) => {
            errors.push(format!("ontology objects: {error}"));
            0
        }
    };
    let action_count = match action_contracts {
        Ok(items) => items.len(),
        Err(error) => {
            errors.push(format!("ontology actions: {error}"));
            0
        }
    };
    let domains = vec![
        ConfigurationDomainApi {
            key: "connect",
            label: "Connectors",
            total: sensors.len(),
            healthy: enabled_sensors.saturating_sub(stale_sensors),
            attention: sensors.len().saturating_sub(enabled_sensors) + stale_sensors,
            state: if !sensors.is_empty() && enabled_sensors == sensors.len() && stale_sensors == 0
            {
                "good"
            } else {
                "attention"
            },
            href: "/sensors",
        },
        ConfigurationDomainApi {
            key: "normalize",
            label: "Normalization",
            total: mappings.len(),
            healthy: mappings.len(),
            attention: usize::from(mappings.is_empty()),
            state: if mappings.is_empty() { "attention" } else { "good" },
            href: "/normalization-mappings",
        },
        ConfigurationDomainApi {
            key: "understand",
            label: "AI analysis",
            total: 1,
            healthy: usize::from(matches!(
                analysis_provider,
                "azure_foundry" | "openai_compatible"
            )),
            attention: usize::from(!matches!(
                analysis_provider,
                "azure_foundry" | "openai_compatible"
            )),
            state: if matches!(analysis_provider, "azure_foundry" | "openai_compatible") {
                "good"
            } else {
                "attention"
            },
            href: "/analysis-config",
        },
        ConfigurationDomainApi {
            key: "model",
            label: "Ontology model",
            total: object_type_count + object_count,
            healthy: usize::from(object_type_count > 0 && object_count > 0),
            attention: usize::from(object_type_count == 0 || object_count == 0),
            state: if object_type_count > 0 && object_count > 0 { "good" } else { "attention" },
            href: "/ontology",
        },
        ConfigurationDomainApi {
            key: "detect",
            label: "Detection rules",
            total: triggers.len(),
            healthy: enabled_triggers,
            attention: triggers.len().saturating_sub(enabled_triggers),
            state: if enabled_triggers > 0 { "good" } else { "attention" },
            href: "/triggers",
        },
        ConfigurationDomainApi {
            key: "respond",
            label: "Action contracts",
            total: action_count,
            healthy: action_count,
            attention: usize::from(action_count == 0),
            state: if action_count > 0 { "good" } else { "attention" },
            href: "/actions/library",
        },
        ConfigurationDomainApi {
            key: "retention",
            label: "Retention",
            total: retention.len(),
            healthy: enabled_retention,
            attention: retention.len().saturating_sub(enabled_retention),
            state: if !retention.is_empty() && enabled_retention == retention.len() {
                "good"
            } else {
                "attention"
            },
            href: "/retention-policies",
        },
        ConfigurationDomainApi {
            key: "egress",
            label: "Egress boundary",
            total: egress.len(),
            healthy: egress.len(),
            attention: usize::from(egress.is_empty()),
            state: if egress.is_empty() { "attention" } else { "good" },
            href: "/egress-allowlist",
        },
    ];
    let healthy_domains = domains.iter().filter(|domain| domain.state == "good").count();
    axum::Json(serde_json::json!({
        "domains": domains,
        "healthy_domains": healthy_domains,
        "attention_domains": domains.len().saturating_sub(healthy_domains),
        "analysis_provider": analysis_provider,
        "errors": errors,
    }))
    .into_response()
}

/// Returns the same authoritative RBAC reference matrix rendered by the Security permissions
/// page. This is descriptive policy data, so it is available to every authenticated tenant
/// member and does not grant the permissions it describes.
pub async fn get_permissions_reference_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<crate::permissions_reference_handler::PermissionsQuery>,
) -> Response {
    if let Err(error) = principal(&state, &headers).await {
        return error;
    }
    let rows = crate::permissions_reference_handler::permission_rows();
    let viewer_allowed_count = rows.iter().filter(|row| row.viewer_state != "deny").count();
    let operator_allowed_count = rows.iter().filter(|row| row.operator_state != "deny").count();
    let admin_allowed_count = rows.iter().filter(|row| row.admin_state != "deny").count();
    axum::Json(serde_json::json!({
        "active_role": crate::permissions_reference_handler::normalize_role(&query.role),
        "rows": rows,
        "viewer_allowed_count": viewer_allowed_count,
        "operator_allowed_count": operator_allowed_count,
        "admin_allowed_count": admin_allowed_count,
    }))
    .into_response()
}

/// Returns the active identity and workspace context for non-browser shells. Browser sessions
/// and service-account requests use the same tenant/role projection; service accounts fall back
/// to the tenant UUID when no workspace cookie exists.
pub async fn get_session_context_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let workspace = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == crate::WORKSPACE_COOKIE_NAME).then(|| value.to_string())
            })
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("tenant-{}", principal.tenant_id));
    axum::Json(serde_json::json!({
        "username": principal.username,
        "role": principal.role,
        "tenant_id": principal.tenant_id,
        "workspace": workspace,
    }))
    .into_response()
}

#[derive(Debug, Serialize)]
struct ConnectorCatalogApiEntry {
    connector_type: &'static str,
    display_name: &'static str,
    category: &'static str,
    description: &'static str,
    fields: Vec<ConnectorCatalogFieldApi>,
}

#[derive(Debug, Serialize)]
struct ConnectorCatalogFieldApi {
    env_var: &'static str,
    label: &'static str,
    secret: bool,
    optional: bool,
}

/// Lists the connector marketplace contract used by the script generator, including the exact
/// environment fields each external agent must provide.
pub async fn list_sensor_catalog_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = principal(&state, &headers).await {
        return error;
    }
    let catalog = CONNECTOR_CATALOG
        .iter()
        .map(|(connector_type, display_name, category, description)| ConnectorCatalogApiEntry {
            connector_type,
            display_name,
            category,
            description,
            fields: fields_for(connector_type)
                .into_iter()
                .map(|field| ConnectorCatalogFieldApi {
                    env_var: field.env_var,
                    label: field.label,
                    secret: field.secret,
                    optional: field.optional,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    axum::Json(serde_json::json!({"connectors": catalog})).into_response()
}

#[derive(Debug, Deserialize)]
pub struct GenerateSensorScriptApiRequest {
    pub connector_type: String,
    pub name: String,
    pub gateway_url: String,
    pub api_key: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

/// Generates the three external-agent deployment artifacts used by the HTML connector
/// onboarding flow. Secrets are used only to render the caller-requested script and are never
/// persisted by this endpoint.
pub async fn generate_sensor_script_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<GenerateSensorScriptApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let connector_type = request.connector_type.trim();
    let Some(label) = display_name(connector_type) else {
        return api_error(StatusCode::BAD_REQUEST, "unknown connector_type");
    };
    let name = request.name.trim();
    if name.is_empty() || name.len() > 120 {
        return api_error(StatusCode::BAD_REQUEST, "name must be between 1 and 120 characters");
    }
    let gateway_url = request.gateway_url.trim();
    if !(gateway_url.starts_with("http://") || gateway_url.starts_with("https://")) {
        return api_error(StatusCode::BAD_REQUEST, "gateway_url must be an http(s) URL");
    }
    if request.api_key.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "api_key is required");
    }
    let fields = fields_for(connector_type);
    for supplied in request.fields.keys() {
        if !fields.iter().any(|field| field.env_var == supplied) {
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("unknown connector field: {supplied}"),
            );
        }
    }
    for field in &fields {
        if !field.optional
            && request
                .fields
                .get(field.env_var)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        {
            return api_error(StatusCode::BAD_REQUEST, format!("{} is required", field.env_var));
        }
    }
    let (bash, powershell, docker) = crate::sensor_script_handler::build_scripts(
        connector_type,
        label,
        name,
        principal.tenant_id,
        gateway_url,
        request.api_key.trim(),
        &request.fields,
    );
    axum::Json(serde_json::json!({
        "connector_type": connector_type,
        "connector_label": label,
        "sensor_name": name,
        "bash_script": bash,
        "powershell_script": powershell,
        "docker_command": docker,
    }))
    .into_response()
}

pub async fn list_users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state.users_client.list_users(principal.tenant_id, principal.role).await {
        Ok(users) => axum::Json(users).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// Returns one tenant-scoped identity for focused administration views. The Auth Service remains
/// the source of truth; this handler never accepts a tenant identifier from the caller.
pub async fn get_user_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    match state.users_client.list_users(principal.tenant_id, principal.role).await {
        Ok(users) => match users.into_iter().find(|user| user.id == id) {
            Some(user) => axum::Json(user).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "user not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateUserApiRequest {
    pub username: String,
    pub password: String,
    pub role: Role,
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateUserApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    if request.username.trim().is_empty() || request.username.len() > 160 {
        return api_error(StatusCode::BAD_REQUEST, "username must be between 1 and 160 characters");
    }
    if request.password.len() < 12 {
        return api_error(StatusCode::BAD_REQUEST, "password must be at least 12 characters");
    }
    match state
        .users_client
        .create_user(
            principal.tenant_id,
            principal.role,
            request.username.trim(),
            &request.password,
            request.role,
            &principal.username,
        )
        .await
    {
        Ok(user) => (StatusCode::CREATED, axum::Json(user)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRoleApiRequest {
    pub role: Role,
}

pub async fn update_user_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<UpdateUserRoleApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state
        .users_client
        .update_user_role(
            principal.tenant_id,
            principal.role,
            id,
            request.role,
            &principal.username,
        )
        .await
    {
        Ok(user) => axum::Json(user).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state
        .users_client
        .delete_user(principal.tenant_id, principal.role, id, &principal.username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkDeleteUsersApiRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct BulkDeleteUsersApiResponse {
    pub deleted: Vec<Uuid>,
    pub failed: Vec<Uuid>,
}

pub async fn bulk_delete_users_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkDeleteUsersApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 users");
    }
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let tenant_users =
        match state.users_client.list_users(principal.tenant_id, principal.role).await {
            Ok(users) => {
                users.into_iter().map(|user| user.id).collect::<std::collections::HashSet<_>>()
            }
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        if !tenant_users.contains(&id) {
            failed.push(id);
            continue;
        }
        match state
            .users_client
            .delete_user(principal.tenant_id, principal.role, id, &principal.username)
            .await
        {
            Ok(()) => deleted.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(BulkDeleteUsersApiResponse { deleted, failed }).into_response()
}

#[derive(Debug, Deserialize)]
pub struct BulkUpdateUserRoleApiRequest {
    pub ids: Vec<Uuid>,
    pub role: Role,
}

#[derive(Debug, Serialize)]
pub struct BulkUpdateUserRoleApiResponse {
    pub updated: Vec<Uuid>,
    pub failed: Vec<Uuid>,
    pub role: Role,
}

pub async fn bulk_update_user_role_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkUpdateUserRoleApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 users");
    }
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let tenant_users =
        match state.users_client.list_users(principal.tenant_id, principal.role).await {
            Ok(users) => {
                users.into_iter().map(|user| user.id).collect::<std::collections::HashSet<_>>()
            }
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        if !tenant_users.contains(&id) {
            failed.push(id);
            continue;
        }
        match state
            .users_client
            .update_user_role(
                principal.tenant_id,
                principal.role,
                id,
                request.role,
                &principal.username,
            )
            .await
        {
            Ok(_) => updated.push(id),
            Err(_) => failed.push(id),
        }
    }
    axum::Json(BulkUpdateUserRoleApiResponse { updated, failed, role: request.role })
        .into_response()
}

pub async fn get_password_policy(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state.users_client.password_policy().await {
        Ok(policy) => axum::Json(policy).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn export_user_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = require_admin(&principal) {
        return e;
    }
    match state.users_client.export_user_data(principal.tenant_id, principal.role, id).await {
        Ok(bytes) => {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn list_ontology_objects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyObjectsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(ontology_client) = crate::ontology_client::global() else {
        return api_error(StatusCode::BAD_GATEWAY, "ontology service is not configured");
    };
    match ontology_client.list_objects(&principal.query_token, query.type_id).await {
        Ok(objects) => axum::Json(
            objects
                .into_iter()
                .filter(|object| ontology_object_api_matches(object, &query))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct OntologyGraphApiQuery {
    pub object_id: Option<Uuid>,
    pub link_type_id: Option<Uuid>,
    pub q: Option<String>,
    pub depth: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct OntologyGraphNodeApi {
    pub id: Uuid,
    pub object_type_id: Uuid,
    pub type_name: String,
    pub summary: String,
    pub properties: serde_json::Value,
    pub degree: usize,
}

#[derive(Debug, Serialize)]
pub struct OntologyGraphEdgeApi {
    pub id: Uuid,
    pub link_type_id: Uuid,
    pub label: String,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub properties: Option<serde_json::Value>,
}

fn ontology_graph_neighbors(
    objects: &[common::ontology::Object],
    links: &[common::ontology::Link],
    center: Uuid,
    depth: usize,
    limit: usize,
) -> std::collections::HashSet<Uuid> {
    if !objects.iter().any(|object| object.id == center) {
        return std::collections::HashSet::new();
    }
    let object_ids =
        objects.iter().map(|object| object.id).collect::<std::collections::HashSet<_>>();
    let mut seen = std::collections::HashSet::from([center]);
    let mut frontier = vec![center];
    for _ in 0..depth {
        let mut next = Vec::new();
        for link in links {
            let neighbor = if frontier.contains(&link.source_object_id) {
                Some(link.target_object_id)
            } else if frontier.contains(&link.target_object_id) {
                Some(link.source_object_id)
            } else {
                None
            };
            let Some(neighbor) = neighbor else { continue };
            if object_ids.contains(&neighbor) && seen.insert(neighbor) {
                next.push(neighbor);
                if seen.len() >= limit {
                    return seen;
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    seen
}

/// Returns a bounded, tenant-scoped ontology graph for external operator shells. The HTML
/// Ontology workbench and API consumers now share the same neighborhood, relationship-type, and
/// evidence boundaries instead of requiring clients to reconstruct edges from flat objects.
pub async fn get_ontology_graph_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyGraphApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let depth = query.depth.unwrap_or(1);
    if !(1..=3).contains(&depth) {
        return api_error(StatusCode::BAD_REQUEST, "depth must be between 1 and 3");
    }
    let limit = query.limit.unwrap_or(24);
    if !(1..=100).contains(&limit) {
        return api_error(StatusCode::BAD_REQUEST, "limit must be between 1 and 100");
    }
    let Some(client) = crate::ontology_client::global() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "ontology client unavailable");
    };
    let (types, link_types, objects, links) = tokio::join!(
        client.list_object_types(&principal.query_token),
        client.list_link_types(&principal.query_token),
        client.list_objects(&principal.query_token, None),
        client.list_links(&principal.query_token),
    );
    let types = match types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let link_types = match link_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let objects = match objects {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let links = match links {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let link_type_ids = link_types
        .iter()
        .filter(|item| query.link_type_id.is_none_or(|id| item.id == id))
        .map(|item| item.id)
        .collect::<std::collections::HashSet<_>>();
    let graph_links = links
        .into_iter()
        .filter(|link| link_type_ids.contains(&link.link_type_id))
        .collect::<Vec<_>>();
    let filter = OntologyObjectsApiQuery { q: query.q.clone(), ..Default::default() };
    let matching = objects
        .iter()
        .filter(|object| ontology_object_api_matches(object, &filter))
        .cloned()
        .collect::<Vec<_>>();
    let selected_ids = if let Some(center) = query.object_id {
        let mut ids = ontology_graph_neighbors(&objects, &graph_links, center, depth, limit);
        ids.retain(|id| *id == center || matching.iter().any(|object| object.id == *id));
        ids
    } else {
        matching.iter().take(limit).map(|object| object.id).collect()
    };
    if selected_ids.is_empty() && query.object_id.is_some() {
        return api_error(StatusCode::NOT_FOUND, "ontology graph center object not found");
    }
    let selected = selected_ids.iter().copied().collect::<std::collections::HashSet<_>>();
    let type_names = types
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let link_names = link_types
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let mut degrees = std::collections::HashMap::<Uuid, usize>::new();
    let edges = graph_links
        .into_iter()
        .filter(|link| {
            selected.contains(&link.source_object_id) && selected.contains(&link.target_object_id)
        })
        .map(|link| {
            *degrees.entry(link.source_object_id).or_default() += 1;
            *degrees.entry(link.target_object_id).or_default() += 1;
            OntologyGraphEdgeApi {
                id: link.id,
                link_type_id: link.link_type_id,
                label: link_names
                    .get(&link.link_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown relationship".to_string()),
                source_id: link.source_object_id,
                target_id: link.target_object_id,
                properties: link.properties,
            }
        })
        .collect::<Vec<_>>();
    let nodes = objects
        .into_iter()
        .filter(|object| selected.contains(&object.id))
        .map(|object| OntologyGraphNodeApi {
            id: object.id,
            object_type_id: object.object_type_id,
            type_name: type_names
                .get(&object.object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown type".to_string()),
            summary: object
                .properties
                .get("name")
                .or_else(|| object.properties.get("subject"))
                .or_else(|| object.properties.get("title"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Untitled object")
                .to_string(),
            properties: object.properties,
            degree: degrees.get(&object.id).copied().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    axum::Json(serde_json::json!({
        "center_id": query.object_id,
        "depth": depth,
        "limit": limit,
        "truncated": matching.len() > limit || selected_ids.len() >= limit,
        "nodes": nodes,
        "edges": edges,
    }))
    .into_response()
}

/// Returns one tenant-scoped ontology object for direct investigation handoffs. The ontology
/// client currently exposes a tenant-scoped collection read rather than a single-object read;
/// resolve through that boundary instead of reaching into the ontology store from the UI.
pub async fn get_ontology_object(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&_state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client = match ontology_client() {
        Ok(v) => v,
        Err(e) => return e,
    };
    match client.list_objects(&principal.query_token, None).await {
        Ok(objects) => match objects.into_iter().find(|object| object.id == id) {
            Some(object) => axum::Json(object).into_response(),
            None => api_error(StatusCode::NOT_FOUND, "ontology object not found"),
        },
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ObjectAnnotationApiRequest {
    pub body: String,
}

pub async fn list_ontology_object_annotations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let Some(client) = crate::ontology_client::global() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "ontology client unavailable");
    };
    match client.list_object_annotations(&principal.query_token, id).await {
        Ok(annotations) => {
            axum::Json(annotations.into_iter().take(100).collect::<Vec<_>>()).into_response()
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn create_ontology_object_annotation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<ObjectAnnotationApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let body = request.body.trim().to_string();
    if body.is_empty() || body.len() > 4000 {
        return api_error(StatusCode::BAD_REQUEST, "body must contain 1–4000 characters");
    }
    let Some(client) = crate::ontology_client::global() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "ontology client unavailable");
    };
    match client
        .create_object_annotation(
            &principal.query_token,
            &principal.username,
            id,
            &crate::ontology_client::CreateObjectAnnotationRequest { body },
        )
        .await
    {
        Ok(annotation) => (StatusCode::CREATED, axum::Json(annotation)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct OntologyObject360RelatedApi {
    id: Uuid,
    relationship_id: Uuid,
    link_type_id: Uuid,
    relationship: String,
    direction: String,
    type_name: String,
    summary: String,
    properties: Option<serde_json::Value>,
    properties_schema: Option<serde_json::Value>,
    history_href: String,
    history: Vec<OntologyObject360HistoryApi>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360RelationshipTargetApi {
    id: Uuid,
    type_name: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct OntologyObject360RelationshipOptionApi {
    id: Uuid,
    name: String,
    direction: String,
    cardinality: String,
    eligible: bool,
    eligibility_reason: Option<String>,
    target_type_name: String,
    targets: Vec<OntologyObject360RelationshipTargetApi>,
    properties_schema: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360SignalApi {
    id: Uuid,
    event_type: String,
    status: String,
    occurred_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360SourceRecordApi {
    id: Uuid,
    connector_id: String,
    source_type: String,
    ingested_at: DateTime<Utc>,
    preview: String,
}

#[derive(Debug, Serialize)]
struct OntologyObject360IncidentApi {
    id: Uuid,
    title: String,
    severity: String,
    status: String,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360ActionApi {
    id: Uuid,
    action_name: String,
    outcome: String,
    review_status: Option<String>,
    review_assignee: Option<String>,
    executed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360AvailableActionApi {
    id: Uuid,
    name: String,
    parameter_schema: serde_json::Value,
    preconditions: serde_json::Value,
    effect_definition: serde_json::Value,
    eligible: bool,
}

#[derive(Debug, Serialize)]
struct OntologyObject360HistoryApi {
    change_type: String,
    actor: String,
    before_state: Option<serde_json::Value>,
    after_state: Option<serde_json::Value>,
    changed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360TimelineApi {
    kind: String,
    title: String,
    detail: String,
    occurred_at: DateTime<Utc>,
    href: Option<String>,
}

#[derive(Debug, Serialize)]
struct OntologyObject360Api {
    object: common::ontology::Object,
    object_type: Option<common::ontology::ObjectType>,
    related: Vec<OntologyObject360RelatedApi>,
    relationship_options: Vec<OntologyObject360RelationshipOptionApi>,
    source_records: Vec<OntologyObject360SourceRecordApi>,
    signals: Vec<OntologyObject360SignalApi>,
    incidents: Vec<OntologyObject360IncidentApi>,
    actions: Vec<OntologyObject360ActionApi>,
    available_actions: Vec<OntologyObject360AvailableActionApi>,
    history: Vec<OntologyObject360HistoryApi>,
    annotations: Vec<common::ontology::ObjectAnnotation>,
    timeline: Vec<OntologyObject360TimelineApi>,
}

fn ontology_object_summary_api(object: &common::ontology::Object) -> String {
    object
        .properties
        .get("name")
        .or_else(|| object.properties.get("subject"))
        .or_else(|| object.properties.get("title"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Untitled object")
        .to_string()
}

fn action_targets_object_api(targets: &serde_json::Value, object_id: Uuid) -> bool {
    targets.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.as_str().and_then(|value| Uuid::parse_str(value).ok()) == Some(object_id)
        })
    })
}

/// Returns the bounded object-centric investigation read model used by the browser Ontology
/// page. This keeps external operator shells from having to join raw ontology, signal, case,
/// action-review, and immutable-history endpoints themselves.
pub async fn get_ontology_object_360_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let Some(client) = crate::ontology_client::global() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "ontology client unavailable");
    };
    let (types, objects, links, link_types, action_types, invocations, reviews, events, incidents) = tokio::join!(
        client.list_object_types(&principal.query_token),
        client.list_objects(&principal.query_token, None),
        client.list_links(&principal.query_token),
        client.list_link_types(&principal.query_token),
        client.list_action_types(&principal.query_token),
        client.list_action_invocations(&principal.query_token),
        client.list_action_reviews(&principal.query_token),
        state.events_client.list_events(&principal.query_token, 1000, 0, None, None),
        state.incidents_client.list_incidents(principal.tenant_id, None),
    );
    let types = match types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let objects = match objects {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let object = match objects.iter().find(|object| object.id == id) {
        Some(value) => value.clone(),
        None => return api_error(StatusCode::NOT_FOUND, "ontology object not found"),
    };
    let links = match links {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let link_types = match link_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let link_history =
        client.list_all_link_history(&principal.query_token).await.unwrap_or_default();
    let mut link_history_by_id =
        std::collections::HashMap::<Uuid, Vec<OntologyObject360HistoryApi>>::new();
    for entry in link_history {
        link_history_by_id.entry(entry.link_id).or_default().push(OntologyObject360HistoryApi {
            change_type: entry.change_type,
            actor: entry.actor,
            before_state: entry.before_state,
            after_state: entry.after_state,
            changed_at: entry.changed_at,
        });
    }
    let action_types = match action_types {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let invocations = match invocations {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let reviews = match reviews {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let events = match events {
        Ok(page) => page.events,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let incidents = match incidents {
        Ok(value) => value,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let types_by_id =
        types.iter().map(|item| (item.id, item)).collect::<std::collections::HashMap<_, _>>();
    let objects_by_id =
        objects.iter().map(|item| (item.id, item)).collect::<std::collections::HashMap<_, _>>();
    let link_names = link_types
        .iter()
        .map(|item| (item.id, item.name.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let link_schemas = link_types
        .iter()
        .map(|item| (item.id, item.properties_schema.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let relationship_options = link_types
        .iter()
        .filter_map(|link_type| {
            let (direction, target_type_id) =
                if link_type.source_object_type_id == object.object_type_id {
                    ("outgoing", link_type.target_object_type_id)
                } else if link_type.target_object_type_id == object.object_type_id {
                    ("incoming", link_type.source_object_type_id)
                } else {
                    return None;
                };
            let target_type_name = types_by_id
                .get(&target_type_id)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| "Unknown type".to_string());
            let source_unique = link_type.cardinality != "one-to-many";
            let target_unique = link_type.cardinality != "many-to-one";
            let center_has_link = links.iter().any(|link| {
                link.link_type_id == link_type.id
                    && if direction == "outgoing" {
                        source_unique && link.source_object_id == object.id
                    } else {
                        target_unique && link.target_object_id == object.id
                    }
            });
            let targets = objects
                .iter()
                .filter(|candidate| {
                    if candidate.object_type_id != target_type_id || candidate.id == object.id {
                        return false;
                    }
                    if direction == "outgoing" && target_unique {
                        !links.iter().any(|link| {
                            link.link_type_id == link_type.id
                                && link.target_object_id == candidate.id
                        })
                    } else if direction == "incoming" && source_unique {
                        !links.iter().any(|link| {
                            link.link_type_id == link_type.id
                                && link.source_object_id == candidate.id
                        })
                    } else {
                        true
                    }
                })
                .take(50)
                .map(|candidate| OntologyObject360RelationshipTargetApi {
                    id: candidate.id,
                    type_name: target_type_name.clone(),
                    summary: ontology_object_summary_api(candidate),
                })
                .collect::<Vec<_>>();
            Some(OntologyObject360RelationshipOptionApi {
                id: link_type.id,
                name: link_type.name.clone(),
                direction: direction.to_string(),
                cardinality: link_type.cardinality.clone(),
                eligible: !center_has_link && !targets.is_empty(),
                eligibility_reason: if center_has_link {
                    Some(format!("{} endpoint already has its governed relationship", direction))
                } else if targets.is_empty() {
                    Some("No compatible target objects are available".to_string())
                } else {
                    None
                },
                target_type_name,
                targets,
                properties_schema: link_type.properties_schema.clone(),
            })
        })
        .take(25)
        .collect::<Vec<_>>();
    let related = links
        .iter()
        .filter_map(|link| {
            let (related_id, direction) = if link.source_object_id == id {
                (link.target_object_id, "outgoing")
            } else if link.target_object_id == id {
                (link.source_object_id, "incoming")
            } else {
                return None;
            };
            let related_object = objects_by_id.get(&related_id)?;
            Some(OntologyObject360RelatedApi {
                id: related_id,
                relationship_id: link.id,
                link_type_id: link.link_type_id,
                relationship: link_names
                    .get(&link.link_type_id)
                    .copied()
                    .unwrap_or("Related object")
                    .to_string(),
                direction: direction.to_string(),
                type_name: types_by_id
                    .get(&related_object.object_type_id)
                    .map(|item| item.name.clone())
                    .unwrap_or_else(|| "Unknown type".to_string()),
                summary: ontology_object_summary_api(related_object),
                properties: link.properties.clone(),
                properties_schema: link_schemas.get(&link.link_type_id).cloned().flatten(),
                history_href: format!("/api/v1/ontology/links/{}/history", link.id),
                history: link_history_by_id.remove(&link.id).unwrap_or_default(),
            })
        })
        .take(50)
        .collect::<Vec<_>>();
    let lineage_ids = object
        .source_lineage
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .filter_map(|value| Uuid::parse_str(value).ok())
        .collect::<std::collections::HashSet<_>>();
    let mut source_records = Vec::new();
    for record_id in lineage_ids.iter().take(25) {
        if let Ok(Some(record)) =
            state.stats_client.get_record(principal.tenant_id, *record_id).await
        {
            let preview = record.preview();
            source_records.push(OntologyObject360SourceRecordApi {
                id: record.id,
                connector_id: record.connector_id,
                source_type: record.source_type,
                ingested_at: record.ingested_at,
                preview,
            });
        }
    }
    source_records.sort_by_key(|record| std::cmp::Reverse(record.ingested_at));
    let signals = events
        .iter()
        .filter(|event| event.record_ids.iter().any(|record_id| lineage_ids.contains(record_id)))
        .take(50)
        .map(|event| OntologyObject360SignalApi {
            id: event.id,
            event_type: event.event_type.clone(),
            status: event.status.clone(),
            occurred_at: event.occurred_at,
        })
        .collect::<Vec<_>>();
    let signal_ids =
        signals.iter().map(|signal| signal.id).collect::<std::collections::HashSet<_>>();
    let incident_ids = incidents
        .iter()
        .filter(|incident| incident.event_ids.iter().any(|event_id| signal_ids.contains(event_id)))
        .take(50)
        .map(|incident| incident.incident.id)
        .collect::<std::collections::HashSet<_>>();
    let incidents = incidents
        .into_iter()
        .filter(|incident| incident_ids.contains(&incident.incident.id))
        .map(|incident| OntologyObject360IncidentApi {
            id: incident.incident.id,
            title: incident.incident.title,
            severity: incident.incident.severity.to_string(),
            status: incident.incident.status.to_string(),
            updated_at: incident.incident.updated_at,
        })
        .collect::<Vec<_>>();
    let available_actions = action_types
        .iter()
        .map(|action| OntologyObject360AvailableActionApi {
            id: action.id,
            name: action.name.clone(),
            parameter_schema: action.parameter_schema.clone(),
            preconditions: action.preconditions.clone(),
            effect_definition: action.effect_definition.clone(),
            eligible: action
                .target_object_type_id
                .is_none_or(|type_id| type_id == object.object_type_id)
                && action_preconditions_satisfied(&object.properties, &action.preconditions),
        })
        .take(50)
        .collect::<Vec<_>>();
    let action_names = action_types
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let review_by_invocation = reviews
        .into_iter()
        .map(|review| (review.invocation_id, review))
        .collect::<std::collections::HashMap<_, _>>();
    let actions = invocations
        .into_iter()
        .filter(|invocation| action_targets_object_api(&invocation.target_object_ids, id))
        .take(50)
        .map(|invocation| {
            let review = review_by_invocation.get(&invocation.id);
            OntologyObject360ActionApi {
                id: invocation.id,
                action_name: action_names
                    .get(&invocation.action_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown governed action".to_string()),
                outcome: invocation.outcome,
                review_status: review.map(|item| item.status.clone()),
                review_assignee: review.and_then(|item| item.assignee.clone()),
                executed_at: invocation.executed_at,
            }
        })
        .collect::<Vec<_>>();
    let history: Vec<OntologyObject360HistoryApi> =
        match client.list_object_history(&principal.query_token, id).await {
            Ok(value) => value
                .into_iter()
                .take(50)
                .map(|entry| OntologyObject360HistoryApi {
                    change_type: entry.change_type,
                    actor: entry.actor,
                    before_state: entry.before_state,
                    after_state: entry.after_state,
                    changed_at: entry.changed_at,
                })
                .collect(),
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
    let annotations = match client.list_object_annotations(&principal.query_token, id).await {
        Ok(value) => value.into_iter().take(100).collect::<Vec<_>>(),
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let mut timeline = Vec::with_capacity(
        source_records.len()
            + signals.len()
            + incidents.len()
            + actions.len()
            + history.len()
            + annotations.len(),
    );
    timeline.extend(source_records.iter().map(|item| OntologyObject360TimelineApi {
        kind: "evidence".to_string(),
        title: item.source_type.clone(),
        detail: format!("{} · {}", item.connector_id, item.preview),
        occurred_at: item.ingested_at,
        href: Some(format!("/data/{}/journey", item.id)),
    }));
    timeline.extend(signals.iter().map(|item| OntologyObject360TimelineApi {
        kind: "signal".to_string(),
        title: item.event_type.clone(),
        detail: item.status.clone(),
        occurred_at: item.occurred_at,
        href: Some(format!("/events/{}", item.id)),
    }));
    timeline.extend(incidents.iter().map(|item| OntologyObject360TimelineApi {
        kind: "case".to_string(),
        title: item.title.clone(),
        detail: format!("{} · {}", item.severity, item.status),
        occurred_at: item.updated_at,
        href: Some(format!("/incidents/{}", item.id)),
    }));
    timeline.extend(actions.iter().map(|item| OntologyObject360TimelineApi {
        kind: "decision".to_string(),
        title: item.action_name.clone(),
        detail: format!(
            "{} · review {}",
            item.outcome,
            item.review_status.as_deref().unwrap_or("not reviewed")
        ),
        occurred_at: item.executed_at,
        href: Some(format!("/actions/{}", item.id)),
    }));
    timeline.extend(history.iter().map(|item| OntologyObject360TimelineApi {
        kind: "model".to_string(),
        title: format!("Model {}", item.change_type),
        detail: format!("by {}", item.actor),
        occurred_at: item.changed_at,
        href: None,
    }));
    timeline.extend(annotations.iter().map(|item| OntologyObject360TimelineApi {
        kind: "annotation".to_string(),
        title: format!("Operator note by {}", item.author),
        detail: item.body.clone(),
        occurred_at: item.created_at,
        href: None,
    }));
    timeline.sort_by_key(|item| std::cmp::Reverse(item.occurred_at));
    timeline.truncate(100);
    axum::Json(OntologyObject360Api {
        object_type: types_by_id.get(&object.object_type_id).map(|item| (*item).clone()),
        object,
        related,
        relationship_options,
        source_records,
        signals,
        incidents,
        actions,
        available_actions,
        history,
        annotations,
        timeline,
    })
    .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct OntologyCompareApiQuery {
    #[serde(default)]
    pub ids: String,
}

#[derive(Debug, Serialize)]
struct OntologyCompareObjectApi {
    id: Uuid,
    type_name: String,
    summary: String,
    updated_at: DateTime<Utc>,
    properties: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct OntologyComparePropertyApi {
    key: String,
    values: Vec<String>,
}

/// Compares a bounded set of modeled objects for API-driven workbenches. The browser route
/// renders the same property matrix as HTML; this projection keeps the object-centric evidence
/// usable by external operator consoles without exposing objects outside the authenticated
/// tenant.
pub async fn get_ontology_compare_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyCompareApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(client) = crate::ontology_client::global() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "ontology client unavailable");
    };
    let ids = query
        .ids
        .split(',')
        .filter_map(|value| Uuid::parse_str(value.trim()).ok())
        .take(6)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain at least one object UUID");
    }
    let types = match client.list_object_types(&principal.query_token).await {
        Ok(types) => types,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let type_names = types
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let all_objects = match client.list_objects(&principal.query_token, None).await {
        Ok(objects) => objects,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let by_id = all_objects
        .into_iter()
        .map(|object| (object.id, object))
        .collect::<std::collections::HashMap<_, _>>();
    let objects = ids
        .into_iter()
        .filter_map(|id| by_id.get(&id))
        .map(|object| {
            let summary = object
                .properties
                .get("name")
                .or_else(|| object.properties.get("subject"))
                .or_else(|| object.properties.get("title"))
                .or_else(|| object.properties.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Untitled object")
                .to_string();
            OntologyCompareObjectApi {
                id: object.id,
                type_name: type_names
                    .get(&object.object_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown type".to_string()),
                summary,
                updated_at: object.updated_at,
                properties: object.properties.as_object().cloned().unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    let mut keys = std::collections::BTreeSet::new();
    for object in &objects {
        keys.extend(object.properties.keys().cloned());
    }
    let properties = keys
        .into_iter()
        .map(|key| OntologyComparePropertyApi {
            values: objects
                .iter()
                .map(|object| {
                    object.properties.get(&key).map_or_else(
                        || "—".to_string(),
                        |value| {
                            if value.is_string() {
                                value.as_str().unwrap_or_default().to_string()
                            } else {
                                serde_json::to_string(value).unwrap_or_default()
                            }
                        },
                    )
                })
                .collect(),
            key,
        })
        .collect::<Vec<_>>();
    let differing_property_count = properties
        .iter()
        .filter(|row| row.values.windows(2).any(|values| values[0] != values[1]))
        .count();
    let shared_property_count = properties.len().saturating_sub(differing_property_count);
    axum::Json(serde_json::json!({
        "requested_count": query.ids.split(',').filter(|value| !value.trim().is_empty()).count().min(6),
        "matched_count": objects.len(),
        "objects": objects,
        "properties": properties,
        "shared_property_count": shared_property_count,
        "differing_property_count": differing_property_count,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct BrandingApiRequest {
    pub product_name: Option<String>,
    pub logo_url: Option<String>,
    pub accent_color: Option<String>,
}

pub async fn get_branding_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.branding_client.get_branding_by_id(principal.tenant_id).await {
        Ok(branding) => axum::Json(branding).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn put_branding_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BrandingApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let branding = crate::branding_client::Branding {
        product_name: request.product_name.and_then(trimmed_option),
        logo_url: request.logo_url.and_then(trimmed_option),
        accent_color: request.accent_color.and_then(trimmed_option),
    };
    match state
        .branding_client
        .put_branding(principal.tenant_id, principal.role, &principal.username, branding.clone())
        .await
    {
        Ok(()) => axum::Json(branding).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn trimmed_option(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[derive(Debug, Deserialize)]
pub struct AnalysisConfigApiRequest {
    pub prompt: String,
    #[serde(default)]
    pub provider: common::AnalysisProvider,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    /// Omitted keeps the existing secret, `null` clears it, and a string replaces it.
    #[serde(default)]
    pub api_key: Option<Option<String>>,
}

pub async fn get_analysis_config_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.analysis_config_client.get_analysis_config(principal.tenant_id).await {
        Ok(config) => axum::Json(config).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn put_analysis_config_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<AnalysisConfigApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let api_key = request.api_key.as_ref().map(|value| value.as_deref());
    let input = crate::analysis_config_client::AnalysisConfigInput {
        prompt: &request.prompt,
        provider: request.provider,
        model: request.model.as_deref(),
        endpoint: request.endpoint.as_deref(),
        api_key,
    };
    match state
        .analysis_config_client
        .put_analysis_config(principal.tenant_id, principal.role, &principal.username, input)
        .await
    {
        Ok(config) => axum::Json(config).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub fn validate_egress_domains(domains: &[String]) -> Result<(), Response> {
    if domains.len() > 1000 {
        return Err(api_error(StatusCode::BAD_REQUEST, "at most 1000 egress domains are allowed"));
    }
    if domains.iter().any(|domain| {
        let domain = domain.trim();
        domain.is_empty() || domain.len() > 253 || domain.chars().any(char::is_whitespace)
    }) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "egress domains must be non-empty hostnames",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct EgressAllowlistApiRequest {
    #[serde(default)]
    pub domains: Vec<String>,
}

pub async fn get_egress_allowlist_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.egress_allowlist_client.get_allowlist(principal.tenant_id).await {
        Ok(domains) => axum::Json(serde_json::json!({ "domains": domains })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn put_egress_allowlist_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<EgressAllowlistApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if let Err(error) = validate_egress_domains(&request.domains) {
        return error;
    }
    let domains = request.domains.into_iter().filter_map(trimmed_option).collect::<Vec<_>>();
    match state
        .egress_allowlist_client
        .put_allowlist(principal.tenant_id, principal.role, domains, &principal.username)
        .await
    {
        Ok(domains) => axum::Json(serde_json::json!({ "domains": domains })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct SessionApiProjection {
    id: String,
    username: String,
    role: Role,
    created_at: DateTime<Utc>,
    current: bool,
}

pub async fn list_sessions_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let current_id = crate::session_guard::session_cookie_value(&headers);
    let sessions = state
        .session_store
        .list_for_tenant(principal.tenant_id)
        .await
        .into_iter()
        .map(|(id, session)| SessionApiProjection {
            current: current_id.as_deref() == Some(id.as_str()),
            id,
            username: session.username,
            role: session.role,
            created_at: session.created_at,
        })
        .collect::<Vec<_>>();
    axum::Json(serde_json::json!({ "sessions": sessions })).into_response()
}

pub async fn revoke_session_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let Some((_, target)) = state
        .session_store
        .list_for_tenant(principal.tenant_id)
        .await
        .into_iter()
        .find(|(session_id, _)| *session_id == id)
    else {
        return api_error(StatusCode::NOT_FOUND, "session not found");
    };
    state.session_store.delete(&id).await;
    if let Ok(session_id) = id.parse() {
        let _ = state
            .users_client
            .record_session_revocation(
                principal.tenant_id,
                principal.role,
                &principal.username,
                session_id,
                &target.username,
            )
            .await;
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct BulkRevokeSessionsApiRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkRevokeSessionsApiResponse {
    pub revoked: Vec<String>,
    pub failed: Vec<String>,
}

pub async fn bulk_revoke_sessions_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkRevokeSessionsApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    if request.ids.is_empty() || request.ids.len() > 100 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 100 sessions");
    }
    let mut ids = request.ids;
    ids.sort_unstable();
    ids.dedup();
    let tenant_sessions = state
        .session_store
        .list_for_tenant(principal.tenant_id)
        .await
        .into_iter()
        .collect::<HashMap<_, _>>();
    let mut revoked = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        let Some(target) = tenant_sessions.get(&id) else {
            failed.push(id);
            continue;
        };
        state.session_store.delete(&id).await;
        if let Ok(session_id) = id.parse() {
            let audit = state
                .users_client
                .record_session_revocation(
                    principal.tenant_id,
                    principal.role,
                    &principal.username,
                    session_id,
                    &target.username,
                )
                .await;
            if audit.is_err() {
                failed.push(id);
                continue;
            }
        }
        revoked.push(id);
    }
    axum::Json(BulkRevokeSessionsApiResponse { revoked, failed }).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct LoginAttemptsApiQuery {
    pub limit: Option<u32>,
    pub before: Option<DateTime<Utc>>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub reason: Option<String>,
}

fn login_attempt_api_matches(
    attempt: &crate::login_attempts_client::LoginAttempt,
    query: &LoginAttemptsApiQuery,
) -> bool {
    let q = query.q.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let status = query.status.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let reason = query.reason.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    (q.is_empty() || attempt.username.to_ascii_lowercase().contains(&q))
        && (status.is_empty()
            || (status == "success" && attempt.success)
            || (status == "failed" && !attempt.success))
        && (reason.is_empty() || attempt.reason.eq_ignore_ascii_case(&reason))
}

pub async fn list_login_attempts_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LoginAttemptsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 200) as usize;
    match state
        .login_attempts_client
        .list_recent(principal.tenant_id, principal.role, query.before)
        .await
    {
        Ok(mut attempts) => {
            attempts.retain(|attempt| login_attempt_api_matches(attempt, &query));
            let has_more = attempts.len() > limit;
            attempts.truncate(limit);
            let next_before = attempts.last().map(|attempt| attempt.attempted_at);
            axum::Json(serde_json::json!({
                "attempts": attempts,
                "has_more": has_more,
                "limit": limit,
                "next_before": next_before,
            }))
            .into_response()
        }
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn export_login_attempts_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LoginAttemptsApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let mut before = query.before;
    let mut rows = Vec::new();
    let mut exhausted = false;
    for _ in 0..10 {
        let page = match state
            .login_attempts_client
            .list_recent(principal.tenant_id, principal.role, before)
            .await
        {
            Ok(page) => page,
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
        if page.is_empty() {
            exhausted = true;
            break;
        }
        before = page.last().map(|attempt| attempt.attempted_at);
        rows.extend(page.into_iter().filter(|attempt| login_attempt_api_matches(attempt, &query)));
    }
    rows.sort_by_key(|attempt| std::cmp::Reverse(attempt.attempted_at));
    let mut csv = String::from("attempted_at,username,success,reason\n");
    for attempt in rows {
        csv.push_str(&format!(
            "{},{},{},{}\n",
            attempt.attempted_at.to_rfc3339(),
            api_csv_escape(&attempt.username),
            attempt.success,
            api_csv_escape(&attempt.reason),
        ));
    }
    let mut response = (StatusCode::OK, csv).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_static("attachment; filename=login-attempts.csv"),
    );
    if !exhausted {
        if let Some(next_before) = before {
            if let Ok(value) = next_before.to_rfc3339().parse() {
                response
                    .headers_mut()
                    .insert(axum::http::HeaderName::from_static("x-next-before"), value);
            }
        }
    }
    response
}

#[derive(Debug, Deserialize, Default)]
pub struct BackupApiQuery {
    pub before: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

fn normalize_backup_status_api(value: Option<&String>) -> Result<String, Response> {
    let status = value.map(String::as_str).unwrap_or("").trim().to_ascii_lowercase();
    match status.as_str() {
        "" => Ok(String::new()),
        "success" | "failed" | "running" => Ok(status),
        _ => Err(api_error(StatusCode::BAD_REQUEST, "status must be success, failed, or running")),
    }
}

fn backup_status_api(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "success" | "completed" | "succeeded" => "success",
        "failed" | "failure" | "error" => "failed",
        _ => "running",
    }
}

pub async fn list_backups_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<BackupApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let status = match normalize_backup_status_api(query.status.as_ref()) {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.backup_status_client.list_recent(principal.role, query.before).await {
        Ok(runs) => axum::Json(serde_json::json!({
            "runs": runs.into_iter().filter(|run| status.is_empty() || backup_status_api(&run.status) == status).collect::<Vec<_>>()
        })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn trigger_backup_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    match state.backup_status_client.trigger_backup().await {
        Ok(result) => (StatusCode::ACCEPTED, axum::Json(result)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Serialize)]
struct ComplianceApiControl {
    label: String,
    state: String,
    detail: String,
    href: String,
}

#[derive(Debug, Serialize)]
struct ComplianceApiResponse {
    generated_at: DateTime<Utc>,
    control_score: usize,
    control_total: usize,
    controls: Vec<ComplianceApiControl>,
    metrics: serde_json::Value,
    errors: Vec<String>,
}

/// Returns the same evidence-backed control posture as the browser compliance report, in a
/// machine-readable form for auditors and tenant governance integrations.
pub async fn get_compliance_report_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_admin(&principal) {
        return error;
    }
    let mut errors = Vec::new();

    let (users, password_policy_loaded) = tokio::join!(
        state.users_client.list_users(principal.tenant_id, principal.role),
        state.users_client.password_policy(),
    );
    let (mut admin_count, mut operator_count, mut viewer_count, mut mfa_enabled_count) =
        (0usize, 0usize, 0usize, 0usize);
    let total_user_count = match users {
        Ok(users) => {
            for user in &users {
                match user.role {
                    Role::Admin => admin_count += 1,
                    Role::Operator => operator_count += 1,
                    Role::Viewer => viewer_count += 1,
                }
                if user.mfa_enabled {
                    mfa_enabled_count += 1;
                }
            }
            users.len()
        }
        Err(error) => {
            errors.push(format!("users: {error}"));
            0
        }
    };
    let password_policy_loaded = match password_policy_loaded {
        Ok(_) => true,
        Err(error) => {
            errors.push(format!("password policy: {error}"));
            false
        }
    };

    let (retention_policies, egress_domains) = tokio::join!(
        state.retention_policies_client.list_policies(principal.tenant_id),
        state.egress_allowlist_client.get_allowlist(principal.tenant_id),
    );
    let (retention_policy_count, retention_enabled_count) = match retention_policies {
        Ok(policies) => (policies.len(), policies.iter().filter(|policy| policy.enabled).count()),
        Err(error) => {
            errors.push(format!("retention policies: {error}"));
            (0, 0)
        }
    };
    let egress_domain_count = match egress_domains {
        Ok(domains) => domains.len(),
        Err(error) => {
            errors.push(format!("egress allowlist: {error}"));
            0
        }
    };

    let (sensors, connector_stats) = tokio::join!(
        state.sensors_client.list_sensors(principal.tenant_id, 1000, 0),
        state.stats_client.connector_stats(principal.tenant_id),
    );
    let (enabled_connector_count, stale_connector_count) = match (sensors, connector_stats) {
        (Ok(page), Ok(stats)) => {
            let now = Utc::now();
            let enabled = page.sensors.iter().filter(|sensor| sensor.enabled).collect::<Vec<_>>();
            let stale = enabled
                .iter()
                .filter(|sensor| {
                    stats
                        .iter()
                        .find(|stat| stat.connector_id == sensor.name)
                        .map(|stat| now - stat.last_ingested_at > chrono::Duration::hours(1))
                        .unwrap_or(true)
                })
                .count();
            (enabled.len(), stale)
        }
        (sensors, connector_stats) => {
            if let Err(error) = sensors {
                errors.push(format!("connectors: {error}"));
            }
            if let Err(error) = connector_stats {
                errors.push(format!("connector stats: {error}"));
            }
            (0, 0)
        }
    };

    let records = state
        .stats_client
        .search_records(
            principal.tenant_id,
            &crate::ingestion_stats_client::RecordSearchFilter {
                limit: 1000,
                ..Default::default()
            },
        )
        .await;
    let (total_record_count, normalized_record_count) = match records {
        Ok(result) => {
            let normalized = result.records.iter().filter(|record| record.is_normalized()).count();
            (result.records.len(), normalized)
        }
        Err(error) => {
            errors.push(format!("records: {error}"));
            (0, 0)
        }
    };

    let cutoff = Utc::now() - chrono::Duration::days(7);
    let (audit_entries, audit_errors) = crate::recent_audit_log_handler::fetch_merged_page(
        &state,
        principal.tenant_id,
        &principal.query_token,
        None,
        200,
    )
    .await;
    let recent_admin_activity_count =
        audit_entries.iter().filter(|(_, entry)| entry.changed_at >= cutoff).count();
    errors.extend(audit_errors);

    let failed_login_count_7d = match state
        .login_attempts_client
        .list_recent(principal.tenant_id, principal.role, None)
        .await
    {
        Ok(attempts) => attempts
            .iter()
            .filter(|attempt| !attempt.success && attempt.attempted_at >= cutoff)
            .count(),
        Err(error) => {
            errors.push(format!("login attempts: {error}"));
            0
        }
    };
    let (last_backup_status, recent_backup_failure_count) =
        match state.backup_status_client.list_recent(principal.role, None).await {
            Ok(runs) => (
                runs.first().map(|run| run.status.clone()),
                runs.iter().filter(|run| run.status == "failed").count(),
            ),
            Err(error) => {
                errors.push(format!("backup status: {error}"));
                (None, 0)
            }
        };

    let mfa_state = if total_user_count == 0 {
        "unknown"
    } else if mfa_enabled_count == total_user_count {
        "ready"
    } else {
        "attention"
    };
    let connector_state = if enabled_connector_count == 0 {
        "unknown"
    } else if stale_connector_count == 0 {
        "ready"
    } else {
        "attention"
    };
    let normalization_state = if total_record_count == 0 {
        "unknown"
    } else if normalized_record_count == total_record_count {
        "ready"
    } else {
        "attention"
    };
    let backup_state =
        if last_backup_status.as_deref() == Some("success") { "ready" } else { "attention" };
    let controls = vec![
        ComplianceApiControl {
            label: "MFA adoption".into(),
            state: mfa_state.into(),
            detail: format!("{mfa_enabled_count} of {total_user_count} accounts protected"),
            href: "/users".into(),
        },
        ComplianceApiControl {
            label: "Password policy".into(),
            state: if password_policy_loaded { "ready" } else { "unknown" }.into(),
            detail: if password_policy_loaded { "Policy loaded" } else { "Unavailable" }.into(),
            href: "/security/password".into(),
        },
        ComplianceApiControl {
            label: "Retention enforcement".into(),
            state: if retention_enabled_count > 0 { "ready" } else { "attention" }.into(),
            detail: format!(
                "{retention_enabled_count} of {retention_policy_count} policies enabled"
            ),
            href: "/retention-policies".into(),
        },
        ComplianceApiControl {
            label: "Egress boundary".into(),
            state: if egress_domain_count > 0 { "ready" } else { "attention" }.into(),
            detail: format!("{egress_domain_count} allowed domain entries"),
            href: "/egress-allowlist".into(),
        },
        ComplianceApiControl {
            label: "Connector freshness".into(),
            state: connector_state.into(),
            detail: format!(
                "{enabled_connector_count} enabled · {stale_connector_count} stale or silent"
            ),
            href: "/sensors?health=stale".into(),
        },
        ComplianceApiControl {
            label: "Normalization completeness".into(),
            state: normalization_state.into(),
            detail: format!("{normalized_record_count} of {total_record_count} records normalized"),
            href: "/normalization-mappings?coverage=pending".into(),
        },
        ComplianceApiControl {
            label: "Backup recovery".into(),
            state: backup_state.into(),
            detail: last_backup_status.clone().unwrap_or_else(|| "No run recorded".into()),
            href: "/security/backups".into(),
        },
        ComplianceApiControl {
            label: "Login anomaly signal".into(),
            state: if failed_login_count_7d == 0 { "ready" } else { "attention" }.into(),
            detail: format!("{failed_login_count_7d} failed attempts in 7 days"),
            href: "/security/login-attempts".into(),
        },
    ];
    let control_total = controls.len();
    let control_score = controls.iter().filter(|control| control.state == "ready").count();
    axum::Json(ComplianceApiResponse {
        generated_at: Utc::now(),
        control_score,
        control_total,
        controls,
        metrics: serde_json::json!({
            "admin_count": admin_count,
            "operator_count": operator_count,
            "viewer_count": viewer_count,
            "total_user_count": total_user_count,
            "mfa_enabled_count": mfa_enabled_count,
            "password_policy_loaded": password_policy_loaded,
            "recent_admin_activity_count": recent_admin_activity_count,
            "failed_login_count_7d": failed_login_count_7d,
            "retention_policy_count": retention_policy_count,
            "retention_enabled_count": retention_enabled_count,
            "egress_domain_count": egress_domain_count,
            "enabled_connector_count": enabled_connector_count,
            "stale_connector_count": stale_connector_count,
            "total_record_count": total_record_count,
            "normalized_record_count": normalized_record_count,
            "last_backup_status": last_backup_status,
            "recent_backup_failure_count": recent_backup_failure_count,
        }),
        errors,
    })
    .into_response()
}

pub async fn export_report_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<crate::reports_handler::ReportsQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (since, until) = crate::reports_handler::parse_date_range(&query.from, &query.to);
    let events = match state
        .events_client
        .list_events(&principal.query_token, 1000, 0, since, until)
        .await
    {
        Ok(page) => page.events,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let incidents = match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(items) => items,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let counts = crate::reports_handler::count_by_event_type(&events);
    let window = format!("{}..{}", query.from, query.to);
    let mut csv = String::from(
        "section,signal_window,event_type,count,incident_title,severity,status,linked_events,action_name,outcome,target_count,executed_at,source_event,source_incident\n",
    );
    csv.push_str(&format!(
        "summary,{},signals in window,{},,,,,,,,,,\n",
        crate::reports_handler::csv_escape(&window),
        events.len()
    ));
    for row in counts {
        csv.push_str(&format!(
            "signals,{},{},{},,,,,,,,,,\n",
            crate::reports_handler::csv_escape(&window),
            crate::reports_handler::csv_escape(&row.event_type),
            row.count
        ));
    }
    for incident in incidents {
        csv.push_str(&format!(
            "incidents,{},{},,{}, {},{},{},,,,,,\n",
            crate::reports_handler::csv_escape(&window),
            "",
            crate::reports_handler::csv_escape(&incident.incident.title),
            incident.incident.severity,
            incident.incident.status,
            incident.event_ids.len()
        ));
    }
    if let Some(client) = crate::ontology_client::global() {
        let action_types =
            client.list_action_types(&principal.query_token).await.unwrap_or_default();
        let action_names = action_types
            .into_iter()
            .map(|action| (action.id, action.name))
            .collect::<std::collections::HashMap<_, _>>();
        let mut actions =
            client.list_action_invocations(&principal.query_token).await.unwrap_or_default();
        actions.retain(|item| {
            since.map(|value| item.executed_at >= value).unwrap_or(true)
                && until.map(|value| item.executed_at <= value).unwrap_or(true)
        });
        actions.sort_by(|left, right| right.executed_at.cmp(&left.executed_at));
        for action in actions {
            let target_count =
                action.target_object_ids.as_array().map(|values| values.len()).unwrap_or(0);
            let event_id = action
                .triggering_event_ref
                .get("event_id")
                .or_else(|| action.triggering_event_ref.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let incident_id = action
                .triggering_event_ref
                .get("incident_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            csv.push_str(&format!(
                "actions,{},,,,,,,{},{},{},{},{},{}\n",
                crate::reports_handler::csv_escape(&window),
                crate::reports_handler::csv_escape(
                    action_names
                        .get(&action.action_type_id)
                        .map(String::as_str)
                        .unwrap_or("Unknown action"),
                ),
                crate::reports_handler::csv_escape(&action.outcome),
                target_count,
                crate::reports_handler::csv_escape(&action.executed_at.to_rfc3339()),
                crate::reports_handler::csv_escape(event_id),
                crate::reports_handler::csv_escape(incident_id),
            ));
        }
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers
        .insert(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        "attachment; filename=operational-report.csv".parse().unwrap(),
    );
    (response_headers, csv).into_response()
}

pub async fn export_report_pdf_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<crate::reports_handler::ReportsQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let (since, until) = crate::reports_handler::parse_date_range(&query.from, &query.to);
    let events = match state
        .events_client
        .list_events(&principal.query_token, 1000, 0, since, until)
        .await
    {
        Ok(page) => page.events,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let incidents = match state.incidents_client.list_incidents(principal.tenant_id, None).await {
        Ok(items) => items,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let counts = crate::reports_handler::count_by_event_type(&events);
    let mut lines = vec![
        "KIZASHI OPERATIONAL REPORT".to_string(),
        format!(
            "Signal window: {} to {}",
            if query.from.is_empty() { "all time" } else { &query.from },
            if query.to.is_empty() { "now" } else { &query.to }
        ),
        format!("Generated: {}", Utc::now().to_rfc3339()),
        String::new(),
        format!("Signals in window: {}", events.len()),
        format!("Incident records: {}", incidents.len()),
        String::new(),
        "EVENT TYPES".to_string(),
    ];
    lines.extend(counts.into_iter().map(|row| format!("{}: {}", row.event_type, row.count)));
    lines.push(String::new());
    lines.push("INCIDENT POSTURE".to_string());
    lines.extend(incidents.into_iter().take(20).map(|item| {
        format!(
            "{} | {} | {} | {} linked events",
            item.incident.severity,
            item.incident.status,
            item.incident.title,
            item.event_ids.len()
        )
    }));
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "application/pdf".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        "attachment; filename=operational-report.pdf".parse().unwrap(),
    );
    (response_headers, crate::reports_handler::build_report_pdf(&lines)).into_response()
}

pub async fn get_mfa_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.mfa_client.status(principal.tenant_id, &principal.username).await {
        Ok(enabled) => axum::Json(serde_json::json!({ "enabled": enabled })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_mfa_policy_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    match state.auth_client.get_mfa_policy(principal.tenant_id, principal.role).await {
        Ok(required) => axum::Json(serde_json::json!({ "required": required })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_oidc_provider_policy_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    match state.auth_client.get_oidc_provider_policy(principal.tenant_id, principal.role).await {
        Ok((provider, available_providers)) => axum::Json(serde_json::json!({
            "provider": provider,
            "available_providers": available_providers,
        }))
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateOidcProviderPolicyApiRequest {
    pub provider: Option<String>,
}

pub async fn update_oidc_provider_policy_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<UpdateOidcProviderPolicyApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    let provider = request.provider.as_deref().map(str::trim).filter(|value| !value.is_empty());
    match state
        .auth_client
        .set_oidc_provider_policy(
            principal.tenant_id,
            principal.role,
            provider,
            &principal.username,
        )
        .await
    {
        Ok(provider) => axum::Json(serde_json::json!({ "provider": provider })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn get_tenant_oidc_configs_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    match state.auth_client.list_tenant_oidc_providers(principal.tenant_id, principal.role).await {
        Ok(providers) => axum::Json(serde_json::json!({"providers": providers})).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct TenantOidcConfigApiRequest {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub redirect_url: String,
}

pub async fn put_tenant_oidc_config_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    axum::Json(request): axum::Json<TenantOidcConfigApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    if request.client_secret.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "client_secret is required");
    }
    match state
        .auth_client
        .set_tenant_oidc_provider(
            principal.tenant_id,
            principal.role,
            &provider,
            &request.client_id,
            &request.client_secret,
            &request.auth_url,
            &request.token_url,
            &request.userinfo_url,
            &request.redirect_url,
            &principal.username,
        )
        .await
    {
        Ok(providers) => axum::Json(serde_json::json!({"providers": providers})).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn delete_tenant_oidc_config_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider): Path<String>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    match state
        .auth_client
        .delete_tenant_oidc_provider(
            principal.tenant_id,
            principal.role,
            &provider,
            &principal.username,
        )
        .await
    {
        Ok(providers) => axum::Json(serde_json::json!({"providers": providers})).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateMfaPolicyApiRequest {
    pub required: bool,
}

pub async fn update_mfa_policy_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<UpdateMfaPolicyApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !principal.role.at_least(Role::Admin) {
        return api_error(StatusCode::FORBIDDEN, "admin role required");
    }
    match state
        .auth_client
        .set_mfa_policy(principal.tenant_id, principal.role, request.required, &principal.username)
        .await
    {
        Ok(required) => axum::Json(serde_json::json!({ "required": required })).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn enroll_mfa_api(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match state.mfa_client.enroll(principal.tenant_id, &principal.username).await {
        Ok(enrollment) => (StatusCode::CREATED, axum::Json(enrollment)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct VerifyMfaApiRequest {
    pub code: String,
}

pub async fn verify_mfa_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<VerifyMfaApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if !valid_totp_code(&request.code) {
        return api_error(StatusCode::BAD_REQUEST, "code must be exactly six digits");
    }
    match state.mfa_client.verify(principal.tenant_id, &principal.username, &request.code).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct DisableMfaApiRequest {
    pub password: String,
}

pub async fn disable_mfa_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<DisableMfaApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if request.password.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "password is required");
    }
    match state
        .mfa_client
        .disable(principal.tenant_id, &principal.username, &request.password)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn valid_totp_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit())
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordApiRequest {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

pub fn validate_password_confirmation(
    new_password: &str,
    confirm_password: &str,
) -> Result<(), Response> {
    if new_password != confirm_password {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "new password and confirmation do not match",
        ));
    }
    if new_password.is_empty() || new_password.len() > 1024 {
        return Err(api_error(StatusCode::BAD_REQUEST, "new password must not be empty"));
    }
    Ok(())
}

pub async fn change_password_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ChangePasswordApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) =
        validate_password_confirmation(&request.new_password, &request.confirm_password)
    {
        return error;
    }
    if request.current_password.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "current password is required");
    }
    match state
        .users_client
        .change_password(
            principal.tenant_id,
            &principal.username,
            &request.current_password,
            &request.new_password,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

fn api_csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub async fn export_data_csv_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataApiQuery>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let base_filter = match data_filter(&query) {
        Ok(filter) => filter,
        Err(error) => return error,
    };
    const PAGE_SIZE: i64 = 100;
    const MAX_PAGES: usize = 20;
    let mut records = Vec::new();
    let mut has_more = false;
    for page in 0..MAX_PAGES {
        let mut filter = base_filter.clone();
        filter.limit = PAGE_SIZE;
        filter.offset = page as i64 * PAGE_SIZE;
        let result = match state.stats_client.search_records(principal.tenant_id, &filter).await {
            Ok(result) => result,
            Err(error) => return api_error(StatusCode::BAD_GATEWAY, error.to_string()),
        };
        has_more = result.has_more;
        records.extend(result.records);
        if !has_more {
            break;
        }
    }
    let mut csv = String::from("id,connector_id,source_type,ingested_at,normalized,raw_payload\n");
    for record in records {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            record.id,
            api_csv_escape(&record.connector_id),
            api_csv_escape(&record.source_type),
            record.ingested_at.to_rfc3339(),
            record.is_normalized(),
            api_csv_escape(&record.raw_payload.to_string()),
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"data-export-{}.csv\"", principal.tenant_id)
            .parse()
            .unwrap(),
    );
    if has_more {
        response_headers
            .insert("x-next-offset", (MAX_PAGES as i64 * PAGE_SIZE).to_string().parse().unwrap());
    }
    (response_headers, csv).into_response()
}

pub async fn update_incident_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(mut incident): axum::Json<common::Incident>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if incident.id != id || incident.tenant_id != principal.tenant_id {
        return api_error(StatusCode::BAD_REQUEST, "incident id and tenant scope do not match");
    }
    incident.updated_at = Utc::now();
    match state
        .incidents_client
        .update_incident(principal.role, &principal.username, incident)
        .await
    {
        Ok(detail) => axum::Json(detail).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct IncidentStatusApiRequest {
    pub status: String,
}

async fn incident_for_api(
    state: &AppState,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<crate::IncidentDetail, Response> {
    match state.incidents_client.get_incident(tenant_id, id).await {
        Ok(Some(detail)) => Ok(detail),
        Ok(None) => Err(api_error(StatusCode::NOT_FOUND, "incident not found")),
        Err(error) => Err(api_error(StatusCode::BAD_GATEWAY, error.to_string())),
    }
}

pub async fn update_incident_status_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<IncidentStatusApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let status = match request.status.parse::<IncidentStatus>() {
        Ok(status) => status,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "unknown incident status"),
    };
    let detail = match incident_for_api(&state, principal.tenant_id, id).await {
        Ok(detail) => detail,
        Err(error) => return error,
    };
    let mut incident = detail.incident;
    incident.status = status;
    incident.resolved_at = (status == IncidentStatus::Resolved).then_some(Utc::now());
    incident.updated_at = Utc::now();
    match state
        .incidents_client
        .update_incident(principal.role, &principal.username, incident)
        .await
    {
        Ok(updated) => axum::Json(updated).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

pub async fn claim_incident_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    let detail = match incident_for_api(&state, principal.tenant_id, id).await {
        Ok(detail) => detail,
        Err(error) => return error,
    };
    if detail.incident.status == IncidentStatus::Resolved {
        return api_error(StatusCode::CONFLICT, "resolved incidents cannot be claimed");
    }
    if detail.incident.assigned_to.is_some() {
        return api_error(StatusCode::CONFLICT, "incident is already assigned");
    }
    let mut incident = detail.incident;
    incident.assigned_to = Some(principal.username.clone());
    incident.updated_at = Utc::now();
    match state
        .incidents_client
        .update_incident(principal.role, &principal.username, incident)
        .await
    {
        Ok(updated) => axum::Json(updated).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct IncidentNoteApiRequest {
    pub body: String,
}

pub fn validate_incident_note(body: &str) -> Result<(), Response> {
    let body = body.trim();
    if body.is_empty() || body.len() > 20_000 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "note must be between 1 and 20000 characters",
        ));
    }
    Ok(())
}

pub async fn add_incident_note_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<IncidentNoteApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if let Err(error) = validate_incident_note(&request.body) {
        return error;
    }
    match state
        .incidents_client
        .add_note(principal.role, &principal.username, principal.tenant_id, id, request.body.trim())
        .await
    {
        Ok(note) => (StatusCode::CREATED, axum::Json(note)).into_response(),
        Err(error) => api_error(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkIncidentApiRequest {
    pub ids: Vec<Uuid>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub assigned_to: Option<String>,
}

pub async fn bulk_update_incidents_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<BulkIncidentApiRequest>,
) -> Response {
    let principal = match principal(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = require_operator(&principal) {
        return error;
    }
    if request.ids.is_empty() || request.ids.len() > 500 {
        return api_error(StatusCode::BAD_REQUEST, "ids must contain between 1 and 500 incidents");
    }
    let status = match request.status.as_deref() {
        Some(value) => match value.parse::<IncidentStatus>() {
            Ok(status) => Some(status),
            Err(_) => return api_error(StatusCode::BAD_REQUEST, "unknown incident status"),
        },
        None => None,
    };
    if status.is_none() && request.assigned_to.is_none() {
        return api_error(StatusCode::BAD_REQUEST, "status or assigned_to is required");
    }
    if request.assigned_to.as_ref().is_some_and(|value| value.len() > 254) {
        return api_error(StatusCode::BAD_REQUEST, "assigned_to is too long");
    }
    let mut updated = 0usize;
    let mut failed = 0usize;
    for id in request.ids {
        let detail = match incident_for_api(&state, principal.tenant_id, id).await {
            Ok(detail) => detail,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let mut incident = detail.incident;
        if let Some(status) = status {
            incident.status = status;
            incident.resolved_at = (status == IncidentStatus::Resolved).then_some(Utc::now());
        }
        if let Some(assigned_to) = request.assigned_to.as_ref() {
            incident.assigned_to = Some(assigned_to.trim().to_string());
        }
        incident.updated_at = Utc::now();
        match state
            .incidents_client
            .update_incident(principal.role, &principal.username, incident)
            .await
        {
            Ok(_) => updated += 1,
            Err(_) => failed += 1,
        }
    }
    axum::Json(serde_json::json!({ "updated": updated, "failed": failed })).into_response()
}
