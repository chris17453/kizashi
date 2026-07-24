#[path = "analysis_config_handler_test.rs"]
#[cfg(test)]
mod analysis_config_handler_test;

use crate::analysis_config_client::{AnalysisConfigInput, AnalysisConfigView};
use crate::execution_client::AnalysisResilienceSummary;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use common::AnalysisProvider;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "analysis_config.html")]
struct AnalysisConfigTemplate {
    show_nav: bool,
    is_admin: bool,
    tenant_id: Uuid,
    prompt: String,
    is_openai_compatible: bool,
    model: String,
    endpoint: String,
    api_key: String,
    api_key_configured: bool,
    can_write: bool,
    saved: bool,
    error: Option<String>,
    prompt_configured: bool,
    provider_label: String,
    connection_ready: bool,
    secret_configured: bool,
    analysis_queue_count: Option<u32>,
    fallback_status: String,
    consumer_status: String,
}

fn posture_fields(
    prompt: &str,
    is_openai_compatible: bool,
    model: &str,
    endpoint: &str,
) -> (bool, String, bool) {
    let prompt_configured = !prompt.trim().is_empty();
    let provider_label = if is_openai_compatible {
        "OpenAI-compatible".to_string()
    } else {
        "Azure AI Foundry".to_string()
    };
    let connection_ready =
        !is_openai_compatible || (!model.trim().is_empty() && !endpoint.trim().is_empty());
    (prompt_configured, provider_label, connection_ready)
}

fn empty_template(
    show_nav: bool,
    is_admin: bool,
    tenant_id: Uuid,
    can_write: bool,
    error: Option<String>,
    analysis_queue_count: Option<u32>,
    fallback_status: String,
    consumer_status: String,
) -> AnalysisConfigTemplate {
    AnalysisConfigTemplate {
        show_nav,
        is_admin,
        tenant_id,
        prompt: String::new(),
        is_openai_compatible: false,
        model: String::new(),
        endpoint: String::new(),
        api_key: String::new(),
        api_key_configured: false,
        can_write,
        saved: false,
        error,
        prompt_configured: false,
        provider_label: "Azure AI Foundry".to_string(),
        connection_ready: true,
        secret_configured: false,
        analysis_queue_count,
        fallback_status,
        consumer_status,
    }
}

/// Note `view.api_key` is *not* used to populate the form's API key field: on a GET this is
/// always `None` (config-admin-service never returns the real secret, see the RBAC audit fix
/// there), and on the response to a fresh PUT it's whatever the operator just typed — echoing
/// either back into the field is either a no-op or pointless. `api_key_configured` drives the
/// "already configured" messaging instead; the field itself always starts blank.
fn template_from_view(
    view: AnalysisConfigView,
    is_admin: bool,
    tenant_id: Uuid,
    can_write: bool,
    saved: bool,
    analysis_queue_count: Option<u32>,
    fallback_status: String,
    consumer_status: String,
) -> AnalysisConfigTemplate {
    let model = view.model.unwrap_or_default();
    let endpoint = view.endpoint.unwrap_or_default();
    let (prompt_configured, provider_label, connection_ready) = posture_fields(
        &view.prompt,
        view.provider == AnalysisProvider::OpenAiCompatible,
        &model,
        &endpoint,
    );
    AnalysisConfigTemplate {
        show_nav: true,
        is_admin,
        tenant_id,
        prompt: view.prompt,
        is_openai_compatible: view.provider == AnalysisProvider::OpenAiCompatible,
        model,
        endpoint,
        api_key: String::new(),
        api_key_configured: view.api_key_configured,
        can_write,
        saved,
        error: None,
        prompt_configured,
        provider_label,
        connection_ready,
        secret_configured: view.api_key_configured,
        analysis_queue_count,
        fallback_status,
        consumer_status,
    }
}

fn resilience_labels(summary: Option<&AnalysisResilienceSummary>) -> (String, String) {
    match summary {
        Some(summary) => (
            if summary.fallback_configured { "configured" } else { "not configured" }.into(),
            if summary.consumer_alive { "online" } else { "stalled" }.into(),
        ),
        None => ("unavailable".into(), "unavailable".into()),
    }
}

