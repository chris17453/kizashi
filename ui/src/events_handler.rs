#[path = "events_handler_filtering_test.rs"]
#[cfg(test)]
mod events_handler_filtering_test;
#[path = "events_handler_test.rs"]
#[cfg(test)]
mod events_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, EventSummary};
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use common::SavedSearchQuery;

fn default_page() -> i64 {
    0
}

/// Parses `YYYY-MM-DD` date-only strings from `<input type="date">` into a fully inclusive
/// `DateTime<Utc>` range -- same shape as `data_handler`'s `parse_date_range`, duplicated here
/// rather than shared since it's a small, page-local concern (matching this codebase's existing
/// convention of per-handler `matches_query`/`sort_rows` helpers rather than a shared crate-wide
/// filter-parsing module).
fn parse_date_range(from: &str, to: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let parse_start = |s: &str| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    };
    let parse_end = |s: &str| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(23, 59, 59))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    };
    (parse_start(from), parse_end(to))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub dir: String,
    /// Scopes the search to a specific incident window -- forwarded to the backend, which
    /// already supported `since`/`until` (unlike `q`/`sort`, which only apply to the current
    /// fetched page since the backend has no substring-match/sort query of its own).
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub status: String,
    /// Case linkage posture: empty/any, `unlinked`, or `linked`. Applied after the event feed
    /// is joined with tenant incident membership so triage can isolate signals that still need
    /// an investigation without changing the event-service contract.
    #[serde(default)]
    pub case_scope: String,
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub created_incident: Option<uuid::Uuid>,
    #[serde(default)]
    pub skipped_count: usize,
    #[serde(default)]
    pub updated_count: usize,
    #[serde(default)]
    pub failed_count: usize,
    #[serde(default)]
    pub linked_count_result: usize,
    #[serde(default)]
    pub link_failed_count: usize,
    #[serde(default)]
    pub linked_incident: Option<uuid::Uuid>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default, Clone)]
struct SavedEventsFilter {
    #[serde(default)]
    q: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    case_scope: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
}

struct SavedEventView {
    id: uuid::Uuid,
    name: String,
    load_url: String,
}

fn to_saved_event_view(query: SavedSearchQuery) -> SavedEventView {
    let filter: SavedEventsFilter = serde_json::from_value(query.filter).unwrap_or_default();
    let load_url = format!("/events?{}", serde_urlencoded::to_string(&filter).unwrap_or_default());
    SavedEventView { id: query.id, name: query.name, load_url }
}

/// Case-insensitive substring match across event_type/group_key/status -- same shape as the
/// other list-page searches (ADR-0062). Like Triggers (ADR-0066), `list_events` is
/// server-paginated, so this only filters the *current page's* already-fetched events, not the
/// tenant's full event history.
fn matches_query(event: &EventSummary, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    event.event_type.to_lowercase().contains(&q)
        || event.group_key.to_lowercase().contains(&q)
        || event.status.to_lowercase().contains(&q)
}

/// Same shape as Triggers' sortable columns (ADR-0070), applied after the search filter and,
/// like search, only reordering the current page. `list_events` already returns most-recent
/// first, so an unset `sort` keeps that existing default.
fn sort_rows(rows: &mut [EventSummary], sort: &str, dir: &str) {
    match sort {
        "event_type" => rows.sort_by_key(|e| e.event_type.to_lowercase()),
        "group_key" => rows.sort_by_key(|e| e.group_key.to_lowercase()),
        "status" => rows.sort_by_key(|e| e.status.to_lowercase()),
        _ => {
            rows.sort_by_key(|e| std::cmp::Reverse(e.occurred_at));
            if dir == "asc" {
                rows.reverse();
            }
            return;
        }
    }
    if dir == "desc" {
        rows.reverse();
    }
}

struct HeatmapCell {
    date: String,
    count: usize,
    opacity: String,
    href: String,
}

fn scoped_event_url(query: &EventsQuery, extras: &[(&str, String)]) -> String {
    let mut params = Vec::new();
    for (key, value) in [
        ("q", query.q.clone()),
        ("status", query.status.clone()),
        ("case_scope", query.case_scope.clone()),
        ("from", query.from.clone()),
        ("to", query.to.clone()),
        ("sort", query.sort.clone()),
        ("dir", query.dir.clone()),
    ] {
        if !value.is_empty() && !extras.iter().any(|(extra_key, _)| *extra_key == key) {
            params.push((key, value));
        }
    }
    params.extend(extras.iter().map(|(key, value)| (*key, value.clone())));
    format!("/events?{}", serde_urlencoded::to_string(params).unwrap_or_default())
}

