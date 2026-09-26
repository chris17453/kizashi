#[cfg(test)]
#[path = "apps_handler_test.rs"]
mod apps_handler_test;

use crate::{build_studio_client, ontology_client, session_guard::require_session, AppState};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
};
use common::{validate_app_definition, AppDefinition, Role};
use serde::Deserialize;

#[derive(Template)]
#[template(path = "apps.html")]
struct AppsPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    models: Vec<AppModel>,
    apps: Vec<AppView>,
    error: Option<String>,
}
struct AppView {
    id: uuid::Uuid,
    name: String,
    description: String,
    block_count: usize,
    enabled: bool,
}
#[derive(Template)]
#[template(path = "app_detail.html")]
struct AppDetailPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    app: AppDetailView,
    error: Option<String>,
}
struct AppDetailView {
    id: uuid::Uuid,
    name: String,
    description: String,
    model_type_id: String,
    blocks_json: String,
    enabled: bool,
    version: i32,
    blocks: Vec<AppBlockView>,
}
struct AppBlockView {
    kind: String,
    href: String,
    label: String,
}
#[derive(Deserialize)]
pub struct AppForm {
    name: String,
    description: String,
    model_type_id: String,
    blocks: String,
    enabled: Option<String>,
}
struct AppModel {
    id: uuid::Uuid,
    name: String,
    property_count: usize,
}

pub async fn get_apps(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let (models, error) = match ontology_client::global() {
        Some(client) => match client.list_object_types(&session.bearer_token).await {
            Ok(types) => (
                types
                    .into_iter()
                    .map(|model| AppModel {
                        id: model.id,
                        name: model.name,
                        property_count: model
                            .property_schema
                            .as_object()
                            .map(|properties| properties.len())
                            .unwrap_or_default(),
                    })
                    .collect(),
                None,
            ),
            Err(error) => (vec![], Some(error.to_string())),
        },
        None => (vec![], Some("Models service is unavailable.".to_string())),
    };
    let apps = match build_studio_client() {
        Some(client) => client
            .list_apps(session.tenant_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|app| AppView {
                id: app.id,
                name: app.name,
                description: app.description,
                block_count: app.blocks.as_array().map(Vec::len).unwrap_or_default(),
                enabled: app.enabled,
            })
            .collect(),
        None => vec![],
    };
    Html(
        AppsPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            models,
            apps,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
pub async fn get_app_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let result = match build_studio_client() {
        Some(client) => client.get_app(session.tenant_id, id).await,
        None => Ok(None),
    };
    let (app, error) = match result {
        Ok(Some(app)) => (Some(app), None),
        Ok(None) => (None, Some("App Definition was not found.".to_string())),
        Err(error) => (None, Some(error.to_string())),
    };
    let app = app
        .map(|app| AppDetailView {
            id: app.id,
            name: app.name,
            description: app.description,
            model_type_id: app.model_type_id.map(|id| id.to_string()).unwrap_or_default(),
            blocks_json: serde_json::to_string_pretty(&app.blocks)
                .unwrap_or_else(|_| "[]".to_string()),
            enabled: app.enabled,
            version: app.version,
            blocks: app
                .blocks
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|block| {
                    let kind = block.get("kind")?.as_str()?.to_string();
                    let (href, label) = match kind.as_str() {
                        "form" => (
                            app.model_type_id
                                .map(|id| format!("/ontology?type_id={id}#object-create"))
                                .unwrap_or_else(|| "/ontology".to_string()),
                            "Open form",
                        ),
                        "table" => (
                            app.model_type_id
                                .map(|id| format!("/ontology?type_id={id}"))
                                .unwrap_or_else(|| "/ontology".to_string()),
                            "Open table",
                        ),
                        "upload" => ("/data".to_string(), "Open uploads"),
                        "queue" => ("/workflows".to_string(), "Open queue"),
                        "dashboard" => ("/".to_string(), "Open dashboard"),
                        "detail" => ("/ontology".to_string(), "Open detail view"),
                        _ => return None,
                    };
                    Some(AppBlockView { kind, href, label: label.to_string() })
                })
                .collect(),
        })
        .unwrap_or(AppDetailView {
            id,
            name: "App unavailable".to_string(),
            description: String::new(),
            model_type_id: String::new(),
            blocks_json: "[]".to_string(),
            enabled: false,
            version: 1,
            blocks: vec![],
        });
    Html(
        AppDetailPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            app,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
pub async fn post_update_app(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<AppForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let Some(client) = build_studio_client() else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Build Studio unavailable")
            .into_response();
    };
    let existing = match client.get_app(session.tenant_id, id).await {
        Ok(Some(value)) => value,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    };
    let model_type_id = if form.model_type_id.trim().is_empty() {
        None
    } else {
        match uuid::Uuid::parse_str(form.model_type_id.trim()) {
            Ok(value) => Some(value),
            Err(_) => {
                return (axum::http::StatusCode::BAD_REQUEST, "Model type must be a UUID")
                    .into_response()
            }
        }
    };
    let blocks = match serde_json::from_str(&form.blocks) {
        Ok(value) => value,
        Err(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "Blocks must be valid JSON")
                .into_response()
        }
    };
    let app = AppDefinition {
        id,
        tenant_id: session.tenant_id,
        name: form.name.trim().to_string(),
        description: form.description.trim().to_string(),
        model_type_id,
        blocks,
        enabled: form.enabled.is_some(),
        version: existing.version,
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
    };
    if let Err(message) = validate_app_definition(&app) {
        return (axum::http::StatusCode::BAD_REQUEST, message).into_response();
    }
    match client.update_app(session.role, &session.username, app).await {
        Ok(_) => Redirect::to(&format!("/apps/{id}")).into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}
pub async fn post_app(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<AppForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let model_type_id = if form.model_type_id.trim().is_empty() {
        None
    } else {
        match uuid::Uuid::parse_str(form.model_type_id.trim()) {
            Ok(value) => Some(value),
            Err(_) => {
                return (axum::http::StatusCode::BAD_REQUEST, "Model type must be a UUID")
                    .into_response()
            }
        }
    };
    let blocks = match serde_json::from_str(&form.blocks) {
        Ok(value) => value,
        Err(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "Blocks must be valid JSON")
                .into_response()
        }
    };
    let app = AppDefinition {
        id: uuid::Uuid::nil(),
        tenant_id: session.tenant_id,
        name: form.name.trim().to_string(),
        description: form.description.trim().to_string(),
        model_type_id,
        blocks,
        enabled: form.enabled.is_some(),
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    if let Err(message) = validate_app_definition(&app) {
        return (axum::http::StatusCode::BAD_REQUEST, message).into_response();
    }
    let Some(client) = build_studio_client() else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Build Studio unavailable")
            .into_response();
    };
    match client.create_app(session.role, &session.username, app).await {
        Ok(_) => Redirect::to("/apps").into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}
