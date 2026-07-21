#[path = "incident_handlers_test.rs"]
#[cfg(test)]
mod incident_handlers_test;

use crate::events_client::EventDetail;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::{Incident, IncidentSeverity, IncidentStatus};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize, Default)]
pub struct IncidentsQuery {
    #[serde(default)]
    status: String,
}

struct IncidentRow {
    id: Uuid,
    title: String,
    severity: IncidentSeverity,
    status: IncidentStatus,
    event_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Template)]
#[template(path = "incidents.html")]
struct IncidentsTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    incidents: Vec<IncidentRow>,
    status: String,
    error: Option<String>,
    form_error: Option<String>,
}

pub async fn get_incidents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<IncidentsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let status_filter =
        if query.status.is_empty() { None } else { IncidentStatus::from_str(&query.status).ok() };

    match state.incidents_client.list_incidents(session.tenant_id, status_filter).await {
        Ok(details) => {
            let incidents = details
                .into_iter()
                .map(|d| IncidentRow {
                    id: d.incident.id,
                    title: d.incident.title,
                    severity: d.incident.severity,
                    status: d.incident.status,
                    event_count: d.event_ids.len(),
                    created_at: d.incident.created_at,
                })
                .collect();
            Html(
                IncidentsTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    incidents,
                    status: query.status,
                    error: None,
                    form_error: None,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            IncidentsTemplate {
                show_nav: true,
                is_admin,
                can_write,
                incidents: vec![],
                status: query.status,
                error: Some(e.to_string()),
                form_error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateIncidentForm {
    title: String,
    severity: String,
}

pub async fn post_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CreateIncidentForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let Ok(severity) = IncidentSeverity::from_str(&form.severity) else {
        return Redirect::to("/incidents").into_response();
    };
    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: session.tenant_id,
        title: form.title,
        summary: String::new(),
        severity,
        status: IncidentStatus::Open,
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };

    match state
        .incidents_client
        .create_incident(session.role, &session.username, incident, vec![])
        .await
    {
        Ok(detail) => Redirect::to(&format!("/incidents/{}", detail.incident.id)).into_response(),
        Err(_) => Redirect::to("/incidents").into_response(),
    }
}

struct LinkedEventRow {
    event: EventDetail,
}

#[derive(Template)]
#[template(path = "incident_detail.html")]
struct IncidentDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    incident: Option<Incident>,
    linked_events: Vec<LinkedEventRow>,
    error: Option<String>,
}

fn error_page(is_admin: bool, can_write: bool, message: String) -> Response {
    Html(
        IncidentDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            incident: None,
            linked_events: vec![],
            error: Some(message),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn get_incident_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let detail = match state.incidents_client.get_incident(session.tenant_id, id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return error_page(is_admin, can_write, "no incident found with this id".to_string())
        }
        Err(e) => return error_page(is_admin, can_write, e.to_string()),
    };

    let mut linked_events = Vec::with_capacity(detail.event_ids.len());
    for event_id in &detail.event_ids {
        if let Ok(Some(event)) =
            state.events_client.get_event(&session.bearer_token, *event_id).await
        {
            linked_events.push(LinkedEventRow { event });
        }
    }

    Html(
        IncidentDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            incident: Some(detail.incident),
            linked_events,
            error: None,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateIncidentForm {
    title: String,
    severity: String,
    status: String,
}

pub async fn post_update_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<UpdateIncidentForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let existing = match state.incidents_client.get_incident(session.tenant_id, id).await {
        Ok(Some(detail)) => detail.incident,
        _ => return Redirect::to(&format!("/incidents/{id}")).into_response(),
    };

    let Ok(severity) = IncidentSeverity::from_str(&form.severity) else {
        return Redirect::to(&format!("/incidents/{id}")).into_response();
    };
    let Ok(status) = IncidentStatus::from_str(&form.status) else {
        return Redirect::to(&format!("/incidents/{id}")).into_response();
    };

    let resolved_at =
        if status == IncidentStatus::Resolved { Some(chrono::Utc::now()) } else { None };
    let updated = Incident {
        title: form.title,
        severity,
        status,
        updated_at: chrono::Utc::now(),
        resolved_at,
        ..existing
    };

    let _ = state.incidents_client.update_incident(session.role, &session.username, updated).await;
    Redirect::to(&format!("/incidents/{id}")).into_response()
}

pub async fn post_unlink_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((incident_id, event_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let _ = state
        .incidents_client
        .unlink_event(session.role, &session.username, session.tenant_id, incident_id, event_id)
        .await;
    Redirect::to(&format!("/incidents/{incident_id}")).into_response()
}

/// `axum::extract::Form` doesn't collect repeated same-named fields (one checkbox per row, all
/// named `ids`) into a `Vec` — parsing the raw body as a flat list of `(key, value)` pairs
/// sidesteps that, same pattern as Sensors'/API Keys'/Retention Policies' bulk-action handlers
/// (ADR-0065/ADR-0095).
fn parse_ids(raw_body: &[u8]) -> Vec<Uuid> {
    let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body) else {
        return Vec::new();
    };
    pairs
        .into_iter()
        .filter(|(key, _)| key == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect()
}

/// POST /events/create-incident — the "select Events → Create Incident" bulk action (ADR-0111).
pub async fn post_create_incident_from_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let event_ids = parse_ids(&body);
    if event_ids.is_empty() {
        return Redirect::to("/events").into_response();
    }

    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: session.tenant_id,
        title: format!("Incident from {} selected event(s)", event_ids.len()),
        summary: String::new(),
        severity: IncidentSeverity::Medium,
        status: IncidentStatus::Open,
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };

    match state
        .incidents_client
        .create_incident(session.role, &session.username, incident, event_ids)
        .await
    {
        Ok(detail) => Redirect::to(&format!("/incidents/{}", detail.incident.id)).into_response(),
        Err(_) => Redirect::to("/events").into_response(),
    }
}