/// Server-rendered fallback for the interactive trend. The browser upgrades the same scope to
/// the shared line/area renderer, while no-JS clients still receive a useful SVG.
struct ChartBar {
    date: String,
    count: u64,
    height_pct: u32,
    href: String,
}

struct EventHeatmapRow {
    event_type: String,
    total: usize,
    cells: Vec<HeatmapCell>,
}

fn build_event_heatmap(
    events: &[EventSummary],
    query: &EventsQuery,
) -> (Vec<String>, Vec<EventHeatmapRow>) {
    let mut dates = std::collections::BTreeSet::new();
    let mut counts = std::collections::HashMap::<(String, String), usize>::new();
    let mut totals = std::collections::HashMap::<String, usize>::new();
    for event in events {
        let date = event.occurred_at.format("%Y-%m-%d").to_string();
        dates.insert(date.clone());
        *counts.entry((event.event_type.clone(), date)).or_default() += 1;
        *totals.entry(event.event_type.clone()).or_default() += 1;
    }
    let dates = dates.into_iter().collect::<Vec<_>>();
    let mut types = totals.into_iter().collect::<Vec<_>>();
    types.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    types.truncate(8);
    let max = types
        .iter()
        .flat_map(|(event_type, _)| {
            dates
                .iter()
                .map(|date| counts.get(&(event_type.clone(), date.clone())).copied().unwrap_or(0))
        })
        .max()
        .unwrap_or(1);
    let rows = types
        .into_iter()
        .map(|(event_type, total)| EventHeatmapRow {
            cells: dates
                .iter()
                .map(|date| {
                    let count =
                        counts.get(&(event_type.clone(), date.clone())).copied().unwrap_or(0);
                    let intensity =
                        if count == 0 { 0 } else { ((count * 100) / max).max(8) as u32 };
                    let opacity = if intensity >= 100 {
                        "1.0".to_string()
                    } else {
                        format!("0.{:02}", intensity.max(8))
                    };
                    HeatmapCell {
                        href: scoped_event_url(
                            query,
                            &[
                                ("q", event_type.clone()),
                                ("from", date.clone()),
                                ("to", date.clone()),
                            ],
                        ),
                        date: date.clone(),
                        count,
                        opacity,
                    }
                })
                .collect(),
            event_type,
            total,
        })
        .collect();
    (dates, rows)
}

struct IncidentBadge {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
}

struct IncidentOption {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
    selected: bool,
}

struct EventRow {
    event: EventSummary,
    incident: Option<IncidentBadge>,
}

struct PotentialCluster {
    group_key: String,
    total_count: usize,
    unlinked_count: usize,
    event_types: Vec<String>,
    unlinked_event_ids: Vec<uuid::Uuid>,
}

struct EventPostureMetric {
    label: String,
    count: usize,
    percent: i32,
    href: String,
    tone: String,
}

fn matches_case_scope(row: &EventRow, case_scope: &str) -> bool {
    match case_scope {
        "linked" => row.incident.is_some(),
        "unlinked" => row.incident.is_none(),
        _ => true,
    }
}

fn build_potential_clusters(events: &[EventRow]) -> Vec<PotentialCluster> {
    let mut grouped: std::collections::BTreeMap<String, PotentialCluster> =
        std::collections::BTreeMap::new();
    for row in events {
        let cluster =
            grouped.entry(row.event.group_key.clone()).or_insert_with(|| PotentialCluster {
                group_key: row.event.group_key.clone(),
                total_count: 0,
                unlinked_count: 0,
                event_types: Vec::new(),
                unlinked_event_ids: Vec::new(),
            });
        cluster.total_count += 1;
        if !cluster.event_types.iter().any(|event_type| event_type == &row.event.event_type) {
            cluster.event_types.push(row.event.event_type.clone());
        }
        if row.incident.is_none() {
            cluster.unlinked_count += 1;
            cluster.unlinked_event_ids.push(row.event.id);
        }
    }
    let mut clusters = grouped
        .into_values()
        .filter(|cluster| cluster.total_count > 1 && cluster.unlinked_count > 0)
        .collect::<Vec<_>>();
    clusters.sort_by(|left, right| {
        right
            .unlinked_count
            .cmp(&left.unlinked_count)
            .then_with(|| left.group_key.cmp(&right.group_key))
    });
    clusters
}

