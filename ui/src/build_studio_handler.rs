#[cfg(test)]
#[path = "build_studio_handler_test.rs"]
mod build_studio_handler_test;

use crate::{build_studio_client, session_guard::require_session, AppState};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
};
use common::{validate_data_source, DataSource, DataSourceKind, DataSourceMode, Role};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "build_studio.html")]
struct BuildStudioPage {
    show_nav: bool,
    is_admin: bool,
    sources: Vec<String>,
    pipelines: Vec<String>,
    error: Option<String>,
}

pub async fn get_build_studio(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(client) = build_studio_client() else {
        return Html(
            BuildStudioPage {
                show_nav: true,
                is_admin: false,
                sources: vec![],
                pipelines: vec![],
                error: Some("Build Studio is unavailable.".to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response();
    };
    let sources = client.list_data_sources(session.tenant_id).await;
    let pipelines = client.list_pipelines(session.tenant_id).await;
    let error = if sources.is_err() || pipelines.is_err() {
        Some("Configuration service is temporarily unavailable.".to_string())
    } else {
        None
    };
    Html(
        BuildStudioPage {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            sources: sources.unwrap_or_default().into_iter().map(|value| value.name).collect(),
            pipelines: pipelines.unwrap_or_default().into_iter().map(|value| value.name).collect(),
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Template)]
#[template(path = "data_sources.html")]
struct DataSourcesPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    sources: Vec<DataSourceListItem>,
    error: Option<String>,
}

struct DataSourceListItem {
    id: Uuid,
    name: String,
    kind: String,
    mode: String,
    enabled: bool,
}

#[derive(Template)]
#[template(path = "data_source_detail.html")]
struct DataSourceDetailPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    source: DataSourceView,
    error: Option<String>,
}

struct DataSourceView {
    id: Option<Uuid>,
    name: String,
    description: String,
    kind: String,
    mode: String,
    connection: String,
    credential_ref: String,
    enabled: bool,
    version: i32,
}

#[derive(Debug, Deserialize, Default)]
pub struct DataSourceForm {
    name: String,
    description: String,
    kind: String,
    mode: String,
    connection: String,
    credential_ref: String,
    enabled: Option<String>,
}

fn parse_data_source_kind(value: &str) -> Option<DataSourceKind> {
    match value {
        "database" => Some(DataSourceKind::Database),
        "api" => Some(DataSourceKind::Api),
        "stream" => Some(DataSourceKind::Stream),
        "cdc" => Some(DataSourceKind::Cdc),
        "upload" => Some(DataSourceKind::Upload),
        "batch" => Some(DataSourceKind::Batch),
        _ => None,
    }
}
fn parse_data_source_mode(value: &str) -> Option<DataSourceMode> {
    match value {
        "read" => Some(DataSourceMode::Read),
        "projection" => Some(DataSourceMode::Projection),
        "command" => Some(DataSourceMode::Command),
        _ => None,
    }
}
fn kind_name(value: DataSourceKind) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}
fn mode_name(value: DataSourceMode) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}
fn source_view(source: Option<DataSource>) -> DataSourceView {
    match source {
        Some(source) => DataSourceView {
            id: Some(source.id),
            name: source.name,
            description: source.description,
            kind: kind_name(source.kind),
            mode: mode_name(source.mode),
            connection: serde_json::to_string_pretty(&source.connection)
                .unwrap_or_else(|_| "{}".to_string()),
            credential_ref: source.credential_ref.unwrap_or_default(),
            enabled: source.enabled,
            version: source.version,
        },
        None => DataSourceView {
            id: None,
            name: String::new(),
            description: String::new(),
            kind: "api".to_string(),
            mode: "read".to_string(),
            connection: "{}".to_string(),
            credential_ref: String::new(),
            enabled: true,
            version: 1,
        },
    }
}
fn source_from_form(form: DataSourceForm, tenant_id: Uuid) -> Result<DataSource, String> {
    let kind = parse_data_source_kind(form.kind.trim())
        .ok_or_else(|| "Choose a supported source kind.".to_string())?;
    let mode = parse_data_source_mode(form.mode.trim())
        .ok_or_else(|| "Choose a supported source mode.".to_string())?;
    let connection = serde_json::from_str(&form.connection)
        .map_err(|_| "Connection metadata must be valid JSON.".to_string())?;
    let source = DataSource {
        id: Uuid::nil(),
        tenant_id,
        name: form.name.trim().to_string(),
        description: form.description.trim().to_string(),
        kind,
        mode,
        connection,
        credential_ref: (!form.credential_ref.trim().is_empty())
            .then(|| form.credential_ref.trim().to_string()),
        enabled: form.enabled.is_some(),
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    validate_data_source(&source).map_err(str::to_string)?;
    Ok(source)
}
async fn render_sources(state: &AppState, headers: &HeaderMap, error: Option<String>) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let sources = match build_studio_client() {
        Some(client) => client
            .list_data_sources(session.tenant_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|source| DataSourceListItem {
                id: source.id,
                name: source.name,
                kind: kind_name(source.kind),
                mode: mode_name(source.mode),
                enabled: source.enabled,
            })
            .collect(),
        None => Vec::new(),
    };
    Html(
        DataSourcesPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            sources,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
pub async fn get_data_sources(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render_sources(&state, &headers, None).await
}
async fn render_source_detail(
    state: &AppState,
    headers: &HeaderMap,
    source: Option<DataSource>,
    error: Option<String>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    Html(
        DataSourceDetailPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            source: source_view(source),
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
pub async fn get_data_source_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(client) = build_studio_client() else {
        return render_source_detail(
            &state,
            &headers,
            None,
            Some("Data Source service is unavailable.".to_string()),
        )
        .await;
    };
    match client.get_data_source(session.tenant_id, id).await {
        Ok(Some(source)) => render_source_detail(&state, &headers, Some(source), None).await,
        Ok(None) => axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(error) => render_source_detail(&state, &headers, None, Some(error.to_string())).await,
    }
}
pub async fn get_new_data_source(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render_source_detail(&state, &headers, None, None).await
}
pub async fn post_data_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<DataSourceForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let source = match source_from_form(form, session.tenant_id) {
        Ok(value) => value,
        Err(error) => return render_source_detail(&state, &headers, None, Some(error)).await,
    };
    let Some(client) = build_studio_client() else {
        return render_source_detail(
            &state,
            &headers,
            None,
            Some("Data Source service is unavailable.".to_string()),
        )
        .await;
    };
    match client.create_data_source(session.role, &session.username, source).await {
        Ok(source) => Redirect::to(&format!("/build/data-sources/{}", source.id)).into_response(),
        Err(error) => render_source_detail(&state, &headers, None, Some(error.to_string())).await,
    }
}
pub async fn post_update_data_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<DataSourceForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let Some(client) = build_studio_client() else {
        return render_source_detail(
            &state,
            &headers,
            None,
            Some("Data Source service is unavailable.".to_string()),
        )
        .await;
    };
    let existing = match client.get_data_source(session.tenant_id, id).await {
        Ok(Some(value)) => value,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            return render_source_detail(&state, &headers, None, Some(error.to_string())).await
        }
    };
    let mut source = match source_from_form(form, session.tenant_id) {
        Ok(value) => value,
        Err(error) => {
            return render_source_detail(&state, &headers, Some(existing), Some(error)).await
        }
    };
    source.id = id;
    source.version = existing.version;
    source.created_at = existing.created_at;
    match client.update_data_source(session.role, &session.username, source).await {
        Ok(source) => Redirect::to(&format!("/build/data-sources/{}", source.id)).into_response(),
        Err(error) => {
            render_source_detail(&state, &headers, Some(existing), Some(error.to_string())).await
        }
    }
}
