#[path = "analysis_config_handler_test.rs"]
#[cfg(test)]
mod analysis_config_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "analysis_config.html")]
struct AnalysisConfigTemplate {
    show_nav: bool,
    prompt: String,
    can_write: bool,
    saved: bool,
    error: Option<String>,
}

/// GET /analysis-config — the "AI Analysis" page (ADR-0019 / task "AI prompt generation for
/// agent actions"): lets an operator describe in plain English what the AI/ML backend should
/// look for when analyzing this tenant's records, closing the gap where every tenant got
/// identical, uncontrollable analysis behavior.
pub async fn get_analysis_config_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);

    match state.analysis_config_client.get_analysis_config(session.tenant_id).await {
        Ok(config) => Html(
            AnalysisConfigTemplate {
                show_nav: true,
                prompt: config.map(|c| c.prompt).unwrap_or_default(),
                can_write,
                saved: false,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            AnalysisConfigTemplate {
                show_nav: true,
                prompt: String::new(),
                can_write,
                saved: false,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct PutAnalysisConfigForm {
    prompt: String,
}

pub async fn post_analysis_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PutAnalysisConfigForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .analysis_config_client
        .put_analysis_config(session.tenant_id, session.role, &form.prompt)
        .await
    {
        Ok(config) => Html(
            AnalysisConfigTemplate {
                show_nav: true,
                prompt: config.prompt,
                can_write: true,
                saved: true,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            AnalysisConfigTemplate {
                show_nav: true,
                prompt: form.prompt,
                can_write: true,
                saved: false,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