fn event_trend_chart_json(events: &[EventSummary], query: &EventsQuery) -> String {
    let mut counts = std::collections::BTreeMap::<String, i64>::new();
    for event in events {
        *counts.entry(event.occurred_at.format("%Y-%m-%d").to_string()).or_default() += 1;
    }
    let (labels, values): (Vec<_>, Vec<_>) = counts.into_iter().unzip();
    let hrefs = labels
        .iter()
        .map(|date| scoped_event_url(query, &[("from", date.clone()), ("to", date.clone())]))
        .collect::<Vec<_>>();
    #[derive(serde::Serialize)]
    struct ChartData<'a> {
        labels: &'a [String],
        values: &'a [i64],
        hrefs: &'a [String],
    }
    serde_json::to_string(&ChartData { labels: &labels, values: &values, hrefs: &hrefs })
        .unwrap_or_default()
        .replace('<', "\\u003c")
}

fn event_trend_chart_json_from_bars(bars: &[ChartBar]) -> String {
    let labels = bars.iter().map(|bar| bar.date.clone()).collect::<Vec<_>>();
    let values = bars.iter().map(|bar| bar.count as i64).collect::<Vec<_>>();
    let hrefs = bars.iter().map(|bar| bar.href.clone()).collect::<Vec<_>>();
    #[derive(serde::Serialize)]
    struct ChartData<'a> {
        labels: &'a [String],
        values: &'a [i64],
        hrefs: &'a [String],
    }
    serde_json::to_string(&ChartData { labels: &labels, values: &values, hrefs: &hrefs })
        .unwrap_or_default()
        .replace('<', "\\u003c")
}

fn build_chart_bars(
    counts: Vec<crate::events_client::DailyCount>,
    query: &EventsQuery,
) -> Vec<ChartBar> {
    let max = counts.iter().map(|c| c.count).max().unwrap_or(0).max(1);
    counts
        .into_iter()
        .map(|c| {
            let date = c.date;
            ChartBar {
                href: scoped_event_url(query, &[("from", date.clone()), ("to", date.clone())]),
                date,
                count: c.count,
                height_pct: ((c.count as f64 / max as f64) * 100.0).round() as u32,
            }
        })
        .collect()
}

#[derive(Template)]
#[template(path = "events.html")]
struct EventsTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    events: Vec<EventRow>,
    chart_bars: Vec<ChartBar>,
    event_trend_chart_json: String,
    event_trend_has_data: bool,
    page: i64,
    has_more: bool,
    error: Option<String>,
    q: String,
    sort: String,
    dir: String,
    from: String,
    to: String,
    status: String,
    case_scope: String,
    visible_count: usize,
    linked_count: usize,
    unlinked_count: usize,
    actioned_count: usize,
    saved_views: Vec<SavedEventView>,
    notice: String,
    created_incident: Option<uuid::Uuid>,
    skipped_count: usize,
    updated_count: usize,
    failed_count: usize,
    incident_options: Vec<IncidentOption>,
    linked_count_result: usize,
    link_failed_count: usize,
    linked_incident: Option<uuid::Uuid>,
    potential_clusters: Vec<PotentialCluster>,
    status_metrics: Vec<EventPostureMetric>,
    type_metrics: Vec<EventPostureMetric>,
    heatmap_dates: Vec<String>,
    heatmap_rows: Vec<EventHeatmapRow>,
}

