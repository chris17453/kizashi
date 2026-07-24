#[path = "overview_handler_test.rs"]
#[cfg(test)]
mod overview_handler_test;

use crate::attention_summary_handler::unique_attention_case_count;
use crate::events_client::EventSummary;
use crate::ingestion_stats_client::RecordSearchFilter;
use crate::ontology_client;
use crate::session_guard::require_session;
use crate::topology::{build_topology_items, TopologyItem};
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{Datelike, Duration, Utc};

/// How many of the most recent events show up in the dashboard's "Recent Activity" preview —
/// a glance, not a replacement for the full paginated Events page it links to.
const RECENT_ACTIVITY_LIMIT: usize = 5;

#[derive(Template)]
#[template(path = "overview.html")]
struct OverviewTemplate {
    show_nav: bool,
    is_admin: bool,
    tenant_id: uuid::Uuid,
    username: String,
    initial_layout: String,
    sensor_count: usize,
    active_sensor_count: usize,
    stale_sensor_count: usize,
    total_records: i64,
    normalized_records: usize,
    unmodeled_records: usize,
    model_coverage_pct: usize,
    event_count: usize,
    open_incident_count: usize,
    critical_incident_count: usize,
    sla_breached_count: usize,
    platform_status: String,
    services_up: usize,
    services_total: usize,
    /// Compact preview of the same topology `/pipeline` shows in full — turns the dashboard
    /// landing page into something with real content below the KPI row, not just a link list.
    pipeline_items: Vec<TopologyItem>,
    /// The most recent events (already sorted newest-first by the backend), capped at
    /// `RECENT_ACTIVITY_LIMIT` — fills the dead space below the pipeline card with real content
    /// instead of leaving it empty.
    recent_events: Vec<EventSummary>,
    signal_heatmap: Vec<OverviewHeatCell>,
    signal_peak_count: u64,
    signal_trend_chart_json: String,
    recent_incidents: Vec<OverviewIncident>,
    ontology_types: usize,
    ontology_objects: usize,
    ontology_links: usize,
    ontology_actions: usize,
    completed_actions: usize,
    review_actions: usize,
    stale_reviews: usize,
    recent_actions: Vec<OverviewAction>,
    /// One entry per backend call that failed -- every KPI tile used to `.unwrap_or_default()`
    /// silently, so a genuine outage rendered as a plausible-looking "0 sensors / 0 records / 0
    /// events" dashboard, indistinguishable from an idle-but-healthy tenant. Surfaced the same
    /// way `security_overview_handler.rs` already does for its own KPI tiles.
    errors: Vec<String>,
    signal_from: String,
    signal_to: String,
    signal_window_label: String,
    saved_views: Vec<DashboardSavedView>,
    attention: OverviewAttention,
    brief: OverviewBrief,
    operating_chain: Vec<OperatingChainStage>,
    entity_risk_metrics: Vec<OverviewRiskMetric>,
    top_risk_entities: Vec<OverviewRiskEntity>,
}

struct OperatingChainStage {
    label: String,
    count: String,
    detail: String,
    href: String,
    tone: String,
    bar_pct: usize,
}

fn build_operating_chain(
    total_records: i64,
    normalized_records: usize,
    signal_count: usize,
    open_incident_count: usize,
    completed_actions: usize,
    signal_from: &str,
    signal_to: &str,
) -> Vec<OperatingChainStage> {
    let mut stages = vec![
        OperatingChainStage {
            label: "Ingest".to_string(),
            count: total_records.to_string(),
            detail: "raw records received".to_string(),
            href: "/data".to_string(),
            tone: "accent".to_string(),
            bar_pct: 0,
        },
        OperatingChainStage {
            label: "Normalize".to_string(),
            count: normalized_records.to_string(),
            detail: "records ready for modeling".to_string(),
            href: "/data?normalized=true".to_string(),
            tone: "good".to_string(),
            bar_pct: 0,
        },
        OperatingChainStage {
            label: "Detect".to_string(),
            count: signal_count.to_string(),
            detail: "signals in selected window".to_string(),
            href: format!("/events?from={signal_from}&to={signal_to}"),
            tone: "warning".to_string(),
            bar_pct: 0,
        },
        OperatingChainStage {
            label: "Investigate".to_string(),
            count: open_incident_count.to_string(),
            detail: "open cases in triage".to_string(),
            href: "/incidents?status=open".to_string(),
            tone: "risk".to_string(),
            bar_pct: 0,
        },
        OperatingChainStage {
            label: "Respond".to_string(),
            count: completed_actions.to_string(),
            detail: "completed governed decisions".to_string(),
            href: "/actions?outcome=completed".to_string(),
            tone: "good".to_string(),
            bar_pct: 0,
        },
    ];
    // These stages are intentionally not treated as a strict funnel: one signal can create
    // multiple governed decisions. Normalize bars to the busiest stage so the visualization
    // communicates relative operating volume without implying a false one-to-one conversion.
    let denominator = stages
        .iter()
        .filter_map(|stage| stage.count.parse::<usize>().ok())
        .max()
        .unwrap_or_else(|| total_records.max(1) as usize);
    for stage in &mut stages {
        stage.bar_pct = stage
            .count
            .parse::<usize>()
            .unwrap_or_default()
            .saturating_mul(100)
            .checked_div(denominator)
            .unwrap_or(0)
            .min(100);
    }
    stages
}

