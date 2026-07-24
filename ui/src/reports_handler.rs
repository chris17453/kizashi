#[path = "reports_handler_test.rs"]
#[cfg(test)]
mod reports_handler_test;

use crate::session_guard::require_session;
use crate::{ontology_client, AppState, ConnectorStatSummary, RecordSearchFilter};
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};

struct EventTypeCount {
    event_type: String,
    count: usize,
}

fn signal_trend_chart_json(events: &[crate::events_client::EventSummary]) -> String {
    let mut counts = std::collections::BTreeMap::<String, i64>::new();
    for event in events {
        *counts.entry(event.occurred_at.format("%Y-%m-%d").to_string()).or_default() += 1;
    }
    let (labels, values): (Vec<_>, Vec<_>) = counts.into_iter().unzip();
    let hrefs =
        labels.iter().map(|date| format!("/events?from={date}&to={date}")).collect::<Vec<_>>();
    chart_json_with_hrefs(&labels, &values, &hrefs)
}

fn previous_signal_window(
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    match (since, until) {
        (Some(since), Some(until)) => {
            let length = until.signed_duration_since(since);
            (
                Some(since - length - chrono::Duration::seconds(1)),
                Some(since - chrono::Duration::seconds(1)),
            )
        }
        _ => (None, None),
    }
}

struct IncidentReportRow {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
    event_count: usize,
    event_links: Vec<IncidentReportEvent>,
}

struct IncidentReportEvent {
    id: uuid::Uuid,
    event_type: String,
}

struct IncidentPostureMatrixRow {
    severity: String,
    open: usize,
    acknowledged: usize,
    resolved: usize,
    breached: usize,
    total: usize,
}

fn incident_posture_matrix(
    incidents: &[crate::incidents_client::IncidentDetail],
) -> Vec<IncidentPostureMatrixRow> {
    ["critical", "high", "medium", "low"]
        .into_iter()
        .map(|severity| {
            let matching = incidents
                .iter()
                .filter(|item| item.incident.severity.to_string() == severity)
                .collect::<Vec<_>>();
            IncidentPostureMatrixRow {
                severity: severity.to_string(),
                open: matching
                    .iter()
                    .filter(|item| item.incident.status == common::IncidentStatus::Open)
                    .count(),
                acknowledged: matching
                    .iter()
                    .filter(|item| item.incident.status == common::IncidentStatus::Acknowledged)
                    .count(),
                resolved: matching
                    .iter()
                    .filter(|item| item.incident.status == common::IncidentStatus::Resolved)
                    .count(),
                breached: matching
                    .iter()
                    .filter(|item| report_incident_sla_breached(&item.incident, Utc::now()))
                    .count(),
                total: matching.len(),
            }
        })
        .collect()
}