/// POST /events/bulk-status — applies one explicit lifecycle disposition to selected signals.
/// Each update remains tenant-scoped by the bearer token at the EventsClient boundary; failures
/// are reported without hiding successful updates so an operator can reconcile the queue.
pub async fn post_bulk_event_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "operator access required").into_response();
    }
    // Repeated checkbox names arrive as repeated key/value pairs. Parse the raw form so a
    // selection of one or many IDs has the same behavior as the existing event→incident bulk
    // workflow, instead of asking serde's struct decoder to collapse a sequence into one field.
    let fields = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body).unwrap_or_default();
    let values = |key: &str| {
        fields
            .iter()
            .filter(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>()
    };
    let ids = values("ids")
        .into_iter()
        .filter_map(|value| value.parse::<uuid::Uuid>().ok())
        .collect::<Vec<_>>();
    let value = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let redirect_with_scope = |notice: &str, updated_count: usize, failed_count: usize| {
        let query = serde_urlencoded::to_string([
            ("q", value("q")),
            ("status", value("status")),
            ("case_scope", value("case_scope")),
            ("from", value("from")),
            ("to", value("to")),
            ("sort", value("sort")),
            ("dir", value("dir")),
            ("notice", notice.to_string()),
            ("updated_count", updated_count.to_string()),
            ("failed_count", failed_count.to_string()),
        ])
        .unwrap_or_else(|_| format!("notice={notice}"));
        Redirect::to(&format!("/events?{query}"))
    };
    let target_status = value("target_status").trim().to_ascii_lowercase();
    if !matches!(target_status.as_str(), "new" | "triggered" | "actioned" | "dismissed") {
        return redirect_with_scope("invalid-status", 0, 0).into_response();
    }
    if ids.is_empty() {
        return redirect_with_scope("bulk-status-empty", 0, 0).into_response();
    }
    let mut updated_count = 0usize;
    let mut failed_count = 0usize;
    for id in ids {
        match state
            .events_client
            .update_event_status(&session.bearer_token, id, &target_status, &session.username)
            .await
        {
            Ok(()) => updated_count += 1,
            Err(_) => failed_count += 1,
        }
    }
    redirect_with_scope("bulk-status-updated", updated_count, failed_count).into_response()
}

