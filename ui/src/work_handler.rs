#[path = "work_handler_test.rs"]
#[cfg(test)]
mod work_handler_test;

use crate::incident_handlers::incident_sla;
use crate::ontology_client;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use common::{IncidentStatus, SavedSearchQuery};

struct WorkIncident {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
    owner: Option<String>,
    event_count: usize,
    sla_state: String,
    sla_label: String,
    sla_detail: String,
    age_days: i64,
}

struct WorkAction {
    id: uuid::Uuid,
    action_type_id: uuid::Uuid,
    target_object_ids: String,
    parameters: String,
    name: String,
    outcome: String,
    review_status: String,
    review_assignee: String,
    executed_at: chrono::DateTime<chrono::Utc>,
    incident_id: Option<uuid::Uuid>,
    event_id: Option<uuid::Uuid>,
    targets: Vec<WorkActionTarget>,
}

struct WorkActionTarget {
    id: uuid::Uuid,
    label: String,
}

struct WorkMetric {
    label: String,
    count: usize,
    percent: i32,
    href: String,
    tone: String,
}

struct WorkAgeMetric {
    label: String,
    count: usize,
    percent: i32,
    detail: String,
    href: String,
}

#[derive(Template)]
#[template(path = "work.html")]
struct WorkTemplate {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    username: String,
    my_incidents: Vec<WorkIncident>,
    unassigned_incidents: Vec<WorkIncident>,
    other_incidents: Vec<WorkIncident>,
    review_actions: Vec<WorkAction>,
    error: Option<String>,
    notice: String,
    claimed_count: usize,
    claim_failed_count: usize,
    focus: String,
    q: String,
    severity: String,
    sla: String,
    age: String,
    saved_views: Vec<SavedWorkView>,
    severity_metrics: Vec<WorkMetric>,
    sla_metrics: Vec<WorkMetric>,
    ownership_metrics: Vec<WorkMetric>,
    age_metrics: Vec<WorkAgeMetric>,
}

