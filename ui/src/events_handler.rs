#[path = "events_handler_test.rs"]
#[cfg(test)]
mod events_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, EventSummary};
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "events.html")]
struct EventsTemplate {
    show_nav: bool,
    events: Vec<EventSummary>,
    error: Option<String>,
}

pub async fn get_events(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.events_client.list_events(&session.bearer_token).await {
        Ok(events) => {
            Html(EventsTemplate { show_nav: true, events, error: None }.render().unwrap())
                .into_response()
        }
        Err(e) => Html(
            EventsTemplate { show_nav: true, events: vec![], error: Some(e.to_string()) }
                .render()
                .unwrap(),
        )
        .into_response(),
    }
}
