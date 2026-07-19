#[path = "events_handler_test.rs"]
#[cfg(test)]
mod events_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, EventSummary};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

fn default_page() -> i64 {
    0
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
}

#[derive(Template)]
#[template(path = "events.html")]
struct EventsTemplate {
    show_nav: bool,
    events: Vec<EventSummary>,
    page: i64,
    has_more: bool,
    error: Option<String>,
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

    let page = query.page.max(0);
    let offset = (page * DEFAULT_PAGE_SIZE) as u32;
    match state
        .events_client
        .list_events(&session.bearer_token, DEFAULT_PAGE_SIZE as u32, offset)
        .await
    {
        Ok(result) => Html(
            EventsTemplate {
                show_nav: true,
                events: result.events,
                page,
                has_more: result.has_more,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            EventsTemplate {
                show_nav: true,
                events: vec![],
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