struct SavedWorkView {
    id: uuid::Uuid,
    name: String,
    load_url: String,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct WorkQuery {
    #[serde(default)]
    notice: String,
    #[serde(default)]
    claimed_count: usize,
    #[serde(default)]
    claim_failed_count: usize,
    #[serde(default)]
    focus: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    sla: String,
    #[serde(default)]
    age: String,
}

fn normalize_focus(value: &str) -> String {
    match value {
        "assigned" | "unassigned" | "review" => value.to_string(),
        _ => String::new(),
    }
}

fn normalize_severity(value: &str) -> String {
    match value {
        "critical" | "high" | "medium" | "low" => value.to_string(),
        _ => String::new(),
    }
}

fn normalize_sla(value: &str) -> String {
    match value {
        "breached" | "at-risk" | "on-track" => value.to_string(),
        _ => String::new(),
    }
}

fn normalize_age(value: &str) -> String {
    match value {
        "0_1" | "1_7" | "8_30" | "31_plus" => value.to_string(),
        _ => String::new(),
    }
}

fn matches_age(age_days: i64, age: &str) -> bool {
    match age {
        "0_1" => age_days == 0,
        "1_7" => (1..=7).contains(&age_days),
        "8_30" => (8..=30).contains(&age_days),
        "31_plus" => age_days >= 31,
        _ => true,
    }
}

fn work_scope_href(
    q: &str,
    severity: &str,
    sla: &str,
    focus: &str,
    age: &str,
    extra: (&str, &str),
) -> String {
    let mut params = vec![
        ("q", q.to_string()),
        ("severity", severity.to_string()),
        ("sla", sla.to_string()),
        ("focus", focus.to_string()),
        ("age", age.to_string()),
    ];
    params.push((extra.0, extra.1.to_string()));
    format!("/work?{}", serde_urlencoded::to_string(params).unwrap_or_default())
}

fn matches_work_text(value: &str, query: &str) -> bool {
    query.trim().is_empty()
        || value.to_ascii_lowercase().contains(&query.trim().to_ascii_lowercase())
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn saved_work_views(queries: Vec<SavedSearchQuery>) -> Vec<SavedWorkView> {
    queries
        .into_iter()
        .filter(|query| {
            query.filter.get("view_kind").and_then(serde_json::Value::as_str) == Some("work")
        })
        .map(|query| {
            let params = query
                .filter
                .as_object()
                .map(|filter| {
                    filter
                        .iter()
                        .filter(|(key, _)| {
                            *key == "focus"
                                || *key == "q"
                                || *key == "severity"
                                || *key == "sla"
                                || *key == "age"
                        })
                        .filter_map(|(key, value)| {
                            Some((key.as_str(), value.as_str()?.to_string()))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            SavedWorkView {
                id: query.id,
                name: query.name,
                load_url: format!(
                    "/work?{}",
                    serde_urlencoded::to_string(params).unwrap_or_default()
                ),
            }
        })
        .collect()
}

fn to_work_incident(
    detail: crate::incidents_client::IncidentDetail,
    now: chrono::DateTime<chrono::Utc>,
) -> WorkIncident {
    let sla = incident_sla(&detail.incident, now);
    let age_days = now.signed_duration_since(detail.incident.created_at).num_days().max(0);
    WorkIncident {
        id: detail.incident.id,
        title: detail.incident.title,
        severity: detail.incident.severity.to_string(),
        status: detail.incident.status.to_string(),
        owner: detail.incident.assigned_to,
        event_count: detail.event_ids.len(),
        sla_state: sla.state,
        sla_label: sla.label,
        sla_detail: sla.detail,
        age_days,
    }
}

pub async fn get_work(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_manage = session.role.at_least(common::Role::Operator);
    let focus = normalize_focus(&query.focus);
    let severity = normalize_severity(&query.severity);
    let sla = normalize_sla(&query.sla);
    let age = normalize_age(&query.age);
    let saved_views = saved_work_views(
        state.saved_search_queries_client.list(session.tenant_id).await.unwrap_or_default(),
    );
    let mut error = None;
    let incidents = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(items) => items,
        Err(e) => {
            error = Some(format!("incidents: {e}"));
            vec![]
        }
    };
    let mut my_incidents = Vec::new();
    let mut unassigned_incidents = Vec::new();
    let mut other_incidents = Vec::new();
    let now = chrono::Utc::now();
    for detail in incidents {
        if detail.incident.status == IncidentStatus::Resolved {
            continue;
        }
        let is_mine = detail.incident.assigned_to.as_deref() == Some(session.username.as_str());
        let is_unassigned = detail.incident.assigned_to.is_none();
        let incident = to_work_incident(detail, now);
        if (!severity.is_empty() && incident.severity != severity)
            || (!sla.is_empty() && incident.sla_state != sla)
            || (!age.is_empty() && !matches_age(incident.age_days, &age))
            || !matches_work_text(
                &format!("{} {} {}", incident.title, incident.severity, incident.status),
                &query.q,
            )
        {
            continue;
        }
        if is_mine {
            my_incidents.push(incident);
        } else if is_unassigned {
            unassigned_incidents.push(incident);
        } else {
            other_incidents.push(incident);
        }
    }
    my_incidents.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.title.cmp(&b.title)));
    unassigned_incidents
        .sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.title.cmp(&b.title)));
    other_incidents.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.title.cmp(&b.title)));

    let mut review_actions = Vec::new();
    if let Some(client) = ontology_client::global() {
        let (types, invocations, objects, reviews) = tokio::join!(
            client.list_action_types(&session.bearer_token),
            client.list_action_invocations(&session.bearer_token),
            client.list_objects(&session.bearer_token, None),
            client.list_action_reviews(&session.bearer_token),
        );
        if let Err(e) = &types {
            if error.is_none() {
                error = Some(format!("action definitions: {e}"));
            }
        }
        if let Err(e) = &invocations {
            if error.is_none() {
                error = Some(format!("action ledger: {e}"));
            }
        }
        if let Err(e) = &objects {
            if error.is_none() {
                error = Some(format!("ontology objects: {e}"));
            }
        }
        let object_titles = objects
            .unwrap_or_default()
            .into_iter()
            .map(|object| {
                let label = object
                    .properties
                    .get("name")
                    .or_else(|| object.properties.get("title"))
                    .or_else(|| object.properties.get("subject"))
                    .or_else(|| object.properties.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled entity")
                    .to_string();
                (object.id, label)
            })
            .collect::<std::collections::HashMap<_, _>>();
        let names = types
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.id, item.name))
            .collect::<std::collections::HashMap<_, _>>();
        let reviews = reviews
            .unwrap_or_default()
            .into_iter()
            .map(|review| (review.invocation_id, review))
            .collect::<std::collections::HashMap<_, _>>();
        for invocation in invocations
            .unwrap_or_default()
            .into_iter()
            .filter(|item| !item.outcome.eq_ignore_ascii_case("completed"))
            .take(20)
        {
            let event_id = invocation
                .triggering_event_ref
                .get("event_id")
                .or_else(|| invocation.triggering_event_ref.get("id"))
                .and_then(serde_json::Value::as_str)
                .and_then(|value| uuid::Uuid::parse_str(value).ok());
            let incident_id = invocation
                .triggering_event_ref
                .get("incident_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| uuid::Uuid::parse_str(value).ok());
            let target_object_ids = invocation
                .target_object_ids
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            let targets = invocation
                .target_object_ids
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .filter_map(|value| value.parse::<uuid::Uuid>().ok())
                .map(|id| WorkActionTarget {
                    id,
                    label: object_titles.get(&id).cloned().unwrap_or_else(|| id.to_string()),
                })
                .collect();
            review_actions.push(WorkAction {
                id: invocation.id,
                action_type_id: invocation.action_type_id,
                target_object_ids,
                parameters: invocation.parameters.to_string(),
                name: names
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
                    .and_then(|review| review.assignee.clone())
                    .unwrap_or_default(),
                executed_at: invocation.executed_at,
                incident_id,
                event_id,
                targets,
            });
        }
    }
    review_actions.retain(|action| {
        matches_work_text(&format!("{} {}", action.name, action.outcome), &query.q)
    });
    let active_cases = my_incidents
        .iter()
        .chain(unassigned_incidents.iter())
        .chain(other_incidents.iter())
        .collect::<Vec<_>>();
    let metric = |label: &str, count: usize, total: usize, href: String, tone: &str| WorkMetric {
        label: label.to_string(),
        count,
        percent: if total == 0 { 0 } else { (count * 100 / total) as i32 },
        href,
        tone: tone.to_string(),
    };
    let severity_metrics = [
        ("Critical", "critical", "danger"),
        ("High", "high", "warning"),
        ("Medium", "medium", "neutral"),
        ("Low", "low", "good"),
    ]
    .into_iter()
    .map(|(label, key, tone)| {
        metric(
            label,
            active_cases.iter().filter(|item| item.severity == key).count(),
            active_cases.len(),
            work_scope_href(&query.q, &severity, &sla, &focus, &age, ("severity", key)),
            tone,
        )
    })
    .collect::<Vec<_>>();
    let sla_metrics = [
        ("Breached", "breached", "danger"),
        ("At risk", "at-risk", "warning"),
        ("On track", "on-track", "good"),
    ]
    .into_iter()
    .map(|(label, key, tone)| {
        metric(
            label,
            active_cases.iter().filter(|item| item.sla_state == key).count(),
            active_cases.len(),
            work_scope_href(&query.q, &severity, &sla, &focus, &age, ("sla", key)),
            tone,
        )
    })
    .collect::<Vec<_>>();
    let mut ownership_counts = std::collections::BTreeMap::<String, usize>::new();
    for item in &active_cases {
        *ownership_counts
            .entry(item.owner.clone().unwrap_or_else(|| "Unassigned".to_string()))
            .or_default() += 1;
    }
    let ownership_total = active_cases.len();
    let ownership_metrics = ownership_counts
        .into_iter()
        .map(|(owner, count)| {
            let href = if owner == "Unassigned" {
                "/work?focus=unassigned".to_string()
            } else if owner == session.username {
                "/work?focus=assigned".to_string()
            } else {
                let query =
                    serde_urlencoded::to_string([("status", "active"), ("owner", owner.as_str())])
                        .unwrap_or_else(|_| "status=active".to_string());
                format!("/incidents?{query}")
            };
            WorkMetric {
                label: owner,
                count,
                percent: if ownership_total == 0 {
                    0
                } else {
                    (count * 100 / ownership_total) as i32
                },
                href,
                tone: "neutral".to_string(),
            }
        })
        .collect::<Vec<_>>();
    let age_metrics = [
        ("< 1 day", "new cases", 0_i64, 0_i64),
        ("1–7 days", "recent backlog", 1_i64, 7_i64),
        ("8–30 days", "aging backlog", 8_i64, 30_i64),
        ("30+ days", "long-running cases", 31_i64, i64::MAX),
    ]
    .into_iter()
    .map(|(label, detail, minimum, maximum)| {
        let count = active_cases
            .iter()
            .filter(|item| item.age_days >= minimum && item.age_days <= maximum)
            .count();
        let key = match minimum {
            0 => "0_1",
            1 => "1_7",
            8 => "8_30",
            _ => "31_plus",
        };
        WorkAgeMetric {
            label: label.to_string(),
            count,
            percent: if active_cases.is_empty() {
                0
            } else {
                (count * 100 / active_cases.len()) as i32
            },
            detail: detail.to_string(),
            href: work_scope_href(&query.q, &severity, &sla, &focus, &age, ("age", key)),
        }
    })
    .collect::<Vec<_>>();
    Html(
        WorkTemplate {
            show_nav: true,
            is_admin,
            can_manage,
            username: session.username,
            my_incidents,
            unassigned_incidents,
            other_incidents,
            review_actions,
            error,
            notice: query.notice,
            claimed_count: query.claimed_count,
            claim_failed_count: query.claim_failed_count,
            focus,
            q: query.q,
            severity,
            sla,
            age,
            saved_views,
            severity_metrics,
            sla_metrics,
            ownership_metrics,
            age_metrics,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveWorkViewForm {
    name: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    sla: String,
    #[serde(default)]
    focus: String,
    #[serde(default)]
    age: String,
}

fn work_view_redirect(form: &SaveWorkViewForm, notice: &str) -> axum::response::Redirect {
    let query = serde_urlencoded::to_string([
        ("q", form.q.clone()),
        ("severity", normalize_severity(&form.severity)),
        ("sla", normalize_sla(&form.sla)),
        ("focus", normalize_focus(&form.focus)),
        ("age", normalize_age(&form.age)),
        ("notice", notice.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    axum::response::Redirect::to(&format!("/work?{query}"))
}

pub async fn post_save_work_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<SaveWorkViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if form.name.trim().is_empty() {
        return work_view_redirect(&form, "view_invalid").into_response();
    }
    let filter = serde_json::json!({
        "view_kind": "work",
        "q": form.q,
        "severity": normalize_severity(&form.severity),
        "sla": normalize_sla(&form.sla),
        "focus": normalize_focus(&form.focus),
        "age": normalize_age(&form.age),
    });
    match state
        .saved_search_queries_client
        .create(session.tenant_id, form.name.trim(), filter)
        .await
    {
        Ok(_) => work_view_redirect(&form, "view_saved").into_response(),
        Err(_) => work_view_redirect(&form, "view_failed").into_response(),
    }
}

pub async fn post_delete_work_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(()) => axum::response::Redirect::to("/work?notice=view_deleted").into_response(),
        Err(_) => axum::response::Redirect::to("/work?notice=view_failed").into_response(),
    }
}

/// GET /work/export.csv — exports the same filtered active workload shown in My Work. The
/// export is a handoff artifact, not a second queue: it includes assigned/unassigned cases and
/// governed decisions requiring review, honoring the current text, severity, and focus scope.
pub async fn get_work_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let focus = normalize_focus(&query.focus);
    let severity = normalize_severity(&query.severity);
    let sla = normalize_sla(&query.sla);
    let age = normalize_age(&query.age);
    let mut csv = String::from("queue,id,title,severity,status,executed_at,source\n");
    let incidents = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(items) => items,
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    };
    for detail in incidents {
        if detail.incident.status == IncidentStatus::Resolved {
            continue;
        }
        let is_mine = detail.incident.assigned_to.as_deref() == Some(session.username.as_str());
        let queue = if is_mine {
            "assigned"
        } else if detail.incident.assigned_to.is_none() {
            "unassigned"
        } else {
            continue;
        };
        if !focus.is_empty() && focus != queue {
            continue;
        }
        let incident = to_work_incident(detail, chrono::Utc::now());
        if (!severity.is_empty() && incident.severity != severity)
            || (!sla.is_empty() && incident.sla_state != sla)
            || (!age.is_empty() && !matches_age(incident.age_days, &age))
            || !matches_work_text(
                &format!("{} {} {}", incident.title, incident.severity, incident.status),
                &query.q,
            )
        {
            continue;
        }
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            queue,
            incident.id,
            csv_escape(&incident.title),
            incident.severity,
            incident.status,
            "",
            "incident"
        ));
    }
    if focus.is_empty() || focus == "review" {
        if let Some(client) = ontology_client::global() {
            let (types, invocations) = tokio::join!(
                client.list_action_types(&session.bearer_token),
                client.list_action_invocations(&session.bearer_token)
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
                    .cloned()
                    .unwrap_or_else(|| "Unknown governed action".to_string());
                if !matches_work_text(&format!("{} {}", name, invocation.outcome), &query.q) {
                    continue;
                }
                csv.push_str(&format!(
                    "review,{},{},{},{},{},{}\n",
                    invocation.id,
                    csv_escape(&name),
                    "",
                    csv_escape(&invocation.outcome),
                    invocation.executed_at.to_rfc3339(),
                    "action"
                ));
            }
        }
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"work-{}.csv\"", session.tenant_id).parse().unwrap(),
    );
    (response_headers, csv).into_response()
}

