#[path = "triggers_handler_test.rs"]
#[cfg(test)]
mod triggers_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, TriggerSummary};
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "triggers.html")]
struct TriggersTemplate {
    show_nav: bool,
    triggers: Vec<TriggerSummary>,
    error: Option<String>,
}

pub async fn get_triggers(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.triggers_client.list_triggers(session.tenant_id).await {
        Ok(triggers) => {
            Html(TriggersTemplate { show_nav: true, triggers, error: None }.render().unwrap())
                .into_response()
        }
        Err(e) => Html(
            TriggersTemplate { show_nav: true, triggers: vec![], error: Some(e.to_string()) }
                .render()
                .unwrap(),
        )
        .into_response(),
    }
}