struct OverviewAttention {
    total: usize,
    critical_cases: usize,
    unassigned_work: usize,
    review_actions: usize,
    sla_breaches: usize,
    critical_queues: usize,
    connector_attention: usize,
}

struct OverviewBrief {
    signal_summary: String,
    signal_detail: String,
    ownership_summary: String,
    ownership_detail: String,
    response_summary: String,
    response_detail: String,
}

struct OverviewRiskMetric {
    key: String,
    label: String,
    count: usize,
    percent: i32,
}

struct OverviewRiskEntity {
    id: uuid::Uuid,
    label: String,
    type_name: String,
    score: usize,
    tone: String,
    detail: String,
}

struct DashboardSavedView {
    id: uuid::Uuid,
    name: String,
    surface: String,
    load_url: String,
}

const DASHBOARD_WIDGETS: &[&str] = &[
    "signal-trend",
    "governed-decisions",
    "knowledge-model",
    "pipeline-status",
    "recent-activity",
    "decision-queue",
];

fn dashboard_layout_for_user(
    queries: &[common::SavedSearchQuery],
    username: &str,
) -> Option<Vec<String>> {
    queries.iter().rev().find_map(|query| {
        let filter = query.filter.as_object()?;
        if filter.get("view_kind")?.as_str()? != "dashboard_layout"
            || filter.get("owner")?.as_str()? != username
        {
            return None;
        }
        let order = filter
            .get("order")?
            .as_array()?
            .iter()
            .filter_map(|value| value.as_str())
            .filter(|id| DASHBOARD_WIDGETS.contains(id))
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!order.is_empty()).then_some(order)
    })
}

fn valid_dashboard_order(raw: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let order = value
        .as_array()?
        .iter()
        .filter_map(|value| value.as_str())
        .filter(|id| DASHBOARD_WIDGETS.contains(id))
        .map(str::to_string)
        .collect::<Vec<_>>();
    (order.len() == DASHBOARD_WIDGETS.len()
        && DASHBOARD_WIDGETS.iter().all(|id| order.iter().any(|value| value == id)))
    .then_some(order)
}

fn dashboard_saved_views(queries: &[common::SavedSearchQuery]) -> Vec<DashboardSavedView> {
    queries
        .iter()
        .filter_map(|query| {
            let filter = query.filter.as_object()?;
            if filter.get("view_kind").and_then(serde_json::Value::as_str)
                == Some("dashboard_layout")
                || filter.get("view_kind").and_then(serde_json::Value::as_str)
                    == Some("report_schedule")
            {
                return None;
            }
            let (surface, excluded): (&str, &[&str]) =
                if let Some(surface) = filter.get("surface").and_then(serde_json::Value::as_str) {
                    (surface, &["surface"])
                } else if filter.keys().any(|key| {
                    matches!(
                        key.as_str(),
                        "connector_id"
                            | "source_type"
                            | "subject"
                            | "email_from"
                            | "attachment_filename"
                            | "normalized"
                    )
                }) {
                    // Data Explorer bookmarks predate the common `surface`/`view_kind` marker;
                    // their typed filter fields are the stable discriminator.
                    ("data", &[])
                } else {
                    match filter.get("view_kind").and_then(serde_json::Value::as_str)? {
                        "events" => ("events", &["view_kind"]),
                        "actions" => ("actions", &["view_kind"]),
                        "work" => ("work", &["view_kind"]),
                        "global-search" => ("search", &["view_kind"]),
                        "reports" => ("reports", &["view_kind"]),
                        _ => return None,
                    }
                };
            let params = filter
                .iter()
                .filter(|(key, _)| {
                    !excluded.contains(&key.as_str()) && *key != "owner" && *key != "name"
                })
                .filter_map(|(key, value)| {
                    if value.is_null() {
                        return None;
                    }
                    let value =
                        value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
                    (!value.is_empty()).then_some((key.clone(), value))
                })
                .collect::<Vec<_>>();
            let load_url = format!(
                "/{}{}",
                surface,
                if params.is_empty() {
                    String::new()
                } else {
                    format!("?{}", serde_urlencoded::to_string(params).unwrap_or_default())
                }
            );
            Some(DashboardSavedView {
                id: query.id,
                name: query.name.clone(),
                surface: surface.to_string(),
                load_url,
            })
        })
        .collect()
}