pub async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let page = query.page.max(0);
    let offset = (page * DEFAULT_PAGE_SIZE) as u32;
    let (filter_since, filter_until) = parse_date_range(&query.from, &query.to);

    let has_active_scope = !query.q.is_empty()
        || !query.status.is_empty()
        || !query.from.is_empty()
        || !query.to.is_empty()
        || !query.case_scope.is_empty();
    let fallback_chart_bars = if has_active_scope {
        vec![]
    } else {
        let until = chrono::Utc::now();
        let since = until - chrono::Duration::days(30);
        state
            .events_client
            .daily_counts(&session.bearer_token, since, until)
            .await
            .map(|counts| build_chart_bars(counts, &query))
            .unwrap_or_default()
    };
    let (heatmap_dates, heatmap_rows, trend_events) = state
        .events_client
        .list_events(&session.bearer_token, 1000, 0, filter_since, filter_until)
        .await
        .map(|page| {
            let events = page
                .events
                .into_iter()
                .filter(|event| {
                    matches_query(event, &query.q)
                        && (query.status.is_empty() || event.status == query.status)
                })
                .collect::<Vec<_>>();
            let (dates, rows) = build_event_heatmap(&events, &query);
            (dates, rows, events)
        })
        .unwrap_or_default();
    let chart_bars = if trend_events.is_empty() {
        fallback_chart_bars
    } else {
        let mut daily = std::collections::BTreeMap::<String, u64>::new();
        for event in &trend_events {
            *daily.entry(event.occurred_at.format("%Y-%m-%d").to_string()).or_default() += 1;
        }
        build_chart_bars(
            daily
                .into_iter()
                .map(|(date, count)| crate::events_client::DailyCount { date, count })
                .collect(),
            &query,
        )
    };
    let event_trend_chart_json = if trend_events.is_empty() {
        event_trend_chart_json_from_bars(&chart_bars)
    } else {
        event_trend_chart_json(&trend_events, &query)
    };
    let event_trend_has_data = !chart_bars.is_empty();
    let saved_views = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|query| {
            query.filter.get("view_kind").and_then(serde_json::Value::as_str) == Some("events")
        })
        .map(to_saved_event_view)
        .collect::<Vec<_>>();

    match state
        .events_client
        .list_events_filtered(
            &session.bearer_token,
            DEFAULT_PAGE_SIZE as u32,
            offset,
            filter_since,
            filter_until,
            Some(query.q.clone()),
            (!query.status.is_empty()).then(|| query.status.clone()),
        )
        .await
    {
        Ok(result) => {
            let mut events: Vec<EventSummary> =
                result.events.into_iter().filter(|e| matches_query(e, &query.q)).collect();
            sort_rows(&mut events, &query.sort, &query.dir);
            let incidents = state
                .incidents_client
                .list_incidents(session.tenant_id, None)
                .await
                .unwrap_or_default();
            let incident_options = incidents
                .iter()
                .map(|item| IncidentOption {
                    id: item.incident.id,
                    title: item.incident.title.clone(),
                    severity: item.incident.severity.to_string(),
                    status: item.incident.status.to_string(),
                    selected: query.linked_incident == Some(item.incident.id),
                })
                .collect::<Vec<_>>();
            let events: Vec<EventRow> = events
                .into_iter()
                .map(|event| {
                    let incident = incidents
                        .iter()
                        .find(|item| item.event_ids.contains(&event.id))
                        .map(|item| IncidentBadge {
                            id: item.incident.id,
                            title: item.incident.title.clone(),
                            severity: item.incident.severity.to_string(),
                            status: item.incident.status.to_string(),
                        });
                    EventRow { event, incident }
                })
                .filter(|row| matches_case_scope(row, &query.case_scope))
                .collect();
            let visible_count = events.len();
            let linked_count = events.iter().filter(|row| row.incident.is_some()).count();
            let unlinked_count = visible_count.saturating_sub(linked_count);
            let actioned_count = events.iter().filter(|row| row.event.status == "actioned").count();
            let potential_clusters = build_potential_clusters(&events);
            let metric =
                |label: String, count: usize, href: String, tone: &str| EventPostureMetric {
                    label,
                    count,
                    percent: if visible_count == 0 {
                        0
                    } else {
                        (count * 100 / visible_count) as i32
                    },
                    href,
                    tone: tone.to_string(),
                };
            let status_metrics = [
                ("New", "new", "risk"),
                ("Triggered", "triggered", "warning"),
                ("Actioned", "actioned", "good"),
                ("Dismissed", "dismissed", "neutral"),
            ]
            .into_iter()
            .map(|(label, key, tone)| {
                metric(
                    label.to_string(),
                    events.iter().filter(|row| row.event.status == key).count(),
                    scoped_event_url(&query, &[("status", key.to_string())]),
                    tone,
                )
            })
            .collect::<Vec<_>>();
            let mut type_counts = std::collections::HashMap::<String, usize>::new();
            for row in &events {
                *type_counts.entry(row.event.event_type.clone()).or_default() += 1;
            }
            let mut type_metrics = type_counts
                .into_iter()
                .map(|(label, count)| {
                    metric(
                        label.clone(),
                        count,
                        scoped_event_url(&query, &[("q", label)]),
                        "neutral",
                    )
                })
                .collect::<Vec<_>>();
            type_metrics.sort_by(|left, right| {
                right.count.cmp(&left.count).then_with(|| left.label.cmp(&right.label))
            });
            type_metrics.truncate(5);
            Html(
                EventsTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    events,
                    chart_bars,
                    event_trend_chart_json,
                    event_trend_has_data,
                    page,
                    has_more: result.has_more,
                    error: None,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                    from: query.from,
                    to: query.to,
                    status: query.status,
                    case_scope: query.case_scope,
                    visible_count,
                    linked_count,
                    unlinked_count,
                    actioned_count,
                    saved_views,
                    notice: query.notice,
                    created_incident: query.created_incident,
                    skipped_count: query.skipped_count,
                    updated_count: query.updated_count,
                    failed_count: query.failed_count,
                    incident_options,
                    linked_count_result: query.linked_count_result,
                    link_failed_count: query.link_failed_count,
                    linked_incident: query.linked_incident,
                    potential_clusters,
                    status_metrics,
                    type_metrics,
                    heatmap_dates,
                    heatmap_rows,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            EventsTemplate {
                show_nav: true,
                is_admin,
                can_write,
                events: vec![],
                chart_bars,
                event_trend_chart_json,
                event_trend_has_data,
                page,
                has_more: false,
                error: Some(e.to_string()),
                q: query.q,
                sort: query.sort,
                dir: query.dir,
                from: query.from,
                to: query.to,
                status: query.status,
                case_scope: query.case_scope,
                visible_count: 0,
                linked_count: 0,
                unlinked_count: 0,
                actioned_count: 0,
                saved_views,
                notice: query.notice,
                created_incident: query.created_incident,
                skipped_count: query.skipped_count,
                updated_count: query.updated_count,
                failed_count: query.failed_count,
                incident_options: vec![],
                linked_count_result: query.linked_count_result,
                link_failed_count: query.link_failed_count,
                linked_incident: query.linked_incident,
                potential_clusters: vec![],
                status_metrics: vec![],
                type_metrics: vec![],
                heatmap_dates,
                heatmap_rows,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

/// Successive pages fetched per export, each requesting `DEFAULT_PAGE_SIZE` rows -- bounds the
/// export to `CSV_MAX_PAGES * DEFAULT_PAGE_SIZE` rows worst case rather than looping until the
/// tenant's history is exhausted, same shape as `data_handler`'s and `login_attempts_handler`'s
/// CSV exports (ADR-0049).
const CSV_MAX_PAGES: usize = 10;

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// GET /events/export.csv — a compliance-export of the same feed `get_events` shows, honoring
/// the same `?from=`/`?to=` date-range filter, paginated internally up to `CSV_MAX_PAGES` pages
/// so a single request can produce a genuinely useful export rather than just the one page the
/// HTML view shows.
pub async fn get_events_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let (filter_since, filter_until) = parse_date_range(&query.from, &query.to);
    let mut all_events: Vec<EventSummary> = Vec::new();
    for i in 0..CSV_MAX_PAGES {
        let offset = (i as u32) * (DEFAULT_PAGE_SIZE as u32);
        let page = match state
            .events_client
            .list_events_filtered(
                &session.bearer_token,
                DEFAULT_PAGE_SIZE as u32,
                offset,
                filter_since,
                filter_until,
                Some(query.q.clone()),
                (!query.status.is_empty()).then(|| query.status.clone()),
            )
            .await
        {
            Ok(page) => page,
            Err(e) => {
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                    .into_response()
            }
        };
        let has_more = page.has_more;
        all_events.extend(page.events);
        if !has_more {
            break;
        }
    }

    if !query.case_scope.is_empty() {
        let incident_event_ids = state
            .incidents_client
            .list_incidents(session.tenant_id, None)
            .await
            .unwrap_or_default()
            .into_iter()
            .flat_map(|item| item.event_ids)
            .collect::<std::collections::HashSet<_>>();
        all_events.retain(|event| match query.case_scope.as_str() {
            "linked" => incident_event_ids.contains(&event.id),
            "unlinked" => !incident_event_ids.contains(&event.id),
            _ => true,
        });
    }

    let mut csv = String::from("occurred_at,event_type,group_key,status\n");
    for event in &all_events {
        csv.push_str(&format!(
            "{},{},{},{}\n",
            event.occurred_at.to_rfc3339(),
            csv_escape(&event.event_type),
            csv_escape(&event.group_key),
            csv_escape(&event.status),
        ));
    }

    let mut resp_headers = axum::http::HeaderMap::new();
    resp_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    resp_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"events-{}.csv\"", session.tenant_id).parse().unwrap(),
    );

    (resp_headers, csv).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveEventViewForm {
    name: String,
    q: String,
    status: String,
    from: String,
    to: String,
    sort: String,
    dir: String,
    #[serde(default)]
    case_scope: String,
}

fn event_view_redirect(form: &SaveEventViewForm, notice: &str) -> axum::response::Redirect {
    let query = serde_urlencoded::to_string([
        ("q", form.q.clone()),
        ("status", form.status.clone()),
        ("from", form.from.clone()),
        ("to", form.to.clone()),
        ("sort", form.sort.clone()),
        ("dir", form.dir.clone()),
        ("case_scope", form.case_scope.clone()),
        ("notice", notice.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    axum::response::Redirect::to(&format!("/events?{query}"))
}

pub async fn post_save_event_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveEventViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let filter = serde_json::json!({
        "view_kind": "events",
        "q": form.q.clone(),
        "status": form.status.clone(),
        "from": form.from.clone(),
        "to": form.to.clone(),
        "sort": form.sort.clone(),
        "dir": form.dir.clone(),
        "case_scope": form.case_scope.clone(),
    });
    match state.saved_search_queries_client.create(session.tenant_id, &form.name, filter).await {
        Ok(_) => event_view_redirect(&form, "view_saved").into_response(),
        Err(_) => event_view_redirect(&form, "view_failed").into_response(),
    }
}

pub async fn post_delete_event_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(()) => axum::response::Redirect::to("/events").into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}