async fn analysis_resilience(state: &AppState) -> Option<AnalysisResilienceSummary> {
    state.execution_client.analysis_resilience().await.ok().flatten()
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
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);
    let analysis_resilience = analysis_resilience(&state).await;
    let analysis_queue_count =
        analysis_resilience.as_ref().map(|summary| summary.dead_letter_count);
    let (fallback_status, consumer_status) = resilience_labels(analysis_resilience.as_ref());

    match state.analysis_config_client.get_analysis_config(session.tenant_id).await {
        Ok(Some(config)) => Html(
            template_from_view(
                config,
                is_admin,
                session.tenant_id,
                can_write,
                false,
                analysis_queue_count,
                fallback_status,
                consumer_status,
            )
            .render()
            .unwrap(),
        )
        .into_response(),
        Ok(None) => Html(
            empty_template(
                true,
                is_admin,
                session.tenant_id,
                can_write,
                None,
                analysis_queue_count,
                fallback_status,
                consumer_status,
            )
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            empty_template(
                true,
                is_admin,
                session.tenant_id,
                can_write,
                Some(e.to_string()),
                analysis_queue_count,
                fallback_status,
                consumer_status,
            )
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct PutAnalysisConfigForm {
    prompt: String,
    #[serde(default)]
    provider: AnalysisProvider,
    #[serde(default)]
    model: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    api_key: String,
    /// Checkbox, only present in the submitted form data when checked. Needed because a blank
    /// `api_key` field is now ambiguous — "leave the existing key alone" (the normal case, since
    /// the form is never shown the real key to leave in place) vs. "actually remove it".
    #[serde(default)]
    clear_api_key: bool,
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
    let is_admin = session.role.at_least(common::Role::Admin);
    let analysis_resilience = analysis_resilience(&state).await;
    let analysis_queue_count =
        analysis_resilience.as_ref().map(|summary| summary.dead_letter_count);
    let (fallback_status, consumer_status) = resilience_labels(analysis_resilience.as_ref());
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let model = (!form.model.is_empty()).then_some(form.model.as_str());
    let endpoint = (!form.endpoint.is_empty()).then_some(form.endpoint.as_str());
    // Tri-state: checking "clear" always wins and clears the key; otherwise a non-empty field
    // sets a new key; otherwise the field says nothing, so the existing key (if any) is left
    // alone — see `AnalysisConfigInput::api_key`'s doc comment for why blank can't just mean
    // "clear" anymore.
    let api_key = if form.clear_api_key {
        Some(None)
    } else if form.api_key.is_empty() {
        None
    } else {
        Some(Some(form.api_key.as_str()))
    };
    let input = AnalysisConfigInput {
        prompt: &form.prompt,
        provider: form.provider,
        model,
        endpoint,
        api_key,
    };

    match state
        .analysis_config_client
        .put_analysis_config(session.tenant_id, session.role, &session.username, input)
        .await
    {
        Ok(config) => Html(
            template_from_view(
                config,
                is_admin,
                session.tenant_id,
                true,
                true,
                analysis_queue_count,
                fallback_status,
                consumer_status,
            )
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => {
            let (prompt_configured, provider_label, connection_ready) = posture_fields(
                &form.prompt,
                form.provider == AnalysisProvider::OpenAiCompatible,
                &form.model,
                &form.endpoint,
            );
            Html(
                AnalysisConfigTemplate {
                    show_nav: true,
                    is_admin,
                    tenant_id: session.tenant_id,
                    prompt: form.prompt,
                    is_openai_compatible: form.provider == AnalysisProvider::OpenAiCompatible,
                    model: form.model,
                    endpoint: form.endpoint,
                    api_key: String::new(),
                    // Best-effort on this error-re-render path: we don't know whether a key was
                    // already configured before this failed submission, so don't claim one is.
                    api_key_configured: false,
                    can_write: true,
                    saved: false,
                    error: Some(e.to_string()),
                    prompt_configured,
                    provider_label,
                    connection_ready,
                    secret_configured: false,
                    analysis_queue_count,
                    fallback_status,
                    consumer_status,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    }
}