struct OverviewIncident {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
    event_count: usize,
}

struct OverviewAction {
    action_name: String,
    outcome: String,
    target_count: usize,
    executed_at: chrono::DateTime<chrono::Utc>,
    event_id: Option<uuid::Uuid>,
    incident_id: Option<uuid::Uuid>,
    targets: Vec<OverviewActionTarget>,
}

struct OverviewActionTarget {
    id: uuid::Uuid,
    label: String,
}

struct OverviewHeatCell {
    date: String,
    count: u64,
    intensity: u8,
    blank: bool,
}

fn signal_trend_chart_json(counts: &[crate::events_client::DailyCount]) -> String {
    serde_json::json!({
        "title": "Daily signal activity",
        "labels": counts.iter().map(|item| item.date.clone()).collect::<Vec<_>>(),
        "values": counts.iter().map(|item| item.count).collect::<Vec<_>>(),
        "hrefs": counts.iter().map(|item| format!("/events?from={}&to={}", item.date, item.date)).collect::<Vec<_>>(),
    })
    .to_string()
}

fn build_signal_heatmap(counts: &[crate::events_client::DailyCount]) -> Vec<OverviewHeatCell> {
    let max = counts.iter().map(|item| item.count).max().unwrap_or(0);
    let first_weekday = counts
        .first()
        .and_then(|item| chrono::NaiveDate::parse_from_str(&item.date, "%Y-%m-%d").ok())
        .map(|date| date.weekday().num_days_from_monday() as usize)
        .unwrap_or(0);
    let mut cells = Vec::with_capacity(first_weekday + counts.len());
    for _ in 0..first_weekday {
        cells.push(OverviewHeatCell { date: String::new(), count: 0, intensity: 0, blank: true });
    }
    cells.extend(counts.iter().map(|item| OverviewHeatCell {
        date: item.date.clone(),
        count: item.count,
        intensity: if item.count == 0 || max == 0 {
            0
        } else {
            ((item.count * 4 + max - 1) / max).clamp(1, 4) as u8
        },
        blank: false,
    }));
    while cells.len() % 7 != 0 {
        cells.push(OverviewHeatCell { date: String::new(), count: 0, intensity: 0, blank: true });
    }
    cells
}