/// POST /work/bulk-claim — claims selected unassigned active cases through the same full
/// incident update path as the single-case action. Re-reading each case makes concurrent claims
/// safe: a case taken by another operator is reported as a partial failure, never overwritten.
pub async fn post_bulk_claim_work(
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
    let query_value = |key: &str| {
        pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let mut ids = pairs
        .iter()
        .filter(|(key, _)| key == "ids")
        .filter_map(|(_, value)| value.parse::<uuid::Uuid>().ok())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        let q = query_value("q");
        let severity = query_value("severity");
        let sla = query_value("sla");
        let age = query_value("age");
        let focus = query_value("focus");
        let query = serde_urlencoded::to_string([
            ("notice", "claim_empty".to_string()),
            ("q", q),
            ("severity", severity),
            ("sla", sla),
            ("age", age),
            ("focus", focus),
        ])
        .unwrap_or_else(|_| "notice=claim_empty".to_string());
        return axum::response::Redirect::to(&format!("/work?{query}")).into_response();
    }
    let mut claimed_count = 0usize;
    let mut claim_failed_count = 0usize;
    for id in ids {
        let Ok(Some(detail)) = state.incidents_client.get_incident(session.tenant_id, id).await
        else {
            claim_failed_count += 1;
            continue;
        };
        if detail.incident.status == IncidentStatus::Resolved
            || detail.incident.assigned_to.is_some()
        {
            claim_failed_count += 1;
            continue;
        }
        let updated = common::Incident {
            assigned_to: Some(session.username.clone()),
            updated_at: chrono::Utc::now(),
            ..detail.incident
        };
        match state.incidents_client.update_incident(session.role, &session.username, updated).await
        {
            Ok(_) => claimed_count += 1,
            Err(_) => claim_failed_count += 1,
        }
    }
    let q = query_value("q");
    let severity = query_value("severity");
    let sla = query_value("sla");
    let age = query_value("age");
    let focus = query_value("focus");
    let query = serde_urlencoded::to_string([
        ("notice", "bulk_claimed".to_string()),
        ("claimed_count", claimed_count.to_string()),
        ("claim_failed_count", claim_failed_count.to_string()),
        ("q", q),
        ("severity", severity),
        ("sla", sla),
        ("age", age),
        ("focus", focus),
    ])
    .unwrap_or_else(|_| "notice=bulk_claimed".to_string());
    axum::response::Redirect::to(&format!("/work?{query}")).into_response()
}
