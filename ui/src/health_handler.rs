#[path = "health_handler_test.rs"]
#[cfg(test)]
mod health_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, ServiceHealthSummary};
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "health.html")]
struct HealthTemplate {
    show_nav: bool,
    is_admin: bool,
    platform_status: Option<String>,
    services: Vec<ServiceHealthSummary>,
    error: Option<String>,
}

pub async fn get_health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    match state.health_client.platform_health().await {
        Ok(summary) => Html(
            HealthTemplate {
                show_nav: true,
                is_admin,
                platform_status: Some(summary.status),
                services: summary.services,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            HealthTemplate {
                show_nav: true,
                is_admin,
                platform_status: None,
                services: vec![],
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