fn incident_sla_breached(incident: &common::Incident, now: chrono::DateTime<chrono::Utc>) -> bool {
    let target = match incident.severity {
        common::IncidentSeverity::Critical => chrono::Duration::minutes(15),
        common::IncidentSeverity::High => chrono::Duration::hours(1),
        common::IncidentSeverity::Medium => chrono::Duration::hours(4),
        common::IncidentSeverity::Low => chrono::Duration::hours(24),
    };
    let end = incident.resolved_at.unwrap_or(now);
    end.signed_duration_since(incident.created_at) > target
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct OverviewQuery {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
}

fn signal_window(
    query: &OverviewQuery,
) -> (String, String, chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let today = Utc::now().date_naive();
    let to_date = chrono::NaiveDate::parse_from_str(&query.to, "%Y-%m-%d").unwrap_or(today);
    let requested_from = chrono::NaiveDate::parse_from_str(&query.from, "%Y-%m-%d")
        .unwrap_or(to_date - Duration::days(29));
    let from_date = if requested_from <= to_date { requested_from } else { to_date };
    let from = from_date.and_hms_opt(0, 0, 0).unwrap();
    let until = to_date.and_hms_opt(23, 59, 59).unwrap();
    (
        from_date.to_string(),
        to_date.to_string(),
        chrono::DateTime::<Utc>::from_naive_utc_and_offset(from, Utc),
        chrono::DateTime::<Utc>::from_naive_utc_and_offset(until, Utc),
    )
}

/// GET /overview — the landing dashboard: KPI cards summarizing sensors, ingestion volume,
/// events, and platform health at a glance, each pulled from the same backends every other
/// page already reads (no new data path — just presented as tiles instead of a table).
pub async fn get_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OverviewQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let (signal_from, signal_to, signal_since, signal_until) = signal_window(&query);
    let signal_window_label = format!("{} → {}", signal_from, signal_to);
    let saved_queries =
        state.saved_search_queries_client.list(session.tenant_id).await.unwrap_or_default();
    let initial_layout =
        serde_json::to_string(&dashboard_layout_for_user(&saved_queries, &session.username))
            .unwrap_or_else(|_| "null".to_string());
    let saved_views = dashboard_saved_views(&saved_queries);

    let mut errors = Vec::new();

    // Capped at 1000, same tradeoff as the events count below — a KPI tile approximates at
    // very high sensor counts rather than needing an exact total.
    let sensors = match state.sensors_client.list_sensors(session.tenant_id, 1000, 0).await {
        Ok(page) => page.sensors,
        Err(e) => {
            errors.push(format!("sensors: {e}"));
            vec![]
        }
    };
    let connector_stats = match state.stats_client.connector_stats(session.tenant_id).await {
        Ok(stats) => stats,
        Err(e) => {
            errors.push(format!("ingestion stats: {e}"));
            vec![]
        }
    };
    // Capped at 1000 (the same ceiling the backend itself clamps to) — a KPI tile approximates
    // at very high volume rather than needing an exact count, same tradeoff this dashboard
    // already made before pagination existed (it used to silently cap at the default limit).
    let events = match state
        .events_client
        .list_events(&session.bearer_token, 1000, 0, Some(signal_since), Some(signal_until))
        .await
    {
        Ok(page) => page.events,
        Err(e) => {
            errors.push(format!("events: {e}"));
            vec![]
        }
    };
    let health = match state.health_client.platform_health().await {
        Ok(h) => Some(h),
        Err(e) => {
            errors.push(format!("platform health: {e}"));
            None
        }
    };
    let depths = match state.backlog_client.queue_depths().await {
        Ok(depths) => depths,
        Err(e) => {
            errors.push(format!("queue depths: {e}"));
            vec![]
        }
    };
    let pipeline_items =
        health.as_ref().map(|h| build_topology_items(h, &depths)).unwrap_or_default();

    let active_connector_ids: std::collections::HashSet<&str> =
        connector_stats.iter().map(|s| s.connector_id.as_str()).collect();
    let active_sensor_count =
        sensors.iter().filter(|a| active_connector_ids.contains(a.name.as_str())).count();
    let now = Utc::now();
    let stale_sensor_count = sensors
        .iter()
        .filter(|sensor| {
            if !sensor.enabled {
                return false;
            }
            connector_stats
                .iter()
                .find(|stat| stat.connector_id == sensor.name)
                .map(|stat| now - stat.last_ingested_at > Duration::hours(1))
                .unwrap_or(true)
        })
        .count();
    let total_records: i64 = connector_stats.iter().map(|s| s.record_count).sum();
    let normalized_records = match state
        .stats_client
        .search_records(
            session.tenant_id,
            &RecordSearchFilter { normalized: Some(true), limit: 1000, ..Default::default() },
        )
        .await
    {
        Ok(result) => result.records.len(),
        Err(e) => {
            errors.push(format!("normalized records: {e}"));
            0
        }
    };
    let unmodeled_records = match state
        .stats_client
        .search_records(
            session.tenant_id,
            &RecordSearchFilter { normalized: Some(false), limit: 1000, ..Default::default() },
        )
        .await
    {
        Ok(result) => result.records.len(),
        Err(e) => {
            errors.push(format!("unnormalized records: {e}"));
            0
        }
    };
    let model_record_total = normalized_records + unmodeled_records;
    let model_coverage_pct =
        if model_record_total == 0 { 100 } else { normalized_records * 100 / model_record_total };

    let (platform_status, services_up, services_total) = match &health {
        Some(h) => {
            let up = h.services.iter().filter(|s| s.status == "up").count();
            (h.status.clone(), up, h.services.len())
        }
        None => ("unknown".to_string(), 0, 0),
    };

    let recent_events = events.iter().take(RECENT_ACTIVITY_LIMIT).cloned().collect();
    let (signal_heatmap, signal_peak_count, signal_trend_chart_json) = match state
        .events_client
        .daily_counts(&session.bearer_token, signal_since, signal_until)
        .await
    {
        Ok(counts) => {
            let peak = counts.iter().map(|item| item.count).max().unwrap_or(0);
            let heatmap = build_signal_heatmap(&counts);
            let chart = signal_trend_chart_json(&counts);
            (heatmap, peak, chart)
        }
        Err(e) => {
            errors.push(format!("signal trend: {e}"));
            (vec![], 0, signal_trend_chart_json(&[]))
        }
    };

    let incidents = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(incidents) => incidents,
        Err(e) => {
            errors.push(format!("incidents: {e}"));
            vec![]
        }
    };
    let open_incident_count = incidents
        .iter()
        .filter(|incident| incident.incident.status != common::IncidentStatus::Resolved)
        .count();
    let critical_incident_count = incidents
        .iter()
        .filter(|incident| {
            incident.incident.status != common::IncidentStatus::Resolved
                && matches!(incident.incident.severity, common::IncidentSeverity::Critical)
        })
        .count();
    let now = chrono::Utc::now();
    let sla_breached_count = incidents
        .iter()
        .filter(|incident| {
            incident.incident.status != common::IncidentStatus::Resolved
                && incident_sla_breached(&incident.incident, now)
        })
        .count();
    let mut recent_incidents = incidents
        .iter()
        .filter(|incident| incident.incident.status != common::IncidentStatus::Resolved)
        .collect::<Vec<_>>();
    recent_incidents.sort_by(|left, right| {
        let severity_rank = |severity: common::IncidentSeverity| match severity {
            common::IncidentSeverity::Critical => 4,
            common::IncidentSeverity::High => 3,
            common::IncidentSeverity::Medium => 2,
            common::IncidentSeverity::Low => 1,
        };
        severity_rank(right.incident.severity)
            .cmp(&severity_rank(left.incident.severity))
            .then_with(|| right.incident.updated_at.cmp(&left.incident.updated_at))
    });
    let recent_incidents = recent_incidents
        .into_iter()
        .take(4)
        .map(|incident| OverviewIncident {
            id: incident.incident.id,
            title: incident.incident.title.clone(),
            severity: incident.incident.severity.to_string(),
            status: incident.incident.status.to_string(),
            event_count: incident.event_ids.len(),
        })
        .collect();

    let (
        ontology_types,
        ontology_objects,
        ontology_links,
        ontology_actions,
        completed_actions,
        review_actions,
        stale_reviews,
        recent_actions,
        entity_risk_metrics,
        top_risk_entities,
    ) = match ontology_client::global() {
        Some(client) => {
            let types = match client.list_object_types(&session.bearer_token).await {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("ontology types: {error}"));
                    vec![]
                }
            };
            let objects = match client.list_objects(&session.bearer_token, None).await {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("ontology objects: {error}"));
                    vec![]
                }
            };
            let links = match client.list_link_types(&session.bearer_token).await {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("ontology relationships: {error}"));
                    vec![]
                }
            };
            let action_types = match client.list_action_types(&session.bearer_token).await {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("ontology action definitions: {error}"));
                    vec![]
                }
            };
            let actions = match client.list_action_invocations(&session.bearer_token).await {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("ontology actions: {error}"));
                    vec![]
                }
            };
            let object_titles = objects
                .iter()
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
            let action_names = action_types
                .into_iter()
                .map(|action| (action.id, action.name))
                .collect::<std::collections::HashMap<_, _>>();
            let completed_actions = actions
                .iter()
                .filter(|action| action.outcome.eq_ignore_ascii_case("completed"))
                .count();
            let review_actions = actions.len().saturating_sub(completed_actions);
            let reviews = match client.list_action_reviews(&session.bearer_token).await {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!("ontology action reviews: {error}"));
                    vec![]
                }
            };
            let stale_reviews = reviews
                .iter()
                .filter(|review| {
                    !matches!(review.status.as_str(), "approved" | "declined")
                        && review.due_at.is_some_and(|due_at| due_at <= Utc::now())
                })
                .count();
            let object_type_names = types
                .iter()
                .map(|object_type| (object_type.id, object_type.name.clone()))
                .collect::<std::collections::HashMap<_, _>>();
            let mut risk_entities = objects
                .iter()
                .map(|object| {
                    let lineage_ids = object
                        .source_lineage
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|value| value.as_str())
                        .filter_map(|value| uuid::Uuid::parse_str(value).ok())
                        .collect::<std::collections::HashSet<_>>();
                    let linked_event_ids = events
                        .iter()
                        .filter(|event| {
                            event.record_ids.iter().any(|record_id| lineage_ids.contains(record_id))
                        })
                        .map(|event| event.id)
                        .collect::<std::collections::HashSet<_>>();
                    let linked_incidents = incidents.iter().filter(|incident| {
                        incident
                            .event_ids
                            .iter()
                            .any(|event_id| linked_event_ids.contains(event_id))
                    });
                    let mut score = 0usize;
                    let mut reasons = Vec::new();
                    for incident in linked_incidents.filter(|incident| {
                        incident.incident.status != common::IncidentStatus::Resolved
                    }) {
                        let (weight, label) = match incident.incident.severity {
                            common::IncidentSeverity::Critical => (65, "critical case"),
                            common::IncidentSeverity::High => (45, "high-severity case"),
                            common::IncidentSeverity::Medium => (25, "medium-severity case"),
                            common::IncidentSeverity::Low => (12, "open case"),
                        };
                        score += weight;
                        if reasons.len() < 2 {
                            reasons.push(format!("{label}: {}", incident.incident.title));
                        }
                    }
                    for event in events.iter().filter(|event| linked_event_ids.contains(&event.id))
                    {
                        score += if event.status == "triggered" { 18 } else { 10 };
                        if reasons.len() < 2 {
                            reasons.push(format!("signal: {}", event.event_type));
                        }
                    }
                    for _action in actions.iter().filter(|action| {
                        action.target_object_ids.as_array().into_iter().flatten().any(|target| {
                            target.as_str().and_then(|value| uuid::Uuid::parse_str(value).ok())
                                == Some(object.id)
                        }) && !action.outcome.eq_ignore_ascii_case("completed")
                    }) {
                        score += 8;
                        if reasons.len() < 2 {
                            reasons.push("governed decision needs review".to_string());
                        }
                    }
                    let score = score.min(100);
                    let (tone, label) = match score {
                        0 => ("good", "Stable"),
                        1..=25 => ("neutral", "Monitored"),
                        26..=59 => ("warning", "Needs attention"),
                        _ => ("critical", "Critical attention"),
                    };
                    OverviewRiskEntity {
                        id: object.id,
                        label: object
                            .properties
                            .get("name")
                            .or_else(|| object.properties.get("subject"))
                            .or_else(|| object.properties.get("id"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("Untitled entity")
                            .to_string(),
                        type_name: object_type_names
                            .get(&object.object_type_id)
                            .cloned()
                            .unwrap_or_else(|| "Entity".to_string()),
                        score,
                        tone: tone.to_string(),
                        detail: if reasons.is_empty() {
                            label.to_string()
                        } else {
                            reasons.join(" · ")
                        },
                    }
                })
                .collect::<Vec<_>>();
            risk_entities.sort_by(|left, right| right.score.cmp(&left.score));
            let total_risk_entities = risk_entities.len();
            let entity_risk_metrics = [
                ("critical", "Critical attention"),
                ("warning", "Needs attention"),
                ("neutral", "Monitored"),
                ("good", "Stable"),
            ]
            .into_iter()
            .map(|(key, label)| {
                let count = risk_entities.iter().filter(|entity| entity.tone == key).count();
                OverviewRiskMetric {
                    key: key.to_string(),
                    label: label.to_string(),
                    count,
                    percent: if total_risk_entities == 0 {
                        0
                    } else {
                        (count * 100 / total_risk_entities) as i32
                    },
                }
            })
            .collect::<Vec<_>>();
            risk_entities.truncate(5);
            let mut recent_actions = actions;
            recent_actions.sort_by(|left, right| right.executed_at.cmp(&left.executed_at));
            let recent_actions = recent_actions
                .into_iter()
                .take(5)
                .map(|action| {
                    let event_id = action
                        .triggering_event_ref
                        .get("event_id")
                        .or_else(|| action.triggering_event_ref.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| uuid::Uuid::parse_str(value).ok());
                    let incident_id = action
                        .triggering_event_ref
                        .get("incident_id")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| uuid::Uuid::parse_str(value).ok());
                    let targets = action
                        .target_object_ids
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|value| value.as_str())
                        .filter_map(|value| uuid::Uuid::parse_str(value).ok())
                        .map(|id| OverviewActionTarget {
                            id,
                            label: object_titles
                                .get(&id)
                                .cloned()
                                .unwrap_or_else(|| id.to_string()),
                        })
                        .collect();
                    OverviewAction {
                        action_name: action_names
                            .get(&action.action_type_id)
                            .cloned()
                            .unwrap_or_else(|| "Unknown action".to_string()),
                        outcome: action.outcome,
                        target_count: action
                            .target_object_ids
                            .as_array()
                            .map(|values| values.len())
                            .unwrap_or(0),
                        executed_at: action.executed_at,
                        event_id,
                        incident_id,
                        targets,
                    }
                })
                .collect();
            (
                types.len(),
                objects.len(),
                links.len(),
                completed_actions + review_actions,
                completed_actions,
                review_actions,
                stale_reviews,
                recent_actions,
                entity_risk_metrics,
                risk_entities,
            )
        }
        None => {
            errors.push("ontology: client unavailable".to_string());
            (0, 0, 0, 0, 0, 0, 0, vec![], vec![], vec![])
        }
    };

    let critical_cases = critical_incident_count;
    let unassigned_work = incidents
        .iter()
        .filter(|incident| {
            incident.incident.status != common::IncidentStatus::Resolved
                && incident.incident.assigned_to.is_none()
        })
        .count();
    let critical_queues = depths
        .iter()
        .filter(|queue| crate::topology::severity_for(queue.messages) == "critical")
        .count();
    let attention = OverviewAttention {
        total: unique_attention_case_count(&incidents, now)
            + review_actions
            + critical_queues
            + stale_sensor_count,
        critical_cases,
        unassigned_work,
        review_actions,
        sla_breaches: sla_breached_count,
        critical_queues,
        connector_attention: stale_sensor_count,
    };
    let window_days = (signal_until.date_naive() - signal_since.date_naive()).num_days() + 1;
    let daily_signal_velocity = if window_days > 0 {
        events.len() as f64 / window_days as f64
    } else {
        events.len() as f64
    };
    let assigned_open_work = open_incident_count.saturating_sub(unassigned_work);
    let response_total = completed_actions + review_actions;
    let response_percent = if response_total == 0 {
        100
    } else {
        ((completed_actions * 100) / response_total).min(100)
    };
    let brief = OverviewBrief {
        signal_summary: format!("{daily_signal_velocity:.1}/day"),
        signal_detail: format!("{} events across the selected window", events.len()),
        ownership_summary: if open_incident_count == 0 {
            "No open cases".to_string()
        } else {
            format!("{assigned_open_work}/{open_incident_count} owned")
        },
        ownership_detail: if unassigned_work == 0 {
            "Every open case has an owner".to_string()
        } else {
            format!(
                "{unassigned_work} open case{} awaiting ownership",
                if unassigned_work == 1 { "" } else { "s" }
            )
        },
        response_summary: format!("{response_percent}% complete"),
        response_detail: if review_actions == 0 {
            "No governed decisions waiting for review".to_string()
        } else {
            format!(
                "{review_actions} governed decision{} need review",
                if review_actions == 1 { "" } else { "s" }
            )
        },
    };
    let operating_chain = build_operating_chain(
        total_records,
        normalized_records,
        events.len(),
        open_incident_count,
        completed_actions,
        &signal_from,
        &signal_to,
    );

    Html(
        OverviewTemplate {
            show_nav: true,
            is_admin,
            tenant_id: session.tenant_id,
            username: session.username,
            initial_layout,
            sensor_count: sensors.len(),
            active_sensor_count,
            stale_sensor_count,
            total_records,
            normalized_records,
            unmodeled_records,
            model_coverage_pct,
            event_count: events.len(),
            open_incident_count,
            critical_incident_count,
            sla_breached_count,
            platform_status,
            services_up,
            services_total,
            pipeline_items,
            recent_events,
            signal_heatmap,
            signal_peak_count,
            signal_trend_chart_json,
            recent_incidents,
            ontology_types,
            ontology_objects,
            ontology_links,
            ontology_actions,
            completed_actions,
            review_actions,
            stale_reviews,
            recent_actions,
            errors,
            signal_from,
            signal_to,
            signal_window_label,
            saved_views,
            attention,
            brief,
            operating_chain,
            entity_risk_metrics,
            top_risk_entities,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct DashboardLayoutForm {
    order: String,
}

/// Persists the current per-user dashboard arrangement in the tenant-scoped saved-query store.
pub async fn post_dashboard_layout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<DashboardLayoutForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(order) = valid_dashboard_order(&form.order) else {
        return axum::http::StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    let queries =
        state.saved_search_queries_client.list(session.tenant_id).await.unwrap_or_default();
    for query in queries {
        let owned = query.filter.get("view_kind").and_then(serde_json::Value::as_str)
            == Some("dashboard_layout")
            && query.filter.get("owner").and_then(serde_json::Value::as_str)
                == Some(session.username.as_str());
        if owned {
            let _ = state.saved_search_queries_client.delete(session.tenant_id, query.id).await;
        }
    }
    let filter = serde_json::json!({"view_kind":"dashboard_layout", "owner":session.username, "order":order});
    match state
        .saved_search_queries_client
        .create(session.tenant_id, "Personal dashboard layout", filter)
        .await
    {
        Ok(_) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(_) => axum::http::StatusCode::BAD_GATEWAY.into_response(),
    }
}

pub async fn post_reset_dashboard_layout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let queries =
        state.saved_search_queries_client.list(session.tenant_id).await.unwrap_or_default();
    for query in queries {
        let owned = query.filter.get("view_kind").and_then(serde_json::Value::as_str)
            == Some("dashboard_layout")
            && query.filter.get("owner").and_then(serde_json::Value::as_str)
                == Some(session.username.as_str());
        if owned {
            let _ = state.saved_search_queries_client.delete(session.tenant_id, query.id).await;
        }
    }
    Redirect::to("/overview").into_response()
}

#[cfg(test)]
mod dashboard_layout_tests {
    use super::*;

    #[test]
    fn signal_window_uses_requested_inclusive_dates() {
        let (from, to, since, until) = signal_window(&OverviewQuery {
            from: "2026-07-01".to_string(),
            to: "2026-07-07".to_string(),
        });
        assert_eq!(from, "2026-07-01");
        assert_eq!(to, "2026-07-07");
        assert_eq!(since.to_rfc3339(), "2026-07-01T00:00:00+00:00");
        assert_eq!(until.to_rfc3339(), "2026-07-07T23:59:59+00:00");
    }

    #[test]
    fn accepts_only_the_complete_known_widget_set() {
        let order = serde_json::to_string(DASHBOARD_WIDGETS).unwrap();
        assert_eq!(valid_dashboard_order(&order).unwrap().len(), DASHBOARD_WIDGETS.len());
        assert!(valid_dashboard_order(r#"["signal-trend"]"#).is_none());
        assert!(valid_dashboard_order(r#"["signal-trend","signal-trend","knowledge-model","pipeline-status","recent-activity","decision-queue"]"#).is_none());
    }

    #[test]
    fn saved_operational_views_build_deep_links_from_saved_filters() {
        let query = common::SavedSearchQuery::new(
            uuid::Uuid::new_v4(),
            "New risk events",
            serde_json::json!({"view_kind":"events","q":"risk","status":"new"}),
        );
        let views = dashboard_saved_views(&[query]);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].surface, "events");
        assert!(views[0].load_url.contains("/events?"));
        assert!(views[0].load_url.contains("q=risk"));
        assert!(views[0].load_url.contains("status=new"));
    }

    #[test]
    fn data_explorer_bookmarks_are_discovered_without_a_surface_marker() {
        let query = common::SavedSearchQuery::new(
            uuid::Uuid::new_v4(),
            "Unnormalized tickets",
            serde_json::json!({"connector_id":"support","normalized":"false","page":0}),
        );
        let views = dashboard_saved_views(&[query]);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].surface, "data");
        assert!(views[0].load_url.contains("/data?"));
        assert!(views[0].load_url.contains("connector_id=support"));
        assert!(views[0].load_url.contains("normalized=false"));
    }
}
