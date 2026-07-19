#[path = "pipeline_handler_test.rs"]
#[cfg(test)]
mod pipeline_handler_test;

use crate::session_guard::require_session;
use crate::topology::{build_topology_items, TopologyItem};
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "pipeline.html")]
struct PipelineTemplate {
    show_nav: bool,
    items: Vec<TopologyItem>,
    error: Option<String>,
}

pub async fn get_pipeline(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = require_session(state.session_store.as_ref(), &headers).await;
    if let Err(response) = session {
        return response;
    }

    let health = match state.health_client.platform_health().await {
        Ok(summary) => summary,
        Err(e) => {
            return Html(
                PipelineTemplate { show_nav: true, items: vec![], error: Some(e.to_string()) }
                    .render()
                    .unwrap(),
            )
            .into_response();
        }
    };

    // Backlog is a lower-value signal than up/down health — a lookup failure degrades this
    // page to "no backlog numbers" rather than an error page, since the topology itself is
    // still meaningful without it.
    let depths = state.backlog_client.queue_depths().await.unwrap_or_default();
    let items = build_topology_items(&health, &depths);

    Html(PipelineTemplate { show_nav: true, items, error: None }.render().unwrap()).into_response()
}
