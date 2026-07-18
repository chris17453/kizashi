#[path = "data_handler_test.rs"]
#[cfg(test)]
mod data_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, RecordSearchFilter, RecordSummary};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Debug, serde::Deserialize, Default)]
pub struct DataSearchQuery {
    #[serde(default)]
    pub connector_id: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub q: String,
}

#[derive(Template)]
#[template(path = "data.html")]
struct DataTemplate {
    show_nav: bool,
    records: Vec<RecordSummary>,
    query: DataSearchQuery,
    error: Option<String>,
}

/// GET /data — the Data Viewer: search across every connector's ingested records by
/// connector, source type, and free-text substring match, tenant-scoped.
pub async fn get_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataSearchQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let filter = RecordSearchFilter {
        connector_id: (!query.connector_id.is_empty()).then(|| query.connector_id.clone()),
        source_type: (!query.source_type.is_empty()).then(|| query.source_type.clone()),
        query: (!query.q.is_empty()).then(|| query.q.clone()),
    };

    match state.stats_client.search_records(session.tenant_id, &filter).await {
        Ok(records) => {
            Html(DataTemplate { show_nav: true, records, query, error: None }.render().unwrap())
                .into_response()
        }
        Err(e) => Html(
            DataTemplate { show_nav: true, records: vec![], query, error: Some(e.to_string()) }
                .render()
                .unwrap(),
        )
        .into_response(),
    }
}
