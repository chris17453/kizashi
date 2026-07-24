#[path = "data_handler_mutations_test.rs"]
#[cfg(test)]
mod data_handler_mutations_test;
#[path = "data_handler_test.rs"]
#[cfg(test)]
mod data_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::ontology_client::CreateObjectRequest;
use crate::session_guard::require_session;
use crate::{AppState, RecordSearchFilter, RecordSummary};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use common::SavedSearchQuery;
use serde::Serialize;
use uuid::Uuid;

fn default_page() -> i64 {
    0
}

/// Parses `YYYY-MM-DD` date-only strings from `<input type="date">` into a fully inclusive
/// `DateTime<Utc>` range -- `from` at the start of that day, `to` at the end of it, so
/// "2026-07-15 to 2026-07-20" covers all of both endpoint days. An unparseable/empty string
/// (including the common case of only one end of the range being set) yields `None`, not an
/// error -- the field is just omitted from the search filter.
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

#[derive(Debug, serde::Deserialize, Serialize, Default, Clone)]
pub struct DataSearchQuery {
    #[serde(default)]
    pub connector_id: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub email_from: String,
    #[serde(default)]
    pub attachment_filename: String,
    /// Plain `YYYY-MM-DD` from an `<input type="date">`, not a full timestamp -- parsed by hand
    /// rather than deserialized straight to `DateTime<Utc>` since query-string date inputs
    /// arrive date-only. `from` is treated as the start of that day, `to` as the end of it, so
    /// a range like "2026-07-15 to 2026-07-20" is fully inclusive of both endpoints.
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    /// "true"/"false"/empty from a `<select>` -- a plain `Option<bool>` field would reject the
    /// empty "no filter" case as invalid, so this stays a string and gets parsed by hand too.
    #[serde(default)]
    pub normalized: String,
    #[serde(default = "default_page")]
    pub page: i64,
    /// Set by the redirect after `POST /data/reprocess` so the page can show a confirmation —
    /// not a real search filter, never serialized back into `to_saved_search_row`'s bookmark.
    #[serde(default)]
    #[serde(skip_serializing)]
    pub reprocessed: Option<usize>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub reprocessed_connector: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub modeled: Option<usize>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub model_failed: Option<usize>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub notice: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default, Clone)]
struct DataContext {
    #[serde(default)]
    connector_id: String,
    #[serde(default)]
    source_type: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    email_from: String,
    #[serde(default)]
    attachment_filename: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    normalized: String,
}

impl DataContext {
    fn query(&self, extras: &[(&str, String)]) -> String {
        let mut pairs = Vec::new();
        for (key, value) in [
            ("connector_id", &self.connector_id),
            ("source_type", &self.source_type),
            ("q", &self.q),
            ("subject", &self.subject),
            ("email_from", &self.email_from),
            ("attachment_filename", &self.attachment_filename),
            ("from", &self.from),
            ("to", &self.to),
            ("normalized", &self.normalized),
        ] {
            if !value.is_empty() && !extras.iter().any(|(extra_key, _)| *extra_key == key) {
                pairs.push((key, value.clone()));
            }
        }
        pairs.extend(extras.iter().map(|(key, value)| (*key, value.clone())));
        serde_urlencoded::to_string(pairs).unwrap_or_default()
    }

    fn from_query(query: &DataSearchQuery) -> Self {
        Self {
            connector_id: query.connector_id.clone(),
            source_type: query.source_type.clone(),
            q: query.q.clone(),
            subject: query.subject.clone(),
            email_from: query.email_from.clone(),
            attachment_filename: query.attachment_filename.clone(),
            from: query.from.clone(),
            to: query.to.clone(),
            normalized: query.normalized.clone(),
        }
    }
}

struct ModelTypeOption {
    id: Uuid,
    name: String,
}

struct DataSourceBucket {
    source_type: String,
    count: usize,
    percent: i32,
    href: String,
}

struct DataTimelinePoint {
    label: String,
    count: usize,
    height_pct: i32,
    href: String,
}

