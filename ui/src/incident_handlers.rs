#[path = "incident_handlers_test.rs"]
#[cfg(test)]
mod incident_handlers_test;

use crate::audit_log_client::AuditLogEntry;
use crate::events_client::EventDetail;
use crate::ontology_client;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::{Incident, IncidentSeverity, IncidentStatus, SavedSearchQuery};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize, Default)]
pub struct IncidentsQuery {
    #[serde(default)]
    page: i64,
    #[serde(default)]
    status: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    notice: String,
    #[serde(default)]
    view: String,
    #[serde(default)]
    sla: String,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct WorkReturnContext {
    #[serde(default)]
    q: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    sla: String,
    #[serde(default)]
    focus: String,
}

fn work_return_redirect(notice: &str, context: &WorkReturnContext) -> Redirect {
    let params = [
        ("notice", notice.to_string()),
        ("q", context.q.clone()),
        ("severity", context.severity.clone()),
        ("sla", context.sla.clone()),
        ("focus", context.focus.clone()),
    ];
    let params = params.into_iter().filter(|(_, value)| !value.is_empty()).collect::<Vec<_>>();
    Redirect::to(&format!("/work?{}", serde_urlencoded::to_string(params).unwrap_or_default()))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct IncidentDetailQuery {
    #[serde(default)]
    notice: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default, Clone)]
struct SavedIncidentFilter {
    #[serde(default)]
    q: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    view: String,
    #[serde(default)]
    sla: String,
}

struct SavedIncidentView {
    id: Uuid,
    name: String,
    load_url: String,
}

fn to_saved_incident_view(query: SavedSearchQuery) -> SavedIncidentView {
    let filter: SavedIncidentFilter = serde_json::from_value(query.filter).unwrap_or_default();
    let load_url =
        format!("/incidents?{}", serde_urlencoded::to_string(&filter).unwrap_or_default());
    SavedIncidentView { id: query.id, name: query.name, load_url }
}

struct IncidentRow {
    id: Uuid,
    title: String,
    summary: String,
    signal_context: String,
    group_keys: Vec<String>,
    severity: IncidentSeverity,
    status: IncidentStatus,
    assigned_to: Option<String>,
    event_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    sla_state: String,
    sla_label: String,
    sla_detail: String,
}

struct IncidentCorrelationCluster {
    group_key: String,
    case_count: usize,
    signal_count: usize,
    severity: String,
    href: String,
}

struct IncidentEventContext {
    event_type: String,
    group_key: String,
    status: String,
}

struct IncidentMetric {
    label: String,
    count: usize,
    percent: i32,
    href: String,
    tone: String,
}

struct IncidentSlaMatrixCell {
    label: String,
    count: usize,
    href: String,
    tone: String,
}

struct IncidentSlaMatrixRow {
    severity: String,
    cells: Vec<IncidentSlaMatrixCell>,
}

#[derive(Template)]
#[template(path = "incidents.html")]
struct IncidentsTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    incidents: Vec<IncidentRow>,
    total_count: usize,
    matching_count: usize,
    page: i64,
    has_prev: bool,
    has_next: bool,
    open_count: usize,
    acknowledged_count: usize,
    board_open_count: usize,
    board_acknowledged_count: usize,
    board_resolved_count: usize,
    critical_count: usize,
    sla_breached_count: usize,
    status: String,
    severity: String,
    owner: String,
    assignee_options: Vec<AssigneeOption>,
    q: String,
    sort: String,
    dir: String,
    sla: String,
    notice: String,
    view: String,
    error: Option<String>,
    form_error: Option<String>,
    saved_views: Vec<SavedIncidentView>,
    severity_metrics: Vec<IncidentMetric>,
    sla_metrics: Vec<IncidentMetric>,
    sla_matrix: Vec<IncidentSlaMatrixRow>,
    correlation_clusters: Vec<IncidentCorrelationCluster>,
}

fn incident_correlation_clusters(incidents: &[IncidentRow]) -> Vec<IncidentCorrelationCluster> {
    let mut grouped = std::collections::HashMap::<
        String,
        (std::collections::HashSet<Uuid>, usize, IncidentSeverity),
    >::new();
    for incident in incidents {
        let groups = incident
            .group_keys
            .iter()
            .filter(|group| !group.is_empty())
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let signal_share = (incident.event_count / groups.len().max(1)).max(1);
        for group_key in groups {
            let entry = grouped
                .entry(group_key)
                .or_insert_with(|| (std::collections::HashSet::new(), 0, incident.severity));
            entry.0.insert(incident.id);
            entry.1 += signal_share;
            if severity_rank(incident.severity) > severity_rank(entry.2) {
                entry.2 = incident.severity;
            }
        }
    }
    let mut clusters = grouped
        .into_iter()
        .filter(|(_, (case_ids, _, _))| case_ids.len() > 1)
        .map(|(group_key, (case_ids, signal_count, severity))| IncidentCorrelationCluster {
            href: format!(
                "/incidents?{}",
                serde_urlencoded::to_string([("q", group_key.as_str())]).unwrap_or_default()
            ),
            group_key,
            case_count: case_ids.len(),
            signal_count,
            severity: severity.to_string(),
        })
        .collect::<Vec<_>>();
    clusters.sort_by(|left, right| {
        right
            .case_count
            .cmp(&left.case_count)
            .then_with(|| right.signal_count.cmp(&left.signal_count))
            .then_with(|| left.group_key.cmp(&right.group_key))
    });
    clusters
}

fn incident_sla_matrix(incidents: &[IncidentRow]) -> Vec<IncidentSlaMatrixRow> {
    [
        ("Critical", "critical", "risk"),
        ("High", "high", "warning"),
        ("Medium", "medium", "neutral"),
        ("Low", "low", "good"),
    ]
    .into_iter()
    .map(|(label, severity, _tone)| IncidentSlaMatrixRow {
        severity: label.to_string(),
        cells: [
            ("Breached", "breached", "risk"),
            ("At risk", "at-risk", "warning"),
            ("On track", "on-track", "good"),
            ("Resolved", "resolved", "neutral"),
        ]
        .into_iter()
        .map(|(cell_label, sla, tone)| IncidentSlaMatrixCell {
            label: cell_label.to_string(),
            count: incidents
                .iter()
                .filter(|incident| {
                    incident.severity.to_string() == severity && incident.sla_state == sla
                })
                .count(),
            href: format!("/incidents?severity={severity}&sla={sla}"),
            tone: tone.to_string(),
        })
        .collect(),
    })
    .collect()
}

fn severity_rank(severity: IncidentSeverity) -> u8 {
    match severity {
        IncidentSeverity::Critical => 4,
        IncidentSeverity::High => 3,
        IncidentSeverity::Medium => 2,
        IncidentSeverity::Low => 1,
    }
}

const INCIDENT_PAGE_SIZE: usize = 25;

pub(crate) struct IncidentSlaView {
    pub(crate) state: String,
    pub(crate) label: String,
    pub(crate) detail: String,
}

pub(crate) fn incident_sla(
    incident: &Incident,
    now: chrono::DateTime<chrono::Utc>,
) -> IncidentSlaView {
    let target = match incident.severity {
        IncidentSeverity::Critical => chrono::Duration::minutes(15),
        IncidentSeverity::High => chrono::Duration::hours(1),
        IncidentSeverity::Medium => chrono::Duration::hours(4),
        IncidentSeverity::Low => chrono::Duration::hours(24),
    };
    let end = incident.resolved_at.unwrap_or(now);
    let elapsed = (end - incident.created_at).num_seconds().max(0);
    let target_seconds = target.num_seconds();
    if incident.status == IncidentStatus::Resolved {
        let state = if elapsed > target_seconds { "breached" } else { "resolved" };
        return IncidentSlaView {
            state: state.to_string(),
            label: if state == "breached" {
                "Resolved after SLA".to_string()
            } else {
                "Resolved within SLA".to_string()
            },
            detail: format_duration(elapsed),
        };
    }
    let remaining = target_seconds - elapsed;
    let state = if remaining <= 0 {
        "breached"
    } else if remaining <= (target_seconds / 5).max(60) {
        "at-risk"
    } else {
        "on-track"
    };
    let detail = if remaining <= 0 {
        format!("{} past target", format_duration(-remaining))
    } else {
        format!("{} remaining", format_duration(remaining))
    };
    IncidentSlaView {
        state: state.to_string(),
        label: match state {
            "breached" => "SLA breached",
            "at-risk" => "At risk",
            _ => "On track",
        }
        .to_string(),
        detail,
    }
}

fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

fn incident_matches_search(title: &str, summary: &str, signal_context: &str, search: &str) -> bool {
    let search = search.trim().to_ascii_lowercase();
    search.is_empty()
        || title.to_ascii_lowercase().contains(&search)
        || summary.to_ascii_lowercase().contains(&search)
        || signal_context.to_ascii_lowercase().contains(&search)
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
    let assignee_options = incident_assignee_options(&state, &session).await;
    let saved_views = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|query| {
            query.filter.get("view_kind").and_then(serde_json::Value::as_str) == Some("incidents")
        })
        .map(to_saved_incident_view)
        .collect::<Vec<_>>();

    let active_status_filter = query.status == "active";
    let status_filter = if query.status.is_empty() || active_status_filter {
        None
    } else {
        IncidentStatus::from_str(&query.status).ok()
    };
    let severity_filter = if query.severity.is_empty() {
        None
    } else {
        IncidentSeverity::from_str(&query.severity).ok()
    };

    match state.incidents_client.list_incidents(session.tenant_id, status_filter).await {
        Ok(details) => {
            let event_context = state
                .events_client
                .list_events(&session.bearer_token, 1000, 0, None, None)
                .await
                .map(|page| {
                    page.events
                        .into_iter()
                        .map(|event| {
                            (
                                event.id,
                                IncidentEventContext {
                                    event_type: event.event_type,
                                    group_key: event.group_key,
                                    status: event.status,
                                },
                            )
                        })
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .unwrap_or_default();
            let total_count = details.len();
            let now = chrono::Utc::now();
            let sla_filter = match query.sla.as_str() {
                "breached" | "at-risk" | "on-track" | "resolved" => Some(query.sla.as_str()),
                _ => None,
            };
            let mut incidents: Vec<IncidentRow> = details
                .into_iter()
                .map(|d| {
                    let sla = incident_sla(&d.incident, now);
                    let signal_context = d
                        .event_ids
                        .iter()
                        .filter_map(|event_id| event_context.get(event_id))
                        .map(|context| {
                            format!(
                                "{} {} {}",
                                context.event_type, context.group_key, context.status
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    let group_keys = d
                        .event_ids
                        .iter()
                        .filter_map(|event_id| event_context.get(event_id))
                        .map(|context| context.group_key.clone())
                        .filter(|group_key| !group_key.is_empty())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    IncidentRow {
                        id: d.incident.id,
                        title: d.incident.title,
                        summary: d.incident.summary,
                        signal_context,
                        group_keys,
                        severity: d.incident.severity,
                        status: d.incident.status,
                        assigned_to: d.incident.assigned_to,
                        event_count: d.event_ids.len(),
                        created_at: d.incident.created_at,
                        sla_state: sla.state,
                        sla_label: sla.label,
                        sla_detail: sla.detail,
                    }
                })
                .collect();
            let search = query.q.trim().to_ascii_lowercase();
            let owner_filter = query.owner.trim().to_ascii_lowercase();
            incidents.retain(|incident| {
                (!active_status_filter || incident.status != IncidentStatus::Resolved)
                    && severity_filter.is_none_or(|severity| incident.severity == severity)
                    && sla_filter.is_none_or(|sla| incident.sla_state == sla)
                    && (owner_filter.is_empty()
                        || incident
                            .assigned_to
                            .as_deref()
                            .map(|owner| owner.to_ascii_lowercase() == owner_filter)
                            .unwrap_or(false))
                    && incident_matches_search(
                        &incident.title,
                        &incident.summary,
                        &incident.signal_context,
                        &search,
                    )
            });
            // The queue is an investigation surface, so its headline posture must use the
            // same filtered set as the board, matrix, and metric bars. `total_count` remains
            // the workspace-wide context shown beneath the active-case KPI.
            let open_count = incidents
                .iter()
                .filter(|incident| incident.status != IncidentStatus::Resolved)
                .count();
            let acknowledged_count = incidents
                .iter()
                .filter(|incident| incident.status == IncidentStatus::Acknowledged)
                .count();
            let critical_count = incidents
                .iter()
                .filter(|incident| {
                    incident.status != IncidentStatus::Resolved
                        && incident.severity == IncidentSeverity::Critical
                })
                .count();
            let sla_breached_count =
                incidents.iter().filter(|incident| incident.sla_state == "breached").count();
            incidents.sort_by(|left, right| {
                let ordering = match query.sort.as_str() {
                    "title" => {
                        left.title.to_ascii_lowercase().cmp(&right.title.to_ascii_lowercase())
                    }
                    "severity" => severity_rank(left.severity).cmp(&severity_rank(right.severity)),
                    "status" => left.status.to_string().cmp(&right.status.to_string()),
                    "events" => left.event_count.cmp(&right.event_count),
                    _ => left.created_at.cmp(&right.created_at),
                };
                if query.dir.eq_ignore_ascii_case("asc") {
                    ordering
                } else {
                    ordering.reverse()
                }
            });
            let matching_count = incidents.len();
            let correlation_clusters = incident_correlation_clusters(&incidents);
            let metric =
                |label: &str, count: usize, total: usize, href: &str, tone: &str| IncidentMetric {
                    label: label.to_string(),
                    count,
                    percent: if total == 0 { 0 } else { (count * 100 / total) as i32 },
                    href: href.to_string(),
                    tone: tone.to_string(),
                };
            let severity_metrics = [
                ("Critical", IncidentSeverity::Critical, "critical", "risk"),
                ("High", IncidentSeverity::High, "high", "warning"),
                ("Medium", IncidentSeverity::Medium, "medium", "neutral"),
                ("Low", IncidentSeverity::Low, "low", "good"),
            ]
            .into_iter()
            .map(|(label, severity, key, tone)| {
                metric(
                    label,
                    incidents.iter().filter(|item| item.severity == severity).count(),
                    matching_count,
                    &format!("/incidents?severity={key}"),
                    tone,
                )
            })
            .collect::<Vec<_>>();
            let sla_metrics = [
                ("Breached", "breached", "risk"),
                ("At risk", "at-risk", "warning"),
                ("On track", "on-track", "good"),
                ("Resolved", "resolved", "neutral"),
            ]
            .into_iter()
            .map(|(label, key, tone)| {
                metric(
                    label,
                    incidents.iter().filter(|item| item.sla_state == key).count(),
                    matching_count,
                    &format!("/incidents?sla={key}"),
                    tone,
                )
            })
            .collect::<Vec<_>>();
            let sla_matrix = incident_sla_matrix(&incidents);
            let board_open_count =
                incidents.iter().filter(|item| item.status == IncidentStatus::Open).count();
            let board_acknowledged_count =
                incidents.iter().filter(|item| item.status == IncidentStatus::Acknowledged).count();
            let board_resolved_count =
                incidents.iter().filter(|item| item.status == IncidentStatus::Resolved).count();
            let page = query.page.max(0);
            let start = (page as usize).saturating_mul(INCIDENT_PAGE_SIZE);
            let has_prev = page > 0 && start < matching_count.saturating_add(INCIDENT_PAGE_SIZE);
            let has_next = start.saturating_add(INCIDENT_PAGE_SIZE) < matching_count;
            incidents = incidents.into_iter().skip(start).take(INCIDENT_PAGE_SIZE).collect();
            Html(
                IncidentsTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    incidents,
                    total_count,
                    matching_count,
                    page,
                    has_prev,
                    has_next,
                    open_count,
                    acknowledged_count,
                    board_open_count,
                    board_acknowledged_count,
                    board_resolved_count,
                    critical_count,
                    sla_breached_count,
                    status: query.status,
                    severity: query.severity,
                    owner: query.owner,
                    assignee_options,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                    sla: query.sla,
                    notice: query.notice,
                    view: query.view,
                    error: None,
                    form_error: None,
                    saved_views,
                    severity_metrics,
                    sla_metrics,
                    sla_matrix,
                    correlation_clusters,
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
                total_count: 0,
                matching_count: 0,
                page: query.page.max(0),
                has_prev: false,
                has_next: false,
                open_count: 0,
                acknowledged_count: 0,
                board_open_count: 0,
                board_acknowledged_count: 0,
                board_resolved_count: 0,
                critical_count: 0,
                sla_breached_count: 0,
                status: query.status,
                severity: query.severity,
                owner: query.owner,
                assignee_options,
                q: query.q,
                sort: query.sort,
                dir: query.dir,
                sla: query.sla,
                notice: query.notice,
                view: query.view,
                error: Some(e.to_string()),
                form_error: None,
                saved_views,
                severity_metrics: vec![],
                sla_metrics: vec![],
                sla_matrix: vec![],
                correlation_clusters: vec![],
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

/// GET /incidents/export.csv — exports the complete filtered case queue rather than only the
/// visible page. The fields mirror the queue's operational scope and keep the evidence links
/// recoverable for an executive or incident-command handoff.
pub async fn get_incidents_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<IncidentsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let active_status_filter = query.status == "active";
    let status_filter = if query.status.is_empty() || active_status_filter {
        None
    } else {
        IncidentStatus::from_str(&query.status).ok()
    };
    let severity_filter = if query.severity.is_empty() {
        None
    } else {
        IncidentSeverity::from_str(&query.severity).ok()
    };
    let details =
        match state.incidents_client.list_incidents(session.tenant_id, status_filter).await {
            Ok(details) => details,
            Err(error) => {
                return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
            }
        };
    let search = query.q.trim().to_ascii_lowercase();
    let owner_filter = query.owner.trim().to_ascii_lowercase();
    let sla_filter = match query.sla.as_str() {
        "breached" | "at-risk" | "on-track" | "resolved" => Some(query.sla.as_str()),
        _ => None,
    };
    let event_context = state
        .events_client
        .list_events(&session.bearer_token, 1000, 0, None, None)
        .await
        .map(|page| {
            page.events
                .into_iter()
                .map(|event| {
                    (event.id, format!("{} {} {}", event.event_type, event.group_key, event.status))
                })
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let mut rows = details
        .into_iter()
        .filter_map(|detail| {
            let sla = incident_sla(&detail.incident, now);
            let incident = detail.incident;
            let signal_context = detail
                .event_ids
                .iter()
                .filter_map(|event_id| event_context.get(event_id))
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if (active_status_filter && incident.status == IncidentStatus::Resolved)
                || severity_filter.is_some_and(|severity| incident.severity != severity)
                || sla_filter.is_some_and(|state| sla.state != state)
                || (!owner_filter.is_empty()
                    && incident
                        .assigned_to
                        .as_deref()
                        .map(|owner| owner.to_ascii_lowercase() != owner_filter)
                        .unwrap_or(true))
                || (!incident_matches_search(
                    &incident.title,
                    &incident.summary,
                    &signal_context,
                    &search,
                ))
            {
                return None;
            }
            Some((incident, detail.event_ids.len()))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|(left, left_events), (right, right_events)| {
        let ordering = match query.sort.as_str() {
            "title" => left.title.to_ascii_lowercase().cmp(&right.title.to_ascii_lowercase()),
            "severity" => severity_rank(left.severity).cmp(&severity_rank(right.severity)),
            "status" => left.status.to_string().cmp(&right.status.to_string()),
            "events" => left_events.cmp(right_events),
            _ => left.created_at.cmp(&right.created_at),
        };
        if query.dir.eq_ignore_ascii_case("asc") {
            ordering
        } else {
            ordering.reverse()
        }
    });
    let mut csv = String::from(
        "id,title,summary,severity,status,owner,event_count,created_at,updated_at,sla_state,sla_label,sla_detail\n",
    );
    for (incident, event_count) in rows {
        let sla = incident_sla(&incident, now);
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            incident.id,
            csv_escape(&incident.title),
            csv_escape(&incident.summary),
            incident.severity,
            incident.status,
            csv_escape(incident.assigned_to.as_deref().unwrap_or("")),
            event_count,
            incident.created_at.to_rfc3339(),
            incident.updated_at.to_rfc3339(),
            sla.state,
            csv_escape(&sla.label),
            csv_escape(&sla.detail)
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"incidents-{}.csv\"", session.tenant_id).parse().unwrap(),
    );
    (response_headers, csv).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveIncidentViewForm {
    name: String,
    q: String,
    status: String,
    severity: String,
    owner: String,
    sort: String,
    dir: String,
    view: String,
    sla: String,
}

fn incident_view_redirect(form: &SaveIncidentViewForm, notice: &str) -> Redirect {
    let query = serde_urlencoded::to_string([
        ("q", form.q.clone()),
        ("status", form.status.clone()),
        ("severity", form.severity.clone()),
        ("sla", form.sla.clone()),
        ("owner", form.owner.clone()),
        ("sort", form.sort.clone()),
        ("dir", form.dir.clone()),
        ("view", form.view.clone()),
        ("notice", notice.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    Redirect::to(&format!("/incidents?{query}"))
}

pub async fn post_save_incident_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveIncidentViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let filter = serde_json::json!({
        "view_kind": "incidents",
        "q": form.q.clone(),
        "status": form.status.clone(),
        "severity": form.severity.clone(),
        "owner": form.owner.clone(),
        "sort": form.sort.clone(),
        "dir": form.dir.clone(),
        "view": form.view.clone(),
        "sla": form.sla.clone(),
    });
    match state.saved_search_queries_client.create(session.tenant_id, &form.name, filter).await {
        Ok(_) => incident_view_redirect(&form, "view_saved").into_response(),
        Err(_) => incident_view_redirect(&form, "view_failed").into_response(),
    }
}

pub async fn post_delete_incident_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(()) => Redirect::to("/incidents").into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateIncidentForm {
    title: String,
    severity: String,
    #[serde(default)]
    summary: String,
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
    if form.title.trim().is_empty() || form.title.len() > 500 || form.summary.len() > 2_000 {
        return Redirect::to("/incidents").into_response();
    }

    let Ok(severity) = IncidentSeverity::from_str(&form.severity) else {
        return Redirect::to("/incidents").into_response();
    };
    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: session.tenant_id,
        title: form.title,
        summary: form.summary.trim().to_string(),
        severity,
        status: IncidentStatus::Open,
        assigned_to: Some(session.username.clone()),
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

struct IncidentEvidenceRecordView {
    id: Uuid,
    event_count: usize,
    event_types: String,
}

struct ImpactObjectView {
    id: Uuid,
    type_name: String,
    label: String,
    status: String,
    event_count: usize,
    action_count: usize,
    last_action: Option<String>,
    last_outcome: Option<String>,
    last_action_at: Option<chrono::DateTime<chrono::Utc>>,
    actions: Vec<CaseActionView>,
}

struct ImpactRelationshipView {
    relationship: String,
    source_id: Uuid,
    source_type_name: String,
    source_label: String,
    target_id: Uuid,
    target_type_name: String,
    target_label: String,
}

struct CaseGraphNode {
    id: Uuid,
    type_name: String,
    label: String,
    x: i32,
    y: i32,
}

struct CaseGraphEdge {
    relationship: String,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

fn build_case_graph(
    impact_objects: &[ImpactObjectView],
    relationships: &[ImpactRelationshipView],
) -> (Vec<CaseGraphNode>, Vec<CaseGraphEdge>) {
    let mut node_data = std::collections::BTreeMap::<Uuid, (String, String)>::new();
    for object in impact_objects {
        node_data.insert(object.id, (object.type_name.clone(), object.label.clone()));
    }
    for relation in relationships {
        node_data
            .entry(relation.source_id)
            .or_insert_with(|| (relation.source_type_name.clone(), relation.source_label.clone()));
        node_data
            .entry(relation.target_id)
            .or_insert_with(|| (relation.target_type_name.clone(), relation.target_label.clone()));
    }
    let node_ids = node_data.keys().copied().collect::<Vec<_>>();
    let positions = node_ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let column = (index % 3) as i32;
            let row = (index / 3) as i32;
            (*id, (170 + column * 300, 95 + row * 150))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let nodes = node_ids
        .iter()
        .filter_map(|id| {
            let (type_name, label) = node_data.get(id)?.clone();
            let (x, y) = positions[id];
            Some(CaseGraphNode { id: *id, type_name, label, x, y })
        })
        .collect::<Vec<_>>();
    let edges = relationships
        .iter()
        .filter_map(|relation| {
            let (x1, y1) = positions.get(&relation.source_id).copied()?;
            let (x2, y2) = positions.get(&relation.target_id).copied()?;
            Some(CaseGraphEdge { relationship: relation.relationship.clone(), x1, y1, x2, y2 })
        })
        .collect::<Vec<_>>();
    (nodes, edges)
}

fn object_display_label(object: &common::ontology::Object) -> String {
    object
        .properties
        .get("name")
        .or_else(|| object.properties.get("subject"))
        .or_else(|| object.properties.get("title"))
        .or_else(|| object.properties.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Untitled object")
        .to_string()
}

fn impact_relationships(
    objects: &[common::ontology::Object],
    object_type_names: &std::collections::HashMap<Uuid, String>,
    link_types: &[common::ontology::LinkType],
    links: &[common::ontology::Link],
    direct_ids: &std::collections::HashSet<Uuid>,
    max_depth: usize,
) -> Vec<ImpactRelationshipView> {
    let object_by_id = objects
        .iter()
        .map(|object| (object.id, object))
        .collect::<std::collections::HashMap<_, _>>();
    let type_names = link_types
        .iter()
        .map(|link_type| (link_type.id, link_type.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen_objects = direct_ids.clone();
    let mut frontier = direct_ids.iter().copied().collect::<Vec<_>>();
    let mut seen_links = std::collections::HashSet::new();
    let mut result = Vec::new();
    for _ in 0..max_depth {
        let mut next = Vec::new();
        for link in links {
            let to = if frontier.contains(&link.source_object_id) {
                link.target_object_id
            } else if frontier.contains(&link.target_object_id) {
                link.source_object_id
            } else {
                continue;
            };
            if !seen_links.insert(link.id) {
                continue;
            }
            let (Some(source), Some(target)) = (
                object_by_id.get(&link.source_object_id),
                object_by_id.get(&link.target_object_id),
            ) else {
                continue;
            };
            result.push(ImpactRelationshipView {
                relationship: type_names
                    .get(&link.link_type_id)
                    .cloned()
                    .unwrap_or_else(|| "related".to_string()),
                source_id: source.id,
                source_type_name: object_type_names
                    .get(&source.object_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Modeled object".to_string()),
                source_label: object_display_label(source),
                target_id: target.id,
                target_type_name: object_type_names
                    .get(&target.object_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Modeled object".to_string()),
                target_label: object_display_label(target),
            });
            if seen_objects.insert(to) {
                next.push(to);
            }
            if result.len() >= 16 {
                return result;
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    result
}

struct CaseActionView {
    id: Uuid,
    name: String,
    eligible: bool,
    parameter_fields: Vec<CaseActionParameterField>,
    preconditions: String,
    effect_definition: String,
}

struct CaseActionParameterField {
    name: String,
    field_type: String,
    required: bool,
    default_value: String,
}

fn case_action_parameter_fields(schema: &serde_json::Value) -> Vec<CaseActionParameterField> {
    let mut fields = schema
        .as_object()
        .into_iter()
        .flat_map(|items| items.iter())
        .map(|(name, definition)| {
            let field_type = definition
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string")
                .to_string();
            let default_value = match field_type.as_str() {
                "boolean" => "false",
                "number" | "integer" => "0",
                "array" => "[]",
                "object" => "{}",
                _ => "",
            }
            .to_string();
            CaseActionParameterField {
                name: name.clone(),
                field_type,
                required: definition
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                default_value,
            }
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.name.cmp(&right.name));
    fields
}

struct GovernedResponseView {
    id: Uuid,
    action_name: String,
    outcome: String,
    review_status: String,
    review_assignee: Option<String>,
    review_stale: bool,
    target_label: String,
    executed_at: chrono::DateTime<chrono::Utc>,
    event_id: Option<Uuid>,
    incident_id: Option<Uuid>,
}

struct IncidentResponseMetric {
    label: String,
    count: usize,
    percent: i32,
    tone: String,
    href: String,
}

fn response_outcome_metrics(responses: &[GovernedResponseView]) -> Vec<IncidentResponseMetric> {
    let total = responses.len();
    let completed = responses
        .iter()
        .filter(|response| response.outcome.eq_ignore_ascii_case("completed"))
        .count();
    let rejected = responses
        .iter()
        .filter(|response| response.outcome.to_ascii_lowercase().starts_with("rejected"))
        .count();
    let metric = |label: &str, count: usize, tone: &str, href: &str| IncidentResponseMetric {
        label: label.to_string(),
        count,
        percent: if total == 0 { 0 } else { (count * 100 / total) as i32 },
        tone: tone.to_string(),
        href: href.to_string(),
    };
    vec![
        metric("Completed", completed, "good", "/actions?outcome=completed"),
        metric("Rejected", rejected, "risk", "/actions?outcome=review"),
        metric(
            "Other",
            total.saturating_sub(completed + rejected),
            "neutral",
            "/actions?outcome=review",
        ),
    ]
}

#[derive(Template)]
#[template(path = "incident_detail.html")]
struct IncidentDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    incident: Option<Incident>,
    sla: Option<IncidentSlaView>,
    linked_events: Vec<LinkedEventRow>,
    evidence_records: Vec<IncidentEvidenceRecordView>,
    impact_objects: Vec<ImpactObjectView>,
    impact_relationships: Vec<ImpactRelationshipView>,
    case_graph_nodes: Vec<CaseGraphNode>,
    case_graph_edges: Vec<CaseGraphEdge>,
    governed_responses: Vec<GovernedResponseView>,
    timeline: Vec<IncidentTimelineEntry>,
    timeline_metrics: Vec<IncidentTimelineMetric>,
    modeled_event_count: usize,
    response_completed_count: usize,
    response_review_count: usize,
    response_outcome_metrics: Vec<IncidentResponseMetric>,
    notes: Vec<common::IncidentNote>,
    activity: Vec<IncidentActivityRow>,
    assignee_options: Vec<AssigneeOption>,
    error: Option<String>,
    notice: String,
}

struct IncidentActivityRow {
    change_type: String,
    actor: String,
    changed_at: chrono::DateTime<chrono::Utc>,
    summary: String,
}

struct IncidentTimelineEntry {
    kind: String,
    actor: String,
    at: chrono::DateTime<chrono::Utc>,
    summary: String,
    detail: String,
    href: Option<String>,
    is_failure: bool,
}

struct IncidentTimelineMetric {
    label: String,
    detail: String,
    elapsed: String,
    width_pct: usize,
    href: Option<String>,
    tone: String,
}

fn case_duration(from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> String {
    let seconds = (to - from).num_seconds().max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    } else {
        format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3_600)
    }
}

fn audit_value_summary(entry: &AuditLogEntry) -> String {
    let after = entry.after.as_object();
    let before = entry.before.as_ref().and_then(serde_json::Value::as_object);
    let fields = ["status", "severity", "assigned_to", "title"];
    let changed = fields
        .iter()
        .filter_map(|field| {
            let old = before.and_then(|value| value.get(*field));
            let new = after.and_then(|value| value.get(*field));
            (old != new && (old.is_some() || new.is_some())).then(|| {
                let format_value = |value: Option<&serde_json::Value>| match value {
                    Some(serde_json::Value::String(value)) => value.clone(),
                    Some(serde_json::Value::Null) | None => "none".to_string(),
                    Some(value) => value.to_string(),
                };
                format!("{}: {} → {}", field, format_value(old), format_value(new))
            })
        })
        .collect::<Vec<_>>();
    if changed.is_empty() {
        "Case record updated".to_string()
    } else {
        changed.join(" · ")
    }
}

fn activity_rows(entries: Vec<AuditLogEntry>) -> Vec<IncidentActivityRow> {
    let mut rows = entries
        .into_iter()
        .map(|entry| IncidentActivityRow {
            change_type: entry.change_type.clone(),
            actor: entry.actor.clone(),
            changed_at: entry.changed_at,
            summary: audit_value_summary(&entry),
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.changed_at));
    rows
}

struct AssigneeOption {
    username: String,
    role: String,
}

async fn incident_assignee_options(
    state: &AppState,
    session: &crate::Session,
) -> Vec<AssigneeOption> {
    if session.role.at_least(common::Role::Admin) {
        if let Ok(users) = state.users_client.list_users(session.tenant_id, session.role).await {
            return users
                .into_iter()
                .map(|user| AssigneeOption { username: user.username, role: user.role.to_string() })
                .collect();
        }
    }
    vec![AssigneeOption { username: session.username.clone(), role: session.role.to_string() }]
}

fn error_page(is_admin: bool, can_write: bool, message: String) -> Response {
    Html(
        IncidentDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            incident: None,
            sla: None,
            linked_events: vec![],
            evidence_records: vec![],
            impact_objects: vec![],
            impact_relationships: vec![],
            case_graph_nodes: vec![],
            case_graph_edges: vec![],
            governed_responses: vec![],
            timeline: vec![],
            timeline_metrics: vec![],
            modeled_event_count: 0,
            response_completed_count: 0,
            response_review_count: 0,
            response_outcome_metrics: vec![],
            notes: vec![],
            activity: vec![],
            assignee_options: vec![],
            error: Some(message),
            notice: String::new(),
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
    Query(query): Query<IncidentDetailQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);
    let assignee_options = incident_assignee_options(&state, &session).await;

    let detail = match state.incidents_client.get_incident(session.tenant_id, id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return error_page(is_admin, can_write, "no incident found with this id".to_string())
        }
        Err(e) => return error_page(is_admin, can_write, e.to_string()),
    };

    let incident = detail.incident;
    let notes = detail.notes;
    let mut linked_events = Vec::with_capacity(detail.event_ids.len());
    for event_id in &detail.event_ids {
        if let Ok(Some(event)) =
            state.events_client.get_event(&session.bearer_token, *event_id).await
        {
            linked_events.push(LinkedEventRow { event });
        }
    }
    let mut record_context = std::collections::HashMap::<Uuid, Vec<String>>::new();
    for row in &linked_events {
        for record_id in &row.event.record_ids {
            record_context.entry(*record_id).or_default().push(row.event.event_type.clone());
        }
    }
    let mut evidence_records = record_context
        .into_iter()
        .map(|(id, mut event_types)| {
            event_types.sort();
            event_types.dedup();
            IncidentEvidenceRecordView {
                id,
                event_count: event_types.len(),
                event_types: event_types.join(", "),
            }
        })
        .collect::<Vec<_>>();
    evidence_records.sort_by(|left, right| {
        right.event_count.cmp(&left.event_count).then_with(|| left.id.cmp(&right.id))
    });

    let (
        impact_objects,
        impact_relationships,
        governed_responses,
        modeled_event_count,
        response_completed_count,
        response_review_count,
    ) = if let Some(client) = crate::ontology_client::global() {
        let (types, objects, invocations, reviews, action_types, link_types, links) = tokio::join!(
            client.list_object_types(&session.bearer_token),
            client.list_objects(&session.bearer_token, None),
            client.list_action_invocations(&session.bearer_token),
            client.list_action_reviews(&session.bearer_token),
            client.list_action_types(&session.bearer_token),
            client.list_link_types(&session.bearer_token),
            client.list_links(&session.bearer_token),
        );
        let type_names = types
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.id, item.name))
            .collect::<std::collections::HashMap<_, _>>();
        let objects = objects.unwrap_or_default();
        let reviews = reviews
            .unwrap_or_default()
            .into_iter()
            .map(|review| (review.invocation_id, review))
            .collect::<std::collections::HashMap<_, _>>();
        let link_types = link_types.unwrap_or_default();
        let links = links.unwrap_or_default();
        let object_labels = objects
            .iter()
            .map(|object| {
                (
                    object.id,
                    object
                        .properties
                        .get("name")
                        .or_else(|| object.properties.get("subject"))
                        .or_else(|| object.properties.get("title"))
                        .or_else(|| object.properties.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Untitled object")
                        .to_string(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut event_counts = std::collections::HashMap::<Uuid, usize>::new();
        let mut modeled_event_count = 0;
        for row in &linked_events {
            if let Some(object) = objects.iter().find(|object| {
                object.properties.get("id").and_then(serde_json::Value::as_str)
                    == Some(row.event.entity_ref.as_str())
                    || object.id.to_string() == row.event.entity_ref
            }) {
                *event_counts.entry(object.id).or_default() += 1;
                modeled_event_count += 1;
            }
        }
        let invocations = invocations.unwrap_or_default();
        let action_types = action_types.unwrap_or_default();
        let action_names = action_types
            .iter()
            .map(|action| (action.id, action.name.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let mut impact_objects = event_counts
            .into_iter()
            .filter_map(|(id, event_count)| {
                let object = objects.iter().find(|object| object.id == id)?;
                let target_invocations = invocations
                    .iter()
                    .filter(|invocation| {
                        invocation.target_object_ids.as_array().into_iter().flatten().any(
                            |target| {
                                target.as_str().and_then(|value| Uuid::parse_str(value).ok())
                                    == Some(id)
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let last =
                    target_invocations.iter().max_by_key(|invocation| invocation.executed_at);
                let actions = action_types
                    .iter()
                    .map(|action| {
                        let type_match = action
                            .target_object_type_id
                            .map(|type_id| type_id == object.object_type_id)
                            .unwrap_or(true);
                        let preconditions_match = action
                            .preconditions
                            .as_object()
                            .map(|preconditions| {
                                preconditions.iter().all(|(key, expected)| {
                                    object.properties.get(key) == Some(expected)
                                })
                            })
                            .unwrap_or(
                                action.preconditions.is_null()
                                    || action.preconditions == serde_json::json!({}),
                            );
                        CaseActionView {
                            id: action.id,
                            name: action.name.clone(),
                            eligible: type_match && preconditions_match,
                            parameter_fields: case_action_parameter_fields(
                                &action.parameter_schema,
                            ),
                            preconditions: serde_json::to_string_pretty(&action.preconditions)
                                .unwrap_or_default(),
                            effect_definition: serde_json::to_string_pretty(
                                &action.effect_definition,
                            )
                            .unwrap_or_default(),
                        }
                    })
                    .collect::<Vec<_>>();
                Some(ImpactObjectView {
                    id,
                    type_name: type_names
                        .get(&object.object_type_id)
                        .cloned()
                        .unwrap_or_else(|| "Modeled object".to_string()),
                    label: object_labels.get(&id).cloned().unwrap_or_else(|| id.to_string()),
                    status: object
                        .properties
                        .get("status")
                        .or_else(|| object.properties.get("health"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("modeled")
                        .to_string(),
                    event_count,
                    action_count: target_invocations.len(),
                    last_action: last.and_then(|invocation| {
                        action_names.get(&invocation.action_type_id).cloned()
                    }),
                    last_outcome: last.map(|invocation| invocation.outcome.clone()),
                    last_action_at: last.map(|invocation| invocation.executed_at),
                    actions,
                })
            })
            .collect::<Vec<_>>();
        impact_objects.sort_by(|left, right| {
            right
                .event_count
                .cmp(&left.event_count)
                .then_with(|| right.action_count.cmp(&left.action_count))
        });
        let impact_ids =
            impact_objects.iter().map(|object| object.id).collect::<std::collections::HashSet<_>>();
        let impact_relationships =
            impact_relationships(&objects, &type_names, &link_types, &links, &impact_ids, 2);
        let mut governed_responses = invocations
            .into_iter()
            .filter_map(|invocation| {
                let event_id = invocation
                    .triggering_event_ref
                    .get("event_id")
                    .or_else(|| invocation.triggering_event_ref.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok());
                let incident_id = invocation
                    .triggering_event_ref
                    .get("incident_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok());
                let target_id =
                    invocation.target_object_ids.as_array().into_iter().flatten().find_map(
                        |target| {
                            target
                                .as_str()
                                .and_then(|value| Uuid::parse_str(value).ok())
                                .filter(|id| impact_ids.contains(id))
                        },
                    );
                if target_id.is_none() && incident_id != Some(id) {
                    return None;
                }
                Some(GovernedResponseView {
                    id: invocation.id,
                    action_name: action_names
                        .get(&invocation.action_type_id)
                        .cloned()
                        .unwrap_or_else(|| "Unknown governed action".to_string()),
                    outcome: invocation.outcome,
                    review_status: reviews
                        .get(&invocation.id)
                        .map(|review| review.status.replace('_', " "))
                        .unwrap_or_else(|| "not reviewed".to_string()),
                    review_assignee: reviews
                        .get(&invocation.id)
                        .and_then(|review| review.assignee.clone()),
                    review_stale: reviews
                        .get(&invocation.id)
                        .map(|review| {
                            !matches!(review.status.as_str(), "approved" | "declined")
                                && review.due_at.is_some_and(|due_at| due_at <= chrono::Utc::now())
                        })
                        .unwrap_or(false),
                    target_label: target_id
                        .and_then(|target_id| object_labels.get(&target_id).cloned())
                        .unwrap_or_else(|| "Case-level response".to_string()),
                    executed_at: invocation.executed_at,
                    event_id,
                    incident_id,
                })
            })
            .collect::<Vec<_>>();
        governed_responses.sort_by_key(|response| std::cmp::Reverse(response.executed_at));
        governed_responses.truncate(12);
        let response_completed_count = governed_responses
            .iter()
            .filter(|response| response.outcome.eq_ignore_ascii_case("completed"))
            .count();
        let response_review_count =
            governed_responses.len().saturating_sub(response_completed_count);
        (
            impact_objects,
            impact_relationships,
            governed_responses,
            modeled_event_count,
            response_completed_count,
            response_review_count,
        )
    } else {
        (vec![], vec![], vec![], 0, 0, 0)
    };

    // The case page should carry its own decision history. A temporary audit-service outage
    // must not hide the case itself or turn a usable investigation page into an error page.
    let activity = state
        .incidents_client
        .list_audit_log_for_entity(session.tenant_id, id)
        .await
        .map(activity_rows)
        .unwrap_or_default();

    let mut timeline = Vec::new();
    timeline.push(IncidentTimelineEntry {
        kind: "Case opened".to_string(),
        actor: incident.assigned_to.clone().unwrap_or_else(|| "system".to_string()),
        at: incident.created_at,
        summary: incident.title.clone(),
        detail: format!("{} severity · {}", incident.severity, incident.status),
        href: None,
        is_failure: false,
    });
    for row in &linked_events {
        timeline.push(IncidentTimelineEntry {
            kind: "Signal linked".to_string(),
            actor: "signal pipeline".to_string(),
            at: row.event.occurred_at,
            summary: row.event.event_type.clone(),
            detail: format!("{} · {}", row.event.group_key, row.event.status),
            href: Some(format!("/events/{}", row.event.id)),
            is_failure: row.event.status.eq_ignore_ascii_case("dismissed"),
        });
    }
    for row in &activity {
        timeline.push(IncidentTimelineEntry {
            kind: row.change_type.clone(),
            actor: row.actor.clone(),
            at: row.changed_at,
            summary: row.summary.clone(),
            detail: "Case lifecycle".to_string(),
            href: None,
            is_failure: false,
        });
    }
    for note in &notes {
        timeline.push(IncidentTimelineEntry {
            kind: "Investigation note".to_string(),
            actor: note.author.clone(),
            at: note.created_at,
            summary: note.body.clone(),
            detail: "Operator finding".to_string(),
            href: None,
            is_failure: false,
        });
    }
    for response in &governed_responses {
        timeline.push(IncidentTimelineEntry {
            kind: "Governed response".to_string(),
            actor: "operator".to_string(),
            at: response.executed_at,
            summary: format!("{} → {}", response.action_name, response.target_label),
            detail: response.outcome.clone(),
            href: response
                .event_id
                .map(|id| format!("/events/{id}"))
                .or_else(|| Some("/actions".to_string())),
            is_failure: !response.outcome.eq_ignore_ascii_case("completed"),
        });
    }
    timeline.sort_by_key(|entry| std::cmp::Reverse(entry.at));
    timeline.truncate(40);
    let case_opened = incident.created_at;
    let first_signal = linked_events.iter().map(|row| row.event.occurred_at).min();
    let first_signal_href = linked_events
        .iter()
        .min_by_key(|row| row.event.occurred_at)
        .map(|row| format!("/events/{}", row.event.id));
    let first_response = governed_responses.iter().map(|row| row.executed_at).min();
    let first_response_href = governed_responses
        .iter()
        .min_by_key(|row| row.executed_at)
        .and_then(|row| row.event_id.map(|id| format!("/events/{id}")));
    let case_end = incident.resolved_at.unwrap_or_else(chrono::Utc::now);
    let stages = [
        ("Case opened", case_opened, "Investigation created", None, "neutral"),
        (
            "First signal",
            first_signal.unwrap_or(case_opened),
            "Evidence entered the case",
            first_signal_href,
            "good",
        ),
        (
            "First response",
            first_response.unwrap_or(case_opened),
            "Governed action attempted",
            first_response_href,
            "warning",
        ),
        (
            if incident.resolved_at.is_some() { "Resolved" } else { "Open now" },
            case_end,
            if incident.resolved_at.is_some() {
                "Case lifecycle ended"
            } else {
                "Current case age"
            },
            None,
            if incident.resolved_at.is_some() { "good" } else { "risk" },
        ),
    ];
    let max_stage_seconds = stages
        .iter()
        .map(|(_, at, _, _, _)| (*at - case_opened).num_seconds().max(0))
        .max()
        .unwrap_or(1)
        .max(1);
    let timeline_metrics = stages
        .into_iter()
        .map(|(label, at, detail, href, tone)| IncidentTimelineMetric {
            label: label.to_string(),
            detail: detail.to_string(),
            elapsed: case_duration(case_opened, at),
            width_pct: (((at - case_opened).num_seconds().max(0) * 100) / max_stage_seconds).max(4)
                as usize,
            href,
            tone: tone.to_string(),
        })
        .collect();
    let sla = incident_sla(&incident, chrono::Utc::now());
    let (case_graph_nodes, case_graph_edges) =
        build_case_graph(&impact_objects, &impact_relationships);
    let response_outcome_metrics = response_outcome_metrics(&governed_responses);
    Html(
        IncidentDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            incident: Some(incident),
            sla: Some(sla),
            linked_events,
            evidence_records,
            impact_objects,
            impact_relationships,
            case_graph_nodes,
            case_graph_edges,
            governed_responses,
            timeline,
            timeline_metrics,
            modeled_event_count,
            response_completed_count,
            response_review_count,
            response_outcome_metrics,
            notes,
            activity,
            assignee_options,
            error: None,
            notice: query.notice,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

/// GET /incidents/:id/export.csv — exports the same evidence chain shown on the case page so a
/// response lead can hand off an investigation without losing its causal context.
pub async fn get_incident_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let detail = match state.incidents_client.get_incident(session.tenant_id, id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return (axum::http::StatusCode::NOT_FOUND, "incident not found").into_response()
        }
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    };
    let event_ids = detail.event_ids.iter().copied().collect::<std::collections::HashSet<_>>();
    let mut rows = vec![ExportRow {
        section: "case".to_string(),
        at: detail.incident.created_at,
        actor: detail.incident.assigned_to.clone().unwrap_or_else(|| "system".to_string()),
        kind: "Case opened".to_string(),
        summary: detail.incident.title.clone(),
        detail: format!("{} severity · {}", detail.incident.severity, detail.incident.status),
        reference: format!("/incidents/{id}"),
    }];
    for event_id in &detail.event_ids {
        if let Ok(Some(event)) =
            state.events_client.get_event(&session.bearer_token, *event_id).await
        {
            rows.push(ExportRow {
                section: "signals".to_string(),
                at: event.occurred_at,
                actor: "signal pipeline".to_string(),
                kind: "Signal linked".to_string(),
                summary: event.event_type,
                detail: format!("{} · {}", event.group_key, event.status),
                reference: format!("/events/{event_id}"),
            });
        }
    }
    if let Ok(entries) =
        state.incidents_client.list_audit_log_for_entity(session.tenant_id, id).await
    {
        for entry in entries {
            let summary = audit_value_summary(&entry);
            rows.push(ExportRow {
                section: "audit".to_string(),
                at: entry.changed_at,
                actor: entry.actor,
                kind: entry.change_type,
                summary,
                detail: "Case lifecycle".to_string(),
                reference: format!("/incidents/{id}"),
            });
        }
    }
    for note in detail.notes {
        rows.push(ExportRow {
            section: "notes".to_string(),
            at: note.created_at,
            actor: note.author,
            kind: "Investigation note".to_string(),
            summary: note.body,
            detail: "Operator finding".to_string(),
            reference: format!("/incidents/{id}"),
        });
    }
    if let Some(client) = ontology_client::global() {
        let (action_types, invocations, objects) = tokio::join!(
            client.list_action_types(&session.bearer_token),
            client.list_action_invocations(&session.bearer_token),
            client.list_objects(&session.bearer_token, None),
        );
        let action_names = action_types
            .unwrap_or_default()
            .into_iter()
            .map(|action| (action.id, action.name))
            .collect::<std::collections::HashMap<_, _>>();
        let object_labels = objects
            .unwrap_or_default()
            .into_iter()
            .map(|object| {
                let label = object
                    .properties
                    .get("name")
                    .or_else(|| object.properties.get("subject"))
                    .or_else(|| object.properties.get("title"))
                    .or_else(|| object.properties.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled object")
                    .to_string();
                (object.id, label)
            })
            .collect::<std::collections::HashMap<_, _>>();
        for invocation in invocations.unwrap_or_default() {
            let event_id = invocation
                .triggering_event_ref
                .get("event_id")
                .or_else(|| invocation.triggering_event_ref.get("id"))
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok());
            let incident_id = invocation
                .triggering_event_ref
                .get("incident_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok());
            if incident_id != Some(id)
                && !event_id.is_some_and(|event_id| event_ids.contains(&event_id))
            {
                continue;
            }
            let target = invocation
                .target_object_ids
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|value| value.as_str().and_then(|value| Uuid::parse_str(value).ok()))
                .map(|target| {
                    object_labels.get(&target).cloned().unwrap_or_else(|| target.to_string())
                })
                .unwrap_or_else(|| "target unavailable".to_string());
            rows.push(ExportRow {
                section: "responses".to_string(),
                at: invocation.executed_at,
                actor: invocation
                    .triggering_event_ref
                    .get("actor")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("operator")
                    .to_string(),
                kind: "Governed response".to_string(),
                summary: format!(
                    "{} → {}",
                    action_names
                        .get(&invocation.action_type_id)
                        .map(String::as_str)
                        .unwrap_or("Unknown action"),
                    target
                ),
                detail: invocation.outcome,
                reference: event_id
                    .map(|event_id| format!("/events/{event_id}"))
                    .unwrap_or_else(|| "/actions".to_string()),
            });
        }
    }
    rows.sort_by(|left, right| left.at.cmp(&right.at));
    let mut csv = String::from("section,occurred_at,actor,kind,summary,detail,reference\n");
    for row in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_escape(&row.section),
            row.at.to_rfc3339(),
            csv_escape(&row.actor),
            csv_escape(&row.kind),
            csv_escape(&row.summary),
            csv_escape(&row.detail),
            csv_escape(&row.reference)
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers
        .insert(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=incident-{id}-evidence.csv").parse().unwrap(),
    );
    (response_headers, csv).into_response()
}

struct ExportRow {
    section: String,
    at: chrono::DateTime<chrono::Utc>,
    actor: String,
    kind: String,
    summary: String,
    detail: String,
    reference: String,
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct AddIncidentNoteForm {
    body: String,
}

pub async fn post_add_incident_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<AddIncidentNoteForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if form.body.trim().is_empty() {
        return Redirect::to(&format!("/incidents/{id}")).into_response();
    }
    let _ = state
        .incidents_client
        .add_note(session.role, &session.username, session.tenant_id, id, form.body.trim())
        .await;
    Redirect::to(&format!("/incidents/{id}")).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateIncidentForm {
    title: String,
    severity: String,
    status: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    assigned_to: String,
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
    if form.title.trim().is_empty() || form.title.len() > 500 || form.summary.len() > 2_000 {
        return Redirect::to(&format!("/incidents/{id}")).into_response();
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

    let assignee = form.assigned_to.trim();
    let allowed_assignees = incident_assignee_options(&state, &session).await;
    if !assignee.is_empty() && !allowed_assignees.iter().any(|option| option.username == assignee) {
        return Redirect::to(&format!("/incidents/{id}")).into_response();
    }

    let resolved_at =
        if status == IncidentStatus::Resolved { Some(chrono::Utc::now()) } else { None };
    let updated = Incident {
        title: form.title,
        summary: form.summary.trim().to_string(),
        severity,
        status,
        assigned_to: (!assignee.is_empty()).then(|| assignee.to_string()),
        updated_at: chrono::Utc::now(),
        resolved_at,
        ..existing
    };

    let _ = state.incidents_client.update_incident(session.role, &session.username, updated).await;
    Redirect::to(&format!("/incidents/{id}")).into_response()
}

/// POST /incidents/:id/claim — assigns an active unowned case to the current operator. It
/// re-reads and updates the complete Incident through the normal audited service path, so a
/// claim is visible in the case history and cannot silently overwrite the title or severity.
pub async fn post_claim_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(context): Query<WorkReturnContext>,
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
        _ => return work_return_redirect("claim_failed", &context).into_response(),
    };
    if existing.status == IncidentStatus::Resolved || existing.assigned_to.is_some() {
        return work_return_redirect("claim_unavailable", &context).into_response();
    }
    let updated = Incident {
        assigned_to: Some(session.username.clone()),
        updated_at: chrono::Utc::now(),
        ..existing
    };
    match state.incidents_client.update_incident(session.role, &session.username, updated).await {
        Ok(_) => work_return_redirect("claimed", &context).into_response(),
        Err(_) => work_return_redirect("claim_failed", &context).into_response(),
    }
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

fn parse_bulk_status(raw_body: &[u8]) -> Result<Option<IncidentStatus>, ()> {
    let pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body).map_err(|_| ())?;
    let value = pairs
        .into_iter()
        .find(|(key, _)| key == "target_status")
        .map(|(_, value)| value)
        .ok_or(())?;
    if value == "__keep__" {
        Ok(None)
    } else {
        IncidentStatus::from_str(&value).map(Some).map_err(|_| ())
    }
}

fn parse_bulk_owner(raw_body: &[u8]) -> Option<Option<String>> {
    let pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body).ok()?;
    let value = pairs.into_iter().find(|(key, _)| key == "target_owner").map(|(_, value)| value)?;
    if value == "__keep__" {
        None
    } else if value.is_empty() {
        Some(None)
    } else {
        Some(Some(value))
    }
}

fn bulk_incident_redirect(notice: &str, raw_body: &[u8]) -> Redirect {
    let pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body).unwrap_or_default();
    let mut params = vec![("notice", notice.to_string())];
    for key in ["q", "status", "severity", "sla", "owner", "sort", "dir", "view"] {
        if let Some((_, value)) =
            pairs.iter().find(|(name, value)| name == key && !value.is_empty())
        {
            params.push((key, value.clone()));
        }
    }
    Redirect::to(&format!("/incidents?{}", serde_urlencoded::to_string(params).unwrap_or_default()))
}

/// POST /incidents/bulk-update — apply one governed lifecycle transition to selected cases.
/// Each update still travels through the incident client, preserving the service's role/actor
/// checks and audit trail instead of introducing a privileged batch endpoint.
pub async fn post_bulk_update_incidents(
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
    let Ok(target_status) = parse_bulk_status(&body) else {
        return bulk_incident_redirect("bulk_invalid", &body).into_response();
    };
    let target_owner = parse_bulk_owner(&body);
    if target_status.is_none() && target_owner.is_none() {
        return bulk_incident_redirect("bulk_invalid", &body).into_response();
    }
    let allowed_assignees = incident_assignee_options(&state, &session).await;
    if let Some(Some(owner)) = target_owner.as_ref() {
        if !allowed_assignees.iter().any(|option| option.username == *owner) {
            return bulk_incident_redirect("bulk_invalid", &body).into_response();
        }
    }
    let mut ids = parse_ids(&body);
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return bulk_incident_redirect("bulk_empty", &body).into_response();
    }

    let mut updated = 0usize;
    let mut failed = 0usize;
    for id in ids {
        let Ok(Some(detail)) = state.incidents_client.get_incident(session.tenant_id, id).await
        else {
            failed += 1;
            continue;
        };
        let resolved_at = match target_status {
            Some(IncidentStatus::Resolved) => Some(chrono::Utc::now()),
            Some(_) => None,
            None => detail.incident.resolved_at,
        };
        let assigned_to = match target_owner.as_ref() {
            Some(Some(owner)) => Some(owner.clone()),
            Some(None) => None,
            None => detail.incident.assigned_to.clone(),
        };
        let incident = Incident {
            status: target_status.unwrap_or(detail.incident.status),
            assigned_to,
            updated_at: chrono::Utc::now(),
            resolved_at,
            ..detail.incident
        };
        match state
            .incidents_client
            .update_incident(session.role, &session.username, incident)
            .await
        {
            Ok(_) => updated += 1,
            Err(_) => failed += 1,
        }
    }
    let notice = if updated == 0 {
        "bulk_failed"
    } else if failed > 0 {
        "bulk_partial"
    } else {
        "bulk_updated"
    };
    bulk_incident_redirect(notice, &body).into_response()
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct IncidentStatusTransitionForm {
    target_status: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    #[serde(rename = "status")]
    status_filter: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    sla: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
}

fn board_transition_redirect(notice: &str, form: &IncidentStatusTransitionForm) -> Redirect {
    let mut params = vec![("view", "board".to_string()), ("notice", notice.to_string())];
    for (key, value) in [
        ("q", &form.q),
        ("status", &form.status_filter),
        ("severity", &form.severity),
        ("sla", &form.sla),
        ("owner", &form.owner),
        ("sort", &form.sort),
        ("dir", &form.dir),
    ] {
        if !value.is_empty() {
            params.push((key, value.clone()));
        }
    }
    Redirect::to(&format!("/incidents?{}", serde_urlencoded::to_string(params).unwrap_or_default()))
}

/// POST /incidents/:id/status — the focused board-card lifecycle control. It re-reads the full
/// incident before updating so title, brief, owner, and timestamps are never lost by a compact
/// status-only form; the incident service still records the audited transition.
pub async fn post_incident_status_transition(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<IncidentStatusTransitionForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let Ok(status) = IncidentStatus::from_str(&form.target_status) else {
        return board_transition_redirect("bulk_invalid", &form).into_response();
    };
    let Ok(Some(detail)) = state.incidents_client.get_incident(session.tenant_id, id).await else {
        return board_transition_redirect("bulk_failed", &form).into_response();
    };
    let resolved_at =
        if status == IncidentStatus::Resolved { Some(chrono::Utc::now()) } else { None };
    let incident =
        Incident { status, resolved_at, updated_at: chrono::Utc::now(), ..detail.incident };
    let notice = match state
        .incidents_client
        .update_incident(session.role, &session.username, incident)
        .await
    {
        Ok(_) => "transitioned",
        Err(_) => "bulk_failed",
    };
    board_transition_redirect(notice, &form).into_response()
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

    let existing = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(details) => details,
        Err(_) => return Redirect::to("/events?notice=link_lookup_failed").into_response(),
    };
    let linked_event_ids = existing
        .iter()
        .flat_map(|detail| detail.event_ids.iter().copied())
        .collect::<std::collections::HashSet<_>>();
    let unlinked_event_ids =
        event_ids.iter().copied().filter(|id| !linked_event_ids.contains(id)).collect::<Vec<_>>();
    let skipped_count = event_ids.len().saturating_sub(unlinked_event_ids.len());
    if unlinked_event_ids.is_empty() {
        return Redirect::to("/events?notice=already_linked").into_response();
    }

    if let Some(incident_id) =
        find_correlated_incident(&state, &session.bearer_token, &existing, &unlinked_event_ids)
            .await
    {
        let mut linked_count = 0usize;
        for event_id in &unlinked_event_ids {
            if state
                .incidents_client
                .link_event(
                    session.role,
                    &session.username,
                    session.tenant_id,
                    incident_id,
                    *event_id,
                )
                .await
                .is_ok()
            {
                linked_count += 1;
            }
        }
        return Redirect::to(&format!(
            "/incidents/{incident_id}?notice=correlated&linked_count={linked_count}"
        ))
        .into_response();
    }

    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: session.tenant_id,
        title: format!("Incident from {} selected event(s)", unlinked_event_ids.len()),
        summary: format!(
            "Investigation created from {} selected event signal(s).",
            unlinked_event_ids.len()
        ),
        severity: IncidentSeverity::Medium,
        status: IncidentStatus::Open,
        assigned_to: Some(session.username.clone()),
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };

    match state
        .incidents_client
        .create_incident(session.role, &session.username, incident, unlinked_event_ids)
        .await
    {
        Ok(detail) if skipped_count > 0 => Redirect::to(&format!(
            "/events?notice=skipped_linked&created_incident={}&skipped_count={skipped_count}",
            detail.incident.id
        ))
        .into_response(),
        Ok(detail) => Redirect::to(&format!("/incidents/{}", detail.incident.id)).into_response(),
        Err(_) => Redirect::to("/events").into_response(),
    }
}

/// POST /events/link-incident — attach several selected signals to one existing case. This is
/// the complementary bulk workflow to creating a new case: responders can keep a single case
/// as the investigation grows without opening each event detail page individually.
pub async fn post_link_events_to_incident(
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
    let pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body).unwrap_or_default();
    let incident_id = pairs
        .iter()
        .find(|(key, _)| key == "incident_id")
        .and_then(|(_, value)| Uuid::parse_str(value).ok());
    let Some(incident_id) = incident_id else {
        return Redirect::to("/events?notice=link_invalid").into_response();
    };
    let event_ids = parse_ids(&body);
    if event_ids.is_empty() {
        return Redirect::to("/events?notice=link_empty").into_response();
    }
    let mut linked_count = 0usize;
    let mut failed_count = 0usize;
    for event_id in event_ids {
        match state
            .incidents_client
            .link_event(session.role, &session.username, session.tenant_id, incident_id, event_id)
            .await
        {
            Ok(()) => linked_count += 1,
            Err(_) => failed_count += 1,
        }
    }
    Redirect::to(&format!("/events?notice=events-linked&linked_count_result={linked_count}&link_failed_count={failed_count}&linked_incident={incident_id}")).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateEventIncidentForm {
    title: String,
    severity: String,
    #[serde(default)]
    summary: String,
}

fn event_correlation_keys(event: &EventDetail) -> Vec<String> {
    [event.group_key.trim(), event.entity_ref.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

async fn find_correlated_incident(
    state: &AppState,
    bearer_token: &str,
    existing: &[crate::IncidentDetail],
    candidate_event_ids: &[Uuid],
) -> Option<Uuid> {
    let mut candidate_keys = std::collections::HashSet::new();
    for event_id in candidate_event_ids {
        if let Ok(Some(event)) = state.events_client.get_event(bearer_token, *event_id).await {
            candidate_keys.extend(event_correlation_keys(&event));
        }
    }
    if candidate_keys.is_empty() {
        return None;
    }
    for detail in existing {
        if detail.incident.status == IncidentStatus::Resolved {
            continue;
        }
        for event_id in &detail.event_ids {
            if let Ok(Some(event)) = state.events_client.get_event(bearer_token, *event_id).await {
                if event_correlation_keys(&event)
                    .into_iter()
                    .any(|key| candidate_keys.contains(&key))
                {
                    return Some(detail.incident.id);
                }
            }
        }
    }
    None
}

/// POST /events/:id/create-incident — the focused version of the Events bulk action, keeping
/// the operator on the signal's investigation page after creating a case.
pub async fn post_create_incident_from_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<Uuid>,
    Form(form): Form<CreateEventIncidentForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    // Re-read the tenant's case links before creating a focused case. The detail page normally
    // hides this form once a signal is linked, but the server guard also makes refreshes,
    // duplicate submissions, and hand-crafted requests idempotent.
    let existing =
        state.incidents_client.list_incidents(session.tenant_id, None).await.unwrap_or_default();
    if existing.iter().any(|detail| detail.event_ids.contains(&event_id)) {
        return Redirect::to(&format!("/events/{event_id}?notice=already_linked")).into_response();
    }
    if let Some(incident_id) =
        find_correlated_incident(&state, &session.bearer_token, &existing, &[event_id]).await
    {
        let notice = if state
            .incidents_client
            .link_event(session.role, &session.username, session.tenant_id, incident_id, event_id)
            .await
            .is_ok()
        {
            "correlated"
        } else {
            "failed"
        };
        return Redirect::to(&format!(
            "/incidents/{incident_id}?notice={notice}&correlated_event={event_id}"
        ))
        .into_response();
    }
    let Ok(severity) = IncidentSeverity::from_str(&form.severity) else {
        return Redirect::to(&format!("/events/{event_id}?notice=invalid")).into_response();
    };
    if form.title.trim().is_empty() || form.title.len() > 500 || form.summary.len() > 2_000 {
        return Redirect::to(&format!("/events/{event_id}?notice=invalid")).into_response();
    }
    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: session.tenant_id,
        title: form.title.trim().to_string(),
        summary: form.summary.trim().to_string(),
        severity,
        status: IncidentStatus::Open,
        assigned_to: Some(session.username.clone()),
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };
    match state
        .incidents_client
        .create_incident(session.role, &session.username, incident, vec![event_id])
        .await
    {
        Ok(detail) => Redirect::to(&format!("/incidents/{}", detail.incident.id)).into_response(),
        Err(_) => Redirect::to(&format!("/events/{event_id}?notice=failed")).into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct LinkEventIncidentForm {
    incident_id: Uuid,
}

/// POST /events/:id/link-incident — attach one signal to an existing case from its detail view.
pub async fn post_link_event_to_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<Uuid>,
    Form(form): Form<LinkEventIncidentForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let notice = match state
        .incidents_client
        .link_event(session.role, &session.username, session.tenant_id, form.incident_id, event_id)
        .await
    {
        Ok(()) => "linked",
        Err(_) => "failed",
    };
    Redirect::to(&format!("/events/{event_id}?notice={notice}")).into_response()
}
