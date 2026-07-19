#[path = "triggers_handler_test.rs"]
#[cfg(test)]
mod triggers_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, TriggerSummary};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

fn default_page() -> i64 {
    0
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct TriggersQuery {
    #[serde(default = "default_page")]
    pub page: i64,
}

#[derive(Template)]
#[template(path = "triggers.html")]
struct TriggersTemplate {
    show_nav: bool,
    triggers: Vec<TriggerSummary>,
    page: i64,
    has_more: bool,
    error: Option<String>,
}

pub async fn get_triggers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TriggersQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let page = query.page.max(0);
    match state
        .triggers_client
        .list_triggers(session.tenant_id, DEFAULT_PAGE_SIZE, page * DEFAULT_PAGE_SIZE)
        .await
    {
        Ok(result) => Html(
            TriggersTemplate {
                show_nav: true,
                triggers: result.triggers,
                page,
                has_more: result.has_more,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            TriggersTemplate {
                show_nav: true,
                triggers: vec![],
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