struct DataConnectorHeatmapRow {
    connector_id: String,
    normalized_count: usize,
    unnormalized_count: usize,
    normalized_percent: i32,
    href: String,
}

/// A saved query's filter, plus a pre-built `/data?...` query string so the template can link
/// straight to it without re-deriving field-by-field URL encoding in Askama (which can't call
/// arbitrary Rust functions).
struct SavedSearchRow {
    id: Uuid,
    name: String,
    load_url: String,
}

fn to_saved_search_row(query: SavedSearchQuery) -> SavedSearchRow {
    let filter: DataSearchQuery = serde_json::from_value(query.filter).unwrap_or_default();
    let load_url = format!("/data?{}", serde_urlencoded::to_string(&filter).unwrap_or_default());
    SavedSearchRow { id: query.id, name: query.name, load_url }
}

#[derive(Template)]
#[template(path = "data.html")]
struct DataTemplate {
    show_nav: bool,
    is_admin: bool,
    records: Vec<RecordSummary>,
    query: DataSearchQuery,
    page: i64,
    has_more: bool,
    saved_searches: Vec<SavedSearchRow>,
    can_write: bool,
    reprocessed: Option<usize>,
    error: Option<String>,
    sensor_names: Vec<String>,
    visible_count: usize,
    normalized_count: usize,
    unnormalized_count: usize,
    connector_count: usize,
    model_types: Vec<ModelTypeOption>,
    source_buckets: Vec<DataSourceBucket>,
    ingestion_timeline: Vec<DataTimelinePoint>,
    connector_heatmap: Vec<DataConnectorHeatmapRow>,
}

/// Upper bound on how many registered sensor names populate the Connector ID datalist — a
/// plain HTML datalist stops being a usable picker well before "thousands of sensors" scale,
/// so past this we still let the operator type any connector_id freely (the field always
/// accepts free text; the datalist is autocomplete convenience, not a hard picker).
const SENSOR_NAME_SUGGESTION_LIMIT: i64 = 500;

/// Shared by the HTML view and the CSV export -- the two must never silently diverge on what
/// counts as "matching the current search," same reasoning as `recent_audit_log_handler`'s
/// `fetch_merged_page`.
fn build_filter(query: &DataSearchQuery, limit: i64, offset: i64) -> RecordSearchFilter {
    let (from, to) = parse_date_range(&query.from, &query.to);
    let normalized = match query.normalized.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    };
    RecordSearchFilter {
        connector_id: (!query.connector_id.is_empty()).then(|| query.connector_id.clone()),
        source_type: (!query.source_type.is_empty()).then(|| query.source_type.clone()),
        query: (!query.q.is_empty()).then(|| query.q.clone()),
        subject: (!query.subject.is_empty()).then(|| query.subject.clone()),
        email_from: (!query.email_from.is_empty()).then(|| query.email_from.clone()),
        attachment_filename: (!query.attachment_filename.is_empty())
            .then(|| query.attachment_filename.clone()),
        from,
        to,
        normalized,
        limit,
        offset,
    }
}

