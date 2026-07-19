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
use common::SavedSearchQuery;
use serde::Serialize;
use uuid::Uuid;

fn default_page() -> i64 {
    0
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
    #[serde(default = "default_page")]
    pub page: i64,
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
    error: Option<String>,
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

    let page = query.page.max(0);
    let filter = RecordSearchFilter {
        connector_id: (!query.connector_id.is_empty()).then(|| query.connector_id.clone()),
        source_type: (!query.source_type.is_empty()).then(|| query.source_type.clone()),
        query: (!query.q.is_empty()).then(|| query.q.clone()),
        subject: (!query.subject.is_empty()).then(|| query.subject.clone()),
        email_from: (!query.email_from.is_empty()).then(|| query.email_from.clone()),
        attachment_filename: (!query.attachment_filename.is_empty())
            .then(|| query.attachment_filename.clone()),
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

    match state.stats_client.search_records(session.tenant_id, &filter).await {
        Ok(result) => Html(
            DataTemplate {
                show_nav: true,
                records: result.records,
                query,
                page,
                has_more: result.has_more,
                saved_searches,
                error: None,
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
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
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
        page: 0,
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
