#[path = "data_handler_test.rs"]
#[cfg(test)]
mod data_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
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
    records: Vec<RecordSummary>,
    query: DataSearchQuery,
    page: i64,
    has_more: bool,
    saved_searches: Vec<SavedSearchRow>,
    can_write: bool,
    reprocessed: Option<usize>,
    error: Option<String>,
    sensor_names: Vec<String>,
}

/// Upper bound on how many registered sensor names populate the Connector ID datalist — a
/// plain HTML datalist stops being a usable picker well before "thousands of sensors" scale,
/// so past this we still let the operator type any connector_id freely (the field always
/// accepts free text; the datalist is autocomplete convenience, not a hard picker).
const SENSOR_NAME_SUGGESTION_LIMIT: i64 = 500;

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

    let page = query.page.max(0);
    let (from, to) = parse_date_range(&query.from, &query.to);
    let normalized = match query.normalized.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    };
    let filter = RecordSearchFilter {
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
        limit: DEFAULT_PAGE_SIZE,
        offset: page * DEFAULT_PAGE_SIZE,
    };

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

    let sensor_names = state
        .sensors_client
        .list_sensors(session.tenant_id, SENSOR_NAME_SUGGESTION_LIMIT, 0)
        .await
        .map(|page| page.sensors.into_iter().map(|s| s.name).collect())
        .unwrap_or_default();

    match state.stats_client.search_records(session.tenant_id, &filter).await {
        Ok(result) => Html(
            DataTemplate {
                show_nav: true,
                records: result.records,
                query,
                page,
                has_more: result.has_more,
                saved_searches,
                can_write,
                reprocessed,
                error: None,
                sensor_names,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            DataTemplate {
                show_nav: true,
                records: vec![],
                query,
                page,
                has_more: false,
                saved_searches,
                can_write,
                reprocessed,
                error: Some(e.to_string()),
                sensor_names,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

/// POST /data/reprocess — operator-gated: republishes `record.ingested` for every one of this
/// tenant's unnormalized records (the recovery path for records ingested before a
/// `NormalizationMapping` existed for their source type). A UI wrapper around the API-only
/// `POST /v1/records/reprocess` capability shipped without one initially.
pub async fn post_reprocess(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let republished = state.stats_client.reprocess(session.tenant_id).await.unwrap_or(0);
    Redirect::to(&format!("/data?reprocessed={republished}")).into_response()
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
        connector_id: form.connector_id,
        source_type: form.source_type,
        q: form.q,
        subject: form.subject,
        email_from: form.email_from,
        attachment_filename: form.attachment_filename,
        from: form.from,
        to: form.to,
        normalized: form.normalized,
        page: 0,
        reprocessed: None,
    };
    let _ = state
        .saved_search_queries_client
        .create(session.tenant_id, &form.name, serde_json::to_value(&filter).unwrap_or_default())
        .await;

    Redirect::to("/data").into_response()
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