/// GET /data — the Data Viewer: search across every connector's ingested records by
/// connector, source type, free-text substring match, and (for email-shaped records)
/// subject/from/attachment filename, tenant-scoped and paginated (`DEFAULT_PAGE_SIZE` per
/// page — every list in this platform needs to hold up at "thousands of inboxes" scale, not
/// silently truncate at an arbitrary limit). Also lists saved searches (ADR-0029) so a filter
/// can be bookmarked and revisited.
pub async fn get_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataSearchQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let page = query.page.max(0);
    let filter = build_filter(&query, DEFAULT_PAGE_SIZE, page * DEFAULT_PAGE_SIZE);

    let saved_searches = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(to_saved_search_row)
        .collect();

    let can_write = session.role.at_least(common::Role::Operator);
    let reprocessed = query.reprocessed;
    let context = DataContext::from_query(&query);

    let sensor_names = state
        .sensors_client
        .list_sensors(session.tenant_id, SENSOR_NAME_SUGGESTION_LIMIT, 0)
        .await
        .map(|page| page.sensors.into_iter().map(|s| s.name).collect())
        .unwrap_or_default();
    let model_types = if can_write {
        if let Some(client) = crate::ontology_client::global() {
            client
                .list_object_types(&session.bearer_token)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|object_type| ModelTypeOption { id: object_type.id, name: object_type.name })
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    match state.stats_client.search_records(session.tenant_id, &filter).await {
        Ok(result) => {
            let visible_count = result.records.len();
            let normalized_count =
                result.records.iter().filter(|record| record.is_normalized()).count();
            let unnormalized_count = visible_count.saturating_sub(normalized_count);
            let connector_count = result
                .records
                .iter()
                .map(|record| record.connector_id.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len();
            let mut source_counts = std::collections::HashMap::<String, usize>::new();
            for record in &result.records {
                *source_counts.entry(record.source_type.clone()).or_default() += 1;
            }
            let mut source_buckets = source_counts
                .into_iter()
                .map(|(source_type, count)| DataSourceBucket {
                    href: format!(
                        "/data?{}",
                        context.query(&[("source_type", source_type.clone())])
                    ),
                    source_type,
                    count,
                    percent: if visible_count == 0 {
                        0
                    } else {
                        (count * 100 / visible_count) as i32
                    },
                })
                .collect::<Vec<_>>();
            source_buckets.sort_by(|left, right| {
                right.count.cmp(&left.count).then_with(|| left.source_type.cmp(&right.source_type))
            });
            let mut day_counts = std::collections::BTreeMap::<chrono::NaiveDate, usize>::new();
            let mut connector_counts = std::collections::HashMap::<String, (usize, usize)>::new();
            for record in &result.records {
                *day_counts.entry(record.ingested_at.date_naive()).or_default() += 1;
                let counts = connector_counts.entry(record.connector_id.clone()).or_default();
                if record.is_normalized() {
                    counts.0 += 1;
                } else {
                    counts.1 += 1;
                }
            }
            let max_day_count = day_counts.values().copied().max().unwrap_or(1);
            let ingestion_timeline = day_counts
                .into_iter()
                .map(|(date, count)| DataTimelinePoint {
                    label: date.format("%b %d").to_string(),
                    count,
                    height_pct: (count * 100 / max_day_count).max(8) as i32,
                    href: format!(
                        "/data?{}",
                        context.query(&[("from", date.to_string()), ("to", date.to_string())])
                    ),
                })
                .collect::<Vec<_>>();
            let mut connector_heatmap = connector_counts
                .into_iter()
                .map(|(connector_id, (normalized_count, unnormalized_count))| {
                    let total = normalized_count + unnormalized_count;
                    DataConnectorHeatmapRow {
                        href: format!(
                            "/data?{}",
                            context.query(&[("connector_id", connector_id.clone())])
                        ),
                        connector_id,
                        normalized_count,
                        unnormalized_count,
                        normalized_percent: if total == 0 {
                            0
                        } else {
                            (normalized_count * 100 / total) as i32
                        },
                    }
                })
                .collect::<Vec<_>>();
            connector_heatmap.sort_by(|left, right| {
                (right.normalized_count + right.unnormalized_count)
                    .cmp(&(left.normalized_count + left.unnormalized_count))
                    .then_with(|| left.connector_id.cmp(&right.connector_id))
            });
            Html(
                DataTemplate {
                    show_nav: true,
                    is_admin,
                    records: result.records,
                    query,
                    page,
                    has_more: result.has_more,
                    saved_searches,
                    can_write,
                    reprocessed,
                    error: None,
                    sensor_names,
                    visible_count,
                    normalized_count,
                    unnormalized_count,
                    connector_count,
                    model_types,
                    source_buckets,
                    ingestion_timeline,
                    connector_heatmap,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            DataTemplate {
                show_nav: true,
                is_admin,
                records: vec![],
                query,
                page,
                has_more: false,
                saved_searches,
                can_write,
                reprocessed,
                error: Some(e.to_string()),
                sensor_names,
                visible_count: 0,
                normalized_count: 0,
                unnormalized_count: 0,
                connector_count: 0,
                model_types,
                source_buckets: vec![],
                ingestion_timeline: vec![],
                connector_heatmap: vec![],
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

/// Successive pages fetched per export, each page requesting the backend's own max page size --
/// bounds the export to `CSV_MAX_PAGES * DEFAULT_PAGE_SIZE` rows worst case rather than looping
/// until the search is exhausted, since an unbounded export against a very large filtered
/// result could otherwise take an unreasonable time/response size. Same tradeoff
/// `recent_audit_log_handler`'s CSV export already made (ADR-0049).
const CSV_MAX_PAGES: usize = 20;

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// GET /data/export.csv — a bulk export of the current filtered search, honoring every field
/// `DataSearchQuery` accepts (via the same `build_filter` the HTML view uses, so the two can
/// never silently diverge on what "the current search" means). Closes a real data-explorer gap:
/// until now, handing a filtered record set to another tool meant clicking into records one at
/// a time. Paginated internally up to `CSV_MAX_PAGES` pages -- a tenant with more matching
/// records than that isn't capped forever, since search filters (especially a date range) can
/// be narrowed to export the rest in a follow-up request, same as the audit log export.
pub async fn get_data_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataSearchQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let mut all_records = Vec::new();
    for page_num in 0..CSV_MAX_PAGES {
        let filter = build_filter(&query, DEFAULT_PAGE_SIZE, page_num as i64 * DEFAULT_PAGE_SIZE);
        match state.stats_client.search_records(session.tenant_id, &filter).await {
            Ok(result) => {
                let has_more = result.has_more;
                all_records.extend(result.records);
                if !has_more {
                    break;
                }
            }
            Err(e) => {
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                    .into_response();
            }
        }
    }

    let mut csv = String::from("id,connector_id,source_type,ingested_at,normalized,raw_payload\n");
    for record in &all_records {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            record.id,
            csv_escape(&record.connector_id),
            csv_escape(&record.source_type),
            record.ingested_at.to_rfc3339(),
            record.is_normalized(),
            csv_escape(&record.raw_payload.to_string()),
        ));
    }

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"data-export-{}.csv\"", session.tenant_id).parse().unwrap(),
    );
    (headers, csv).into_response()
}

/// POST /data/reprocess — operator-gated: republishes `record.ingested` for every one of this
/// tenant's unnormalized records (the recovery path for records ingested before a
/// `NormalizationMapping` existed for their source type). A UI wrapper around the API-only
/// `POST /v1/records/reprocess` capability shipped without one initially.
pub async fn post_reprocess(
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

    let context = serde_urlencoded::from_bytes::<DataContext>(&body).unwrap_or_default();
    let connector_id =
        (!context.connector_id.trim().is_empty()).then_some(context.connector_id.clone());
    let republished = state
        .stats_client
        .reprocess_for_connector(session.tenant_id, connector_id.as_deref())
        .await
        .unwrap_or(0);
    let mut extras = vec![("reprocessed", republished.to_string())];
    if let Some(connector_id) = connector_id {
        extras.push(("reprocessed_connector", connector_id));
    }
    let query = context.query(&extras);
    Redirect::to(&format!("/data?{query}")).into_response()
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct SelectedRecordsForm {
    #[serde(default)]
    pub ids: String,
    #[serde(flatten)]
    context: DataContext,
}

#[derive(Debug, serde::Deserialize)]
pub struct ModelSelectedRecordsForm {
    #[serde(default)]
    pub ids: String,
    pub object_type_id: Uuid,
    #[serde(flatten)]
    context: DataContext,
}

/// POST /data/reprocess-selected — operator-gated recovery for the explicitly selected result
/// rows. Each record remains tenant-scoped by the ingestion service and normalized records are
/// harmless no-ops, so a mixed selection can be submitted from an investigation window.
pub async fn post_reprocess_selected(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<SelectedRecordsForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let mut republished = 0;
    for id in form.ids.split(',').filter_map(|value| Uuid::parse_str(value.trim()).ok()).take(25) {
        if let Ok(count) = state.stats_client.reprocess_record(session.tenant_id, id).await {
            republished += count;
        }
    }
    let query = form.context.query(&[("reprocessed", republished.to_string())]);
    Redirect::to(&format!("/data?{query}")).into_response()
}

/// POST /data/model-selected — promotes up to 25 selected source records into governed
/// ontology objects. The normalized payload is preferred when available and every object keeps
/// its source record id as lineage so investigators can move back to evidence.
pub async fn post_model_selected(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<ModelSelectedRecordsForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let Some(client) = crate::ontology_client::global() else {
        let query = form.context.query(&[("notice", "model-selected-failed".to_string())]);
        return Redirect::to(&format!("/data?{query}")).into_response();
    };
    let mut modeled = 0usize;
    let mut model_failed = 0usize;
    for id in form.ids.split(',').filter_map(|value| Uuid::parse_str(value.trim()).ok()).take(25) {
        let Ok(Some(record)) = state.stats_client.get_record(session.tenant_id, id).await else {
            model_failed += 1;
            continue;
        };
        let payload = record.normalized_payload.unwrap_or(record.raw_payload);
        let properties =
            if payload.is_object() { payload } else { serde_json::json!({ "value": payload }) };
        let input = CreateObjectRequest {
            object_type_id: form.object_type_id,
            properties,
            source_lineage: serde_json::json!([id]),
        };
        match client.create_object(&session.bearer_token, &input).await {
            Ok(()) => modeled += 1,
            Err(_) => model_failed += 1,
        }
    }
    let query = form.context.query(&[
        ("notice", "model-selected".to_string()),
        ("modeled", modeled.to_string()),
        ("model_failed", model_failed.to_string()),
    ]);
    Redirect::to(&format!("/data?{query}")).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveSearchForm {
    name: String,
    #[serde(default)]
    connector_id: String,
    #[serde(default)]
    source_type: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    email_from: String,
    #[serde(default)]
    attachment_filename: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    normalized: String,
}

fn data_search_redirect(form: &SaveSearchForm, notice: &str) -> Redirect {
    let query = serde_urlencoded::to_string([
        ("connector_id", form.connector_id.clone()),
        ("source_type", form.source_type.clone()),
        ("q", form.q.clone()),
        ("subject", form.subject.clone()),
        ("email_from", form.email_from.clone()),
        ("attachment_filename", form.attachment_filename.clone()),
        ("from", form.from.clone()),
        ("to", form.to.clone()),
        ("normalized", form.normalized.clone()),
        ("notice", notice.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    Redirect::to(&format!("/data?{query}"))
}

/// POST /data/saved-searches — no `require_operator` gate (ADR-0029): any authenticated tenant
/// member can bookmark a search.
pub async fn post_save_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<SaveSearchForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let filter = DataSearchQuery {
        connector_id: form.connector_id.clone(),
        source_type: form.source_type.clone(),
        q: form.q.clone(),
        subject: form.subject.clone(),
        email_from: form.email_from.clone(),
        attachment_filename: form.attachment_filename.clone(),
        from: form.from.clone(),
        to: form.to.clone(),
        normalized: form.normalized.clone(),
        page: 0,
        reprocessed: None,
        reprocessed_connector: None,
        modeled: None,
        model_failed: None,
        notice: String::new(),
    };
    let result = state
        .saved_search_queries_client
        .create(session.tenant_id, &form.name, serde_json::to_value(&filter).unwrap_or_default())
        .await;
    match result {
        Ok(_) => data_search_redirect(&form, "saved").into_response(),
        Err(_) => data_search_redirect(&form, "save_failed").into_response(),
    }
}

pub async fn post_delete_saved_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let _ = state.saved_search_queries_client.delete(session.tenant_id, id).await;
    Redirect::to("/data").into_response()
}