fn incident_posture_chart_json(rows: &[IncidentPostureMatrixRow]) -> String {
    let labels = ["Open", "Acknowledged", "Resolved", "SLA breached"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let values = [
        rows.iter().map(|row| row.open).sum::<usize>(),
        rows.iter().map(|row| row.acknowledged).sum::<usize>(),
        rows.iter().map(|row| row.resolved).sum::<usize>(),
        rows.iter().map(|row| row.breached).sum::<usize>(),
    ]
    .into_iter()
    .map(|value| value as i64)
    .collect::<Vec<_>>();
    let hrefs = [
        "/incidents?status=open",
        "/incidents?status=acknowledged",
        "/incidents?status=resolved",
        "/incidents?sla=breached",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    chart_json_with_hrefs(&labels, &values, &hrefs)
}

struct ActionReportRow {
    id: uuid::Uuid,
    action_name: String,
    outcome: String,
    target_count: usize,
    executed_at: DateTime<Utc>,
    event_id: Option<uuid::Uuid>,
    incident_id: Option<uuid::Uuid>,
    targets: Vec<ActionReportTarget>,
}

struct ActionReportTarget {
    id: uuid::Uuid,
    label: String,
}

struct OntologyCoverageRow {
    id: uuid::Uuid,
    name: String,
    object_count: usize,
    property_count: usize,
    relationship_count: usize,
}

#[derive(Clone)]
struct SavedReportView {
    id: uuid::Uuid,
    name: String,
    load_url: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
struct SavedReportFilter {
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
}

#[derive(Debug, serde::Deserialize, Default, Clone)]
pub struct ReportsQuery {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub notice: String,
}

fn parse_date_range(from: &str, to: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let start = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|date| DateTime::<Utc>::from_naive_utc_and_offset(date, Utc));
    let end = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(23, 59, 59))
        .map(|date| DateTime::<Utc>::from_naive_utc_and_offset(date, Utc));
    (start, end)
}

async fn connector_stats_for_window(
    state: &AppState,
    tenant_id: uuid::Uuid,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<(Vec<ConnectorStatSummary>, std::collections::HashSet<uuid::Uuid>), String> {
    let mut offset = 0i64;
    let mut by_connector = std::collections::HashMap::<String, (i64, DateTime<Utc>)>::new();
    let mut record_ids = std::collections::HashSet::new();
    for _ in 0..20 {
        let filter = RecordSearchFilter {
            from: since,
            to: until,
            limit: 500,
            offset,
            ..RecordSearchFilter::default()
        };
        let page = state
            .stats_client
            .search_records(tenant_id, &filter)
            .await
            .map_err(|error| error.to_string())?;
        let page_len = page.records.len() as i64;
        for record in page.records {
            record_ids.insert(record.id);
            let entry = by_connector.entry(record.connector_id).or_insert((0, record.ingested_at));
            entry.0 += 1;
            entry.1 = entry.1.max(record.ingested_at);
        }
        if !page.has_more || page_len == 0 {
            break;
        }
        offset += page_len;
    }
    let mut stats = by_connector
        .into_iter()
        .map(|(connector_id, (record_count, last_ingested_at))| ConnectorStatSummary {
            connector_id,
            record_count,
            last_ingested_at,
        })
        .collect::<Vec<_>>();
    stats.sort_by(|left, right| left.connector_id.cmp(&right.connector_id));
    Ok((stats, record_ids))
}

fn timestamp_in_window(
    timestamp: DateTime<Utc>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> bool {
    since.map(|value| timestamp >= value).unwrap_or(true)
        && until.map(|value| timestamp <= value).unwrap_or(true)
}

fn incident_in_window(
    incident: &crate::incidents_client::IncidentDetail,
    window_event_ids: &std::collections::HashSet<uuid::Uuid>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> bool {
    timestamp_in_window(incident.incident.created_at, since, until)
        || incident.event_ids.iter().any(|id| window_event_ids.contains(id))
}

fn object_in_window(
    object: &common::Object,
    window_record_ids: &std::collections::HashSet<uuid::Uuid>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> bool {
    if since.is_none() && until.is_none() {
        return true;
    }
    let has_window_lineage = object
        .source_lineage
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|value| uuid::Uuid::parse_str(value).ok())
        .any(|id| window_record_ids.contains(&id));
    has_window_lineage || timestamp_in_window(object.updated_at, since, until)
}

/// Serializes `{labels, values, hrefs}` for `static/charts.js` to read out of a `<script
/// type="application/json">` tag. Escapes `<` as `<` so a value containing the literal
/// text `</script>` (a connector_id or event_type an operator controls) can never prematurely
/// close the tag and inject arbitrary markup — standard practice for embedding JSON inside
/// `<script>`, since JSON's own escaping has no reason to touch `<`.
fn chart_json(labels: &[String], values: &[i64]) -> String {
    chart_json_with_hrefs(labels, values, &[])
}

fn chart_json_with_hrefs(labels: &[String], values: &[i64], hrefs: &[String]) -> String {
    #[derive(serde::Serialize)]
    struct ChartData<'a> {
        labels: &'a [String],
        values: &'a [i64],
        hrefs: &'a [String],
    }
    let value = ChartData { labels, values, hrefs };
    serde_json::to_string(&value).unwrap_or_default().replace('<', "\\u003c")
}

/// GET /reports — the platform-wide aggregate view: ingestion volume per connector (from
/// Ingestion Service) alongside event counts per event type (from Query Gateway's event feed).
/// Both are cheap aggregates over data this platform already stores; nothing new is computed
/// or persisted for this page.
#[derive(Template)]
#[template(path = "reports.html")]
struct ReportsTemplate {
    show_nav: bool,
    is_admin: bool,
    connector_stats: Vec<ConnectorStatSummary>,
    connector_stats_chart_json: String,
    event_counts: Vec<EventTypeCount>,
    event_counts_chart_json: String,
    signal_trend_chart_json: String,
    incident_posture_chart_json: String,
    open_incidents: usize,
    critical_incidents: usize,
    acknowledged_incidents: usize,
    sla_breached_incidents: usize,
    ontology_types: usize,
    ontology_objects: usize,
    ontology_coverage: Vec<OntologyCoverageRow>,
    completed_actions: usize,
    review_actions: usize,
    action_rows: Vec<ActionReportRow>,
    signal_count: usize,
    signal_delta: String,
    incident_rows: Vec<IncidentReportRow>,
    incident_matrix: Vec<IncidentPostureMatrixRow>,
    saved_views: Vec<SavedReportView>,
    from: String,
    to: String,
    notice: String,
    error: Option<String>,
    readiness_metrics: Vec<ReportReadinessMetric>,
    operating_funnel: Vec<ReportFunnelStage>,
    comparison_metrics: Vec<ReportComparisonMetric>,
}

struct ReportReadinessMetric {
    label: String,
    count: usize,
    percent: usize,
    detail: String,
    href: String,
    tone: String,
}

struct ReportFunnelStage {
    label: String,
    count: usize,
    percent: usize,
    detail: String,
    href: String,
    tone: String,
}

struct ReportComparisonMetric {
    label: String,
    current: usize,
    previous: usize,
    delta: String,
    tone: String,
    href: String,
}

fn comparison_delta(current: usize, previous: usize) -> (String, String) {
    if previous == 0 && current == 0 {
        ("No change".to_string(), "neutral".to_string())
    } else if previous == 0 {
        ("New activity".to_string(), "warning".to_string())
    } else {
        let delta = ((current as f64 - previous as f64) / previous as f64 * 100.0).round() as i64;
        (
            format!("{delta:+}%"),
            if delta > 0 {
                "warning"
            } else if delta < 0 {
                "good"
            } else {
                "neutral"
            }
            .to_string(),
        )
    }
}

fn report_comparison_metrics(
    signal_count: usize,
    previous_signal_count: Option<usize>,
    record_count: usize,
    previous_record_count: Option<usize>,
    from: &str,
    to: &str,
) -> Vec<ReportComparisonMetric> {
    let Some(previous_signal_count) = previous_signal_count else { return vec![] };
    let previous_record_count = previous_record_count.unwrap_or(0);
    let (signal_delta, signal_tone) = comparison_delta(signal_count, previous_signal_count);
    let (record_delta, record_tone) = comparison_delta(record_count, previous_record_count);
    vec![
        ReportComparisonMetric {
            label: "Signals".into(),
            current: signal_count,
            previous: previous_signal_count,
            delta: signal_delta,
            tone: signal_tone,
            href: format!("/events?from={from}&to={to}"),
        },
        ReportComparisonMetric {
            label: "Source records".into(),
            current: record_count,
            previous: previous_record_count,
            delta: record_delta,
            tone: record_tone,
            href: format!("/data?from={from}&to={to}"),
        },
    ]
}

fn report_operating_funnel(
    record_count: usize,
    signal_count: usize,
    open_incidents: usize,
    ontology_objects: usize,
    response_count: usize,
    from: &str,
    to: &str,
) -> Vec<ReportFunnelStage> {
    let window = |path: &str| {
        serde_urlencoded::to_string([("from", from), ("to", to)])
            .map(|query| format!("{path}{}{query}", if path.contains('?') { '&' } else { '?' }))
            .unwrap_or_else(|_| path.to_string())
    };
    let stages = [
        ("Evidence", record_count, "source records in the selected window", window("/data")),
        (
            "Signals",
            signal_count,
            "generated events available for investigation",
            window("/events"),
        ),
        (
            "Cases",
            open_incidents,
            "active investigations requiring ownership",
            window("/incidents?status=active"),
        ),
        (
            "Model",
            ontology_objects,
            "modeled entities with evidence in the selected window",
            "/ontology".to_string(),
        ),
        (
            "Response",
            response_count,
            "governed decisions recorded in the ledger",
            window("/actions"),
        ),
    ];
    let max_count = stages.iter().map(|(_, count, _, _)| *count).max().unwrap_or(0).max(1);
    stages
        .into_iter()
        .map(|(label, count, detail, href)| ReportFunnelStage {
            label: label.to_string(),
            count,
            percent: if count == 0 { 0 } else { (count * 100 / max_count).max(4) },
            detail: detail.to_string(),
            href,
            tone: if count == 0 { "risk".to_string() } else { "good".to_string() },
        })
        .collect()
}

fn report_readiness_metrics(
    signal_count: usize,
    open_incidents: usize,
    sla_breached_incidents: usize,
    ontology_objects: usize,
    completed_actions: usize,
    review_actions: usize,
    from: &str,
    to: &str,
) -> Vec<ReportReadinessMetric> {
    let window = |path: &str| {
        serde_urlencoded::to_string([("from", from), ("to", to)])
            .map(|query| format!("{path}?{query}"))
            .unwrap_or_else(|_| path.to_string())
    };
    let response_total = completed_actions + review_actions;
    vec![
        ReportReadinessMetric {
            label: "Signal evidence".into(),
            count: signal_count,
            percent: if signal_count > 0 { 100 } else { 0 },
            detail: if signal_count > 0 {
                "events available for review"
            } else {
                "no events in this window"
            }
            .into(),
            href: window("/events"),
            tone: if signal_count > 0 { "good" } else { "risk" }.into(),
        },
        ReportReadinessMetric {
            label: "Case control".into(),
            count: open_incidents + sla_breached_incidents,
            percent: 100usize.saturating_sub(
                (open_incidents + sla_breached_incidents).saturating_mul(20).min(100),
            ),
            detail: format!("{open_incidents} open · {sla_breached_incidents} past target"),
            href: window("/incidents?status=active"),
            tone: if sla_breached_incidents > 0 {
                "risk"
            } else if open_incidents > 0 {
                "warning"
            } else {
                "good"
            }
            .into(),
        },
        ReportReadinessMetric {
            label: "Model coverage".into(),
            count: ontology_objects,
            percent: if ontology_objects > 0 { 100 } else { 0 },
            detail: if ontology_objects > 0 {
                "entities with evidence in this window"
            } else {
                "no modeled entities available"
            }
            .into(),
            href: "/ontology".into(),
            tone: if ontology_objects > 0 { "good" } else { "risk" }.into(),
        },
        ReportReadinessMetric {
            label: "Response assurance".into(),
            count: review_actions,
            percent: if response_total == 0 { 0 } else { completed_actions * 100 / response_total },
            detail: format!("{completed_actions} completed · {review_actions} need review"),
            href: "/actions?outcome=review".into(),
            tone: if review_actions > 0 {
                "warning"
            } else if response_total > 0 {
                "good"
            } else {
                "neutral"
            }
            .into(),
        },
    ]
}

async fn list_saved_report_views(state: &AppState, tenant_id: uuid::Uuid) -> Vec<SavedReportView> {
    state
        .saved_search_queries_client
        .list(tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|query| {
            query.filter.get("view_kind").and_then(serde_json::Value::as_str) == Some("reports")
        })
        .map(|query| {
            let filter: SavedReportFilter =
                serde_json::from_value(query.filter).unwrap_or_default();
            let query_string = serde_urlencoded::to_string(&filter).unwrap_or_default();
            SavedReportView {
                id: query.id,
                name: query.name,
                load_url: format!("/reports?{query_string}"),
            }
        })
        .collect()
}

fn count_by_event_type(events: &[crate::EventSummary]) -> Vec<EventTypeCount> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for event in events {
        *counts.entry(event.event_type.clone()).or_insert(0) += 1;
    }
    counts.into_iter().map(|(event_type, count)| EventTypeCount { event_type, count }).collect()
}

fn report_incident_sla_breached(incident: &common::Incident, now: chrono::DateTime<Utc>) -> bool {
    let target = match incident.severity {
        common::IncidentSeverity::Critical => chrono::Duration::minutes(15),
        common::IncidentSeverity::High => chrono::Duration::hours(1),
        common::IncidentSeverity::Medium => chrono::Duration::hours(4),
        common::IncidentSeverity::Low => chrono::Duration::hours(24),
    };
    incident.resolved_at.unwrap_or(now).signed_duration_since(incident.created_at) > target
}

pub async fn get_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let saved_views = list_saved_report_views(&state, session.tenant_id).await;

    let mut connector_stats = match state.stats_client.connector_stats(session.tenant_id).await {
        Ok(stats) => stats,
        Err(e) => {
            return Html(
                ReportsTemplate {
                    show_nav: true,
                    is_admin,
                    connector_stats: vec![],
                    connector_stats_chart_json: chart_json(&[], &[]),
                    event_counts: vec![],
                    event_counts_chart_json: chart_json(&[], &[]),
                    signal_trend_chart_json: chart_json(&[], &[]),
                    incident_posture_chart_json: chart_json(&[], &[]),
                    open_incidents: 0,
                    critical_incidents: 0,
                    acknowledged_incidents: 0,
                    sla_breached_incidents: 0,
                    ontology_types: 0,
                    ontology_objects: 0,
                    ontology_coverage: vec![],
                    completed_actions: 0,
                    review_actions: 0,
                    action_rows: vec![],
                    signal_count: 0,
                    signal_delta: "Select a signal window for comparison".to_string(),
                    incident_rows: vec![],
                    incident_matrix: vec![],
                    saved_views,
                    from: query.from,
                    to: query.to,
                    notice: query.notice,
                    error: Some(e.to_string()),
                    readiness_metrics: vec![],
                    operating_funnel: vec![],
                    comparison_metrics: vec![],
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };
    let mut window_record_ids = std::collections::HashSet::new();

    let mut error: Option<String> = None;
    let (since, until) = parse_date_range(&query.from, &query.to);
    if since.is_some() || until.is_some() {
        match connector_stats_for_window(&state, session.tenant_id, since, until).await {
            Ok((stats, record_ids)) => {
                connector_stats = stats;
                window_record_ids = record_ids;
            }
            Err(message) => error = Some(format!("connector window: {message}")),
        }
    }
    let window_events =
        match state.events_client.list_events(&session.bearer_token, 1000, 0, since, until).await {
            Ok(page) => page.events,
            Err(e) => {
                error = Some(format!("events: {e}"));
                vec![]
            }
        };
    let event_counts = count_by_event_type(&window_events);
    let event_labels = window_events
        .iter()
        .map(|event| (event.id, event.event_type.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let connector_labels =
        connector_stats.iter().map(|s| s.connector_id.clone()).collect::<Vec<_>>();
    let connector_values = connector_stats.iter().map(|s| s.record_count).collect::<Vec<_>>();
    let connector_hrefs = connector_stats
        .iter()
        .map(|stat| {
            serde_urlencoded::to_string([
                ("connector_id", stat.connector_id.as_str()),
                ("from", query.from.as_str()),
                ("to", query.to.as_str()),
            ])
            .map(|params| format!("/data?{params}"))
            .unwrap_or_else(|_| "/data".to_string())
        })
        .collect::<Vec<_>>();
    let connector_stats_chart_json =
        chart_json_with_hrefs(&connector_labels, &connector_values, &connector_hrefs);
    let signal_count = event_counts.iter().map(|row| row.count).sum::<usize>();
    let record_count = connector_stats.iter().map(|stat| stat.record_count.max(0) as usize).sum();
    let (previous_since, previous_until) = previous_signal_window(since, until);
    let mut previous_signal_count = None;
    let mut previous_record_count = None;
    let signal_delta = match (previous_since, previous_until) {
        (Some(previous_since), Some(previous_until)) => {
            match state
                .events_client
                .list_events(
                    &session.bearer_token,
                    1000,
                    0,
                    Some(previous_since),
                    Some(previous_until),
                )
                .await
            {
                Ok(page) => {
                    let previous_count = page.events.len();
                    previous_signal_count = Some(previous_count);
                    if let Ok((previous_stats, _)) = connector_stats_for_window(
                        &state,
                        session.tenant_id,
                        Some(previous_since),
                        Some(previous_until),
                    )
                    .await
                    {
                        previous_record_count = Some(
                            previous_stats
                                .iter()
                                .map(|stat| stat.record_count.max(0) as usize)
                                .sum(),
                        );
                    }
                    if previous_count == 0 && signal_count == 0 {
                        "No change vs prior window".to_string()
                    } else if previous_count == 0 {
                        "New activity vs prior window".to_string()
                    } else {
                        let delta = ((signal_count as f64 - previous_count as f64)
                            / previous_count as f64
                            * 100.0)
                            .round() as i64;
                        format!("{delta:+}% vs prior window")
                    }
                }
                Err(e) => {
                    if error.is_none() {
                        error = Some(format!("previous signal window: {e}"));
                    }
                    "Prior window unavailable".to_string()
                }
            }
        }
        _ => "Select a signal window for comparison".to_string(),
    };
    let event_chart_labels = event_counts.iter().map(|e| e.event_type.clone()).collect::<Vec<_>>();
    let event_values = event_counts.iter().map(|e| e.count as i64).collect::<Vec<_>>();
    let event_hrefs = event_counts
        .iter()
        .map(|event| {
            serde_urlencoded::to_string([
                ("q", event.event_type.as_str()),
                ("from", query.from.as_str()),
                ("to", query.to.as_str()),
            ])
            .map(|params| format!("/events?{params}"))
            .unwrap_or_else(|_| "/events".to_string())
        })
        .collect::<Vec<_>>();
    let event_counts_chart_json =
        chart_json_with_hrefs(&event_chart_labels, &event_values, &event_hrefs);
    let signal_trend_chart_json = signal_trend_chart_json(&window_events);

    let all_incidents = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(incidents) => incidents,
        Err(e) => {
            if error.is_none() {
                error = Some(format!("incidents: {e}"));
            }
            vec![]
        }
    };
    let window_event_ids =
        window_events.iter().map(|event| event.id).collect::<std::collections::HashSet<_>>();
    let incidents = all_incidents
        .into_iter()
        .filter(|incident| incident_in_window(incident, &window_event_ids, since, until))
        .collect::<Vec<_>>();
    let open_incidents = incidents
        .iter()
        .filter(|incident| incident.incident.status != common::IncidentStatus::Resolved)
        .count();
    let critical_incidents = incidents
        .iter()
        .filter(|incident| {
            incident.incident.status != common::IncidentStatus::Resolved
                && incident.incident.severity == common::IncidentSeverity::Critical
        })
        .count();
    let acknowledged_incidents = incidents
        .iter()
        .filter(|incident| incident.incident.status == common::IncidentStatus::Acknowledged)
        .count();
    let sla_breached_incidents = incidents
        .iter()
        .filter(|incident| report_incident_sla_breached(&incident.incident, Utc::now()))
        .count();
    let mut prioritized_incidents = incidents.iter().collect::<Vec<_>>();
    prioritized_incidents.sort_by(|left, right| {
        let rank = |severity: common::IncidentSeverity| match severity {
            common::IncidentSeverity::Critical => 4,
            common::IncidentSeverity::High => 3,
            common::IncidentSeverity::Medium => 2,
            common::IncidentSeverity::Low => 1,
        };
        (left.incident.status == common::IncidentStatus::Resolved)
            .cmp(&(right.incident.status == common::IncidentStatus::Resolved))
            .then_with(|| rank(right.incident.severity).cmp(&rank(left.incident.severity)))
            .then_with(|| right.incident.updated_at.cmp(&left.incident.updated_at))
    });
    let incident_rows = prioritized_incidents
        .iter()
        .take(8)
        .map(|incident| {
            let event_links = incident
                .event_ids
                .iter()
                .filter_map(|id| {
                    event_labels.get(id).map(|event_type| IncidentReportEvent {
                        id: *id,
                        event_type: event_type.clone(),
                    })
                })
                .collect::<Vec<_>>();
            IncidentReportRow {
                id: incident.incident.id,
                title: incident.incident.title.clone(),
                severity: incident.incident.severity.to_string(),
                status: incident.incident.status.to_string(),
                event_count: event_links.len(),
                event_links,
            }
        })
        .collect();
    let incident_matrix = incident_posture_matrix(&incidents);
    let incident_posture_chart_json = incident_posture_chart_json(&incident_matrix);
    let (
        ontology_types,
        ontology_objects,
        ontology_coverage,
        completed_actions,
        review_actions,
        action_rows,
    ) = match ontology_client::global() {
        Some(client) => {
            let types = client.list_object_types(&session.bearer_token).await;
            let objects = client.list_objects(&session.bearer_token, None).await;
            let links = client.list_links(&session.bearer_token).await;
            let action_types = client.list_action_types(&session.bearer_token).await;
            let actions = client.list_action_invocations(&session.bearer_token).await;
            if let Err(e) = &types {
                if error.is_none() {
                    error = Some(format!("ontology types: {e}"));
                }
            }
            if let Err(e) = &objects {
                if error.is_none() {
                    error = Some(format!("ontology objects: {e}"));
                }
            }
            if let Err(e) = &links {
                if error.is_none() {
                    error = Some(format!("ontology links: {e}"));
                }
            }
            if let Err(e) = &action_types {
                if error.is_none() {
                    error = Some(format!("action definitions: {e}"));
                }
            }
            if let Err(e) = &actions {
                if error.is_none() {
                    error = Some(format!("action history: {e}"));
                }
            }
            let action_names = action_types
                .unwrap_or_default()
                .into_iter()
                .map(|action| (action.id, action.name))
                .collect::<std::collections::HashMap<_, _>>();
            let object_titles = objects
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .map(|object| {
                            let title = object
                                .properties
                                .get("name")
                                .or_else(|| object.properties.get("title"))
                                .or_else(|| object.properties.get("subject"))
                                .or_else(|| object.properties.get("id"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Untitled entity")
                                .to_string();
                            (object.id, title)
                        })
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .unwrap_or_default();
            let scoped_objects = objects
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .filter(|object| object_in_window(object, &window_record_ids, since, until))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let object_counts = scoped_objects.iter().fold(
                std::collections::HashMap::<uuid::Uuid, usize>::new(),
                |mut counts, object| {
                    *counts.entry(object.object_type_id).or_default() += 1;
                    counts
                },
            );
            /*
             * Object titles remain available for action targets across the workspace, while
             * coverage counts and the model stage are scoped to the selected evidence window.
             */
            let object_type_by_id = objects
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .map(|object| (object.id, object.object_type_id))
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .unwrap_or_default();
            let ontology_coverage = types
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .map(|object_type| OntologyCoverageRow {
                            id: object_type.id,
                            name: object_type.name.clone(),
                            object_count: object_counts.get(&object_type.id).copied().unwrap_or(0),
                            property_count: object_type
                                .property_schema
                                .as_object()
                                .map(|schema| schema.len())
                                .unwrap_or(0),
                            relationship_count: links
                                .as_ref()
                                .map(|items| {
                                    items
                                        .iter()
                                        .filter(|link| {
                                            object_type_by_id.get(&link.source_object_id)
                                                == Some(&object_type.id)
                                                || object_type_by_id.get(&link.target_object_id)
                                                    == Some(&object_type.id)
                                        })
                                        .count()
                                })
                                .unwrap_or(0),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut filtered_actions = actions.unwrap_or_default();
            filtered_actions.retain(|item| {
                since.map(|value| item.executed_at >= value).unwrap_or(true)
                    && until.map(|value| item.executed_at <= value).unwrap_or(true)
            });
            let completed = filtered_actions
                .iter()
                .filter(|item| item.outcome.eq_ignore_ascii_case("completed"))
                .count();
            let review = filtered_actions.len().saturating_sub(completed);
            let mut sorted_actions = filtered_actions;
            sorted_actions.sort_by(|left, right| right.executed_at.cmp(&left.executed_at));
            let action_rows = sorted_actions
                .into_iter()
                .take(12)
                .map(|item| {
                    let target_count =
                        item.target_object_ids.as_array().map(|values| values.len()).unwrap_or(0);
                    let targets = item
                        .target_object_ids
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|value| value.as_str())
                        .filter_map(|value| uuid::Uuid::parse_str(value).ok())
                        .map(|id| ActionReportTarget {
                            id,
                            label: object_titles
                                .get(&id)
                                .cloned()
                                .unwrap_or_else(|| id.to_string()),
                        })
                        .collect();
                    let event_id = item
                        .triggering_event_ref
                        .get("event_id")
                        .or_else(|| item.triggering_event_ref.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| uuid::Uuid::parse_str(value).ok());
                    let incident_id = item
                        .triggering_event_ref
                        .get("incident_id")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| uuid::Uuid::parse_str(value).ok());
                    ActionReportRow {
                        id: item.id,
                        action_name: action_names
                            .get(&item.action_type_id)
                            .cloned()
                            .unwrap_or_else(|| "Unknown action".to_string()),
                        outcome: item.outcome,
                        target_count,
                        executed_at: item.executed_at,
                        event_id,
                        incident_id,
                        targets,
                    }
                })
                .collect();
            (
                types.map(|v| v.len()).unwrap_or(0),
                objects
                    .as_ref()
                    .map(|items| {
                        items
                            .iter()
                            .filter(|object| {
                                object_in_window(object, &window_record_ids, since, until)
                            })
                            .count()
                    })
                    .unwrap_or(0),
                ontology_coverage,
                completed,
                review,
                action_rows,
            )
        }
        None => (0, 0, vec![], 0, 0, vec![]),
    };

    Html(
        ReportsTemplate {
            show_nav: true,
            is_admin,
            connector_stats,
            connector_stats_chart_json,
            event_counts,
            event_counts_chart_json,
            signal_trend_chart_json,
            incident_posture_chart_json,
            open_incidents,
            critical_incidents,
            acknowledged_incidents,
            sla_breached_incidents,
            ontology_types,
            ontology_objects,
            ontology_coverage,
            completed_actions,
            review_actions,
            action_rows,
            signal_count,
            signal_delta,
            incident_rows,
            incident_matrix,
            saved_views,
            from: query.from.clone(),
            to: query.to.clone(),
            notice: query.notice,
            error,
            readiness_metrics: report_readiness_metrics(
                signal_count,
                open_incidents,
                sla_breached_incidents,
                ontology_objects,
                completed_actions,
                review_actions,
                &query.from,
                &query.to,
            ),
            operating_funnel: report_operating_funnel(
                record_count,
                signal_count,
                open_incidents,
                ontology_objects,
                completed_actions + review_actions,
                &query.from,
                &query.to,
            ),
            comparison_metrics: report_comparison_metrics(
                signal_count,
                previous_signal_count,
                record_count,
                previous_record_count,
                &query.from,
                &query.to,
            ),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveReportViewForm {
    name: String,
    from: String,
    to: String,
}

pub async fn post_save_report_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveReportViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let filter = serde_json::json!({ "view_kind": "reports", "from": form.from, "to": form.to });
    let window =
        serde_urlencoded::to_string([("from", form.from.as_str()), ("to", form.to.as_str())])
            .unwrap_or_default();
    match state.saved_search_queries_client.create(session.tenant_id, &form.name, filter).await {
        Ok(_) => axum::response::Redirect::to(&format!("/reports?{window}&notice=view_saved"))
            .into_response(),
        Err(_) => axum::response::Redirect::to(&format!("/reports?{window}&notice=view_failed"))
            .into_response(),
    }
}

pub async fn post_delete_report_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(()) => axum::response::Redirect::to("/reports").into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

/// GET /reports/export.csv — preserves the report's selected signal window in a portable
/// handoff. The export contains signal counts, incident posture, and governed response evidence.
pub async fn get_reports_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let (since, until) = parse_date_range(&query.from, &query.to);
    let events =
        match state.events_client.list_events(&session.bearer_token, 1000, 0, since, until).await {
            Ok(page) => page.events,
            Err(error) => {
                return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
            }
        };
    let counts = count_by_event_type(&events);
    let incidents = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(items) => items,
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    };
    let mut csv = String::from(
        "section,signal_window,event_type,count,incident_title,severity,status,linked_events,action_name,outcome,target_count,executed_at,source_event,source_incident\n",
    );
    let window = format!("{}..{}", query.from, query.to);
    csv.push_str(&format!(
        "summary,{},signals in window,{},,,,,,,,,,\n",
        csv_escape(&window),
        events.len()
    ));
    for row in counts {
        csv.push_str(&format!(
            "signals,{},{},{},,,,,,,,,,\n",
            csv_escape(&window),
            csv_escape(&row.event_type),
            row.count
        ));
    }
    for incident in incidents {
        csv.push_str(&format!(
            "incidents,{},{},,{}, {},{},{},,,,,,\n",
            csv_escape(&window),
            "",
            csv_escape(&incident.incident.title),
            incident.incident.severity,
            incident.incident.status,
            incident.event_ids.len()
        ));
    }
    if let Some(client) = ontology_client::global() {
        let action_types =
            client.list_action_types(&session.bearer_token).await.unwrap_or_default();
        let action_names = action_types
            .into_iter()
            .map(|action| (action.id, action.name))
            .collect::<std::collections::HashMap<_, _>>();
        let mut actions =
            client.list_action_invocations(&session.bearer_token).await.unwrap_or_default();
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
                csv_escape(&window),
                csv_escape(
                    action_names
                        .get(&action.action_type_id)
                        .map(String::as_str)
                        .unwrap_or("Unknown action")
                ),
                csv_escape(&action.outcome),
                target_count,
                csv_escape(&action.executed_at.to_rfc3339()),
                csv_escape(event_id),
                csv_escape(incident_id),
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

fn pdf_text_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect()
}

/// Builds a deliberately small, dependency-free PDF using the PDF 1.4 text model. The report
/// is a compact executive summary; the CSV remains the lossless export. Keeping this writer
/// here avoids pretending that a renamed HTML/CSV response is a PDF artifact.
fn build_report_pdf(lines: &[String]) -> Vec<u8> {
    let mut content = String::from("BT\n/F1 11 Tf\n50 760 Td\n");
    for (index, line) in lines.iter().take(48).enumerate() {
        if index > 0 {
            content.push_str("0 -15 Td\n");
        }
        content.push_str(&format!("({}) Tj\n", pdf_text_escape(line)));
    }
    content.push_str("ET\n");
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
    ];
    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );
    pdf
}

/// GET /reports/export.pdf — a real, authenticated PDF summary for executive handoff.
pub async fn get_reports_export_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let (since, until) = parse_date_range(&query.from, &query.to);
    let events =
        match state.events_client.list_events(&session.bearer_token, 1000, 0, since, until).await {
            Ok(page) => page.events,
            Err(error) => {
                return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
            }
        };
    let incidents = match state.incidents_client.list_incidents(session.tenant_id, None).await {
        Ok(items) => items,
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    };
    let counts = count_by_event_type(&events);
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
    (response_headers, build_report_pdf(&lines)).into_response()
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
