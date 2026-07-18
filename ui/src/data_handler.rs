#[path = "data_handler_test.rs"]
#[cfg(test)]
mod data_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, RecordSearchFilter, RecordSummary};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

fn default_page() -> i64 {
    0
}

#[derive(Debug, serde::Deserialize, Default)]
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

#[derive(Template)]
#[template(path = "data.html")]
struct DataTemplate {
    show_nav: bool,
    records: Vec<RecordSummary>,
    query: DataSearchQuery,
    page: i64,
    has_more: bool,
    error: Option<String>,
}

/// GET /data — the Data Viewer: search across every connector's ingested records by
/// connector, source type, free-text substring match, and (for email-shaped records)
/// subject/from/attachment filename, tenant-scoped and paginated (`DEFAULT_PAGE_SIZE` per
/// page — every list in this platform needs to hold up at "thousands of inboxes" scale, not
/// silently truncate at an arbitrary limit).
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

    match state.stats_client.search_records(session.tenant_id, &filter).await {
        Ok(result) => Html(
            DataTemplate {
                show_nav: true,
                records: result.records,
                query,
                page,
                has_more: result.has_more,
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
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
