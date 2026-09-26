#[path = "action_templates_handler_test.rs"]
#[cfg(test)]
mod action_templates_handler_test;

use crate::action_templates_client;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::{validate_action_template_config, ActionTemplate, ActionType, Role};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "action_templates.html")]
struct ActionTemplatesPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    templates: Vec<ActionTemplateView>,
    error: Option<String>,
}

struct ActionTemplateView {
    id: Uuid,
    name: String,
    description: String,
    action_type: String,
    config: String,
    version: i32,
}

#[derive(Debug, Deserialize, Default)]
pub struct ActionTemplateForm {
    pub name: String,
    pub description: String,
    pub action_type: String,
    pub config: String,
}

fn parse_action_type(value: &str) -> Option<ActionType> {
    match value {
        "email" => Some(ActionType::Email),
        "webhook" => Some(ActionType::Webhook),
        "teams_alert" => Some(ActionType::TeamsAlert),
        "create_ticket" => Some(ActionType::CreateTicket),
        "custom" => Some(ActionType::Custom),
        "generate_pdf" => Some(ActionType::GeneratePdf),
        "generate_xlsx" => Some(ActionType::GenerateXlsx),
        _ => None,
    }
}

fn template_from_form(form: ActionTemplateForm, tenant_id: Uuid) -> Result<ActionTemplate, String> {
    let action_type = parse_action_type(form.action_type.trim())
        .ok_or_else(|| "Choose a supported response provider.".to_string())?;
    let config: serde_json::Value = serde_json::from_str(&form.config)
        .map_err(|_| "Provider configuration must be valid JSON.".to_string())?;
    if !config.is_object() {
        return Err("Provider configuration must be a JSON object.".to_string());
    }
    validate_action_template_config(action_type, &config).map_err(|message| message.to_string())?;
    Ok(ActionTemplate {
        id: Uuid::nil(),
        tenant_id,
        name: form.name.trim().to_string(),
        description: form.description.trim().to_string(),
        action_type,
        config,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

async fn render(state: &AppState, headers: &HeaderMap, error: Option<String>) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let can_manage = session.role.at_least(Role::Operator);
    let templates = match action_templates_client::global() {
        Some(client) => client
            .list(session.tenant_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|template| ActionTemplateView {
                id: template.id,
                name: template.name,
                description: template.description,
                action_type: serde_json::to_value(template.action_type)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "custom".to_string()),
                config: serde_json::to_string_pretty(&template.config)
                    .unwrap_or_else(|_| "{}".to_string()),
                version: template.version,
            })
            .collect(),
        None => Vec::new(),
    };
    Html(
        ActionTemplatesPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage,
            templates,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn get_action_templates(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render(&state, &headers, None).await
}

pub async fn post_action_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ActionTemplateForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let template = match template_from_form(form, session.tenant_id) {
        Ok(value) => value,
        Err(error) => return render(&state, &headers, Some(error)).await,
    };
    let Some(client) = action_templates_client::global() else {
        return render(&state, &headers, Some("Action template service unavailable".to_string()))
            .await;
    };
    match client.create(session.role, &session.username, template).await {
        Ok(_) => Redirect::to("/action-templates").into_response(),
        Err(error) => render(&state, &headers, Some(error.to_string())).await,
    }
}

pub async fn post_update_action_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionTemplateForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let Some(client) = action_templates_client::global() else {
        return render(&state, &headers, Some("Action template service unavailable".to_string()))
            .await;
    };
    let Some(existing) = (match client.get(session.tenant_id, id).await {
        Ok(value) => value,
        Err(error) => {
            return render(&state, &headers, Some(error.to_string())).await;
        }
    }) else {
        return render(&state, &headers, Some("Action template not found".to_string())).await;
    };
    let mut template = match template_from_form(form, session.tenant_id) {
        Ok(value) => value,
        Err(error) => return render(&state, &headers, Some(error)).await,
    };
    template.id = id;
    template.created_at = existing.created_at;
    template.version = existing.version;
    match client.update(session.role, &session.username, template).await {
        Ok(_) => Redirect::to("/action-templates").into_response(),
        Err(error) => render(&state, &headers, Some(error.to_string())).await,
    }
}

pub async fn post_delete_action_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = action_templates_client::global() else {
        return render(&state, &headers, Some("Action template service unavailable".to_string()))
            .await;
    };
    match client.delete(session.role, &session.username, session.tenant_id, id).await {
        Ok(()) => Redirect::to("/action-templates").into_response(),
        Err(error) => render(&state, &headers, Some(error.to_string())).await,
    }
}
