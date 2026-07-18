#[path = "data_detail_handler_test.rs"]
#[cfg(test)]
mod data_detail_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "data_detail.html")]
struct DataDetailTemplate {
    show_nav: bool,
    record: Option<RecordSummary>,
    raw_payload_pretty: String,
    normalized_payload_pretty: Option<String>,
    error: Option<String>,
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// GET /data/:id — the Data Viewer's record detail: full raw and normalized payload.
pub async fn get_data_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.stats_client.get_record(session.tenant_id, id).await {
        Ok(Some(record)) => {
            let raw_payload_pretty = pretty(&record.raw_payload);
            let normalized_payload_pretty = record.normalized_payload.as_ref().map(pretty);
            Html(
                DataDetailTemplate {
                    show_nav: true,
                    record: Some(record),
                    raw_payload_pretty,
                    normalized_payload_pretty,
                    error: None,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Ok(None) => Html(
            DataDetailTemplate {
                show_nav: true,
                record: None,
                raw_payload_pretty: String::new(),
                normalized_payload_pretty: None,
                error: Some("no record with that id".to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            DataDetailTemplate {
                show_nav: true,
                record: None,
                raw_payload_pretty: String::new(),
                normalized_payload_pretty: None,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
