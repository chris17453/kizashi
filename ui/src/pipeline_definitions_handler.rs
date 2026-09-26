#[cfg(test)]
#[path = "pipeline_definitions_handler_test.rs"]
mod pipeline_definitions_handler_test;

use crate::{build_studio_client, session_guard::require_session, AppState};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use common::{validate_pipeline_definition, PipelineDefinition, PipelineMode, Role};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "pipeline_definitions.html")]
struct PipelinesPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    pipelines: Vec<PipelineListItem>,
    error: Option<String>,
}
struct PipelineListItem {
    id: Uuid,
    name: String,
    mode: String,
    enabled: bool,
}

#[derive(Template)]
#[template(path = "pipeline_definition_detail.html")]
struct PipelineDetailPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    pipeline: PipelineView,
    sources: Vec<DataSourceOption>,
    error: Option<String>,
}
struct PipelineView {
    id: Option<Uuid>,
    name: String,
    description: String,
    data_source_id: String,
    target_object_type_id: String,
    mode: String,
    steps: String,
    enabled: bool,
    version: i32,
}
struct DataSourceOption {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct PipelineDefinitionForm {
    name: String,
    description: String,
    data_source_id: String,
    target_object_type_id: String,
    mode: String,
    steps: String,
    enabled: Option<String>,
}

fn parse_pipeline_mode(value: &str) -> Option<PipelineMode> {
    match value {
        "projection" => Some(PipelineMode::Projection),
        "command" => Some(PipelineMode::Command),
        _ => None,
    }
}
fn mode_name(value: PipelineMode) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}
fn pipeline_view(pipeline: Option<PipelineDefinition>) -> PipelineView {
    match pipeline {
        Some(pipeline) => PipelineView {
            id: Some(pipeline.id),
            name: pipeline.name,
            description: pipeline.description,
            data_source_id: pipeline.data_source_id.to_string(),
            target_object_type_id: pipeline
                .target_object_type_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            mode: mode_name(pipeline.mode),
            steps: serde_json::to_string_pretty(&pipeline.steps)
                .unwrap_or_else(|_| "[]".to_string()),
            enabled: pipeline.enabled,
            version: pipeline.version,
        },
        None => PipelineView {
            id: None,
            name: String::new(),
            description: String::new(),
            data_source_id: String::new(),
            target_object_type_id: String::new(),
            mode: "projection".to_string(),
            steps: "[\n  {\"kind\": \"extract\", \"config\": {}}\n]".to_string(),
            enabled: true,
            version: 1,
        },
    }
}
fn pipeline_from_form(
    form: PipelineDefinitionForm,
    tenant_id: Uuid,
) -> Result<PipelineDefinition, String> {
    let data_source_id = Uuid::parse_str(form.data_source_id.trim())
        .map_err(|_| "Choose a Data Source.".to_string())?;
    let target_object_type_id = if form.target_object_type_id.trim().is_empty() {
        None
    } else {
        Some(
            Uuid::parse_str(form.target_object_type_id.trim())
                .map_err(|_| "Target model type must be a UUID.".to_string())?,
        )
    };
    let mode = parse_pipeline_mode(form.mode.trim())
        .ok_or_else(|| "Choose a supported pipeline mode.".to_string())?;
    let steps =
        serde_json::from_str(&form.steps).map_err(|_| "Steps must be valid JSON.".to_string())?;
    let pipeline = PipelineDefinition {
        id: Uuid::nil(),
        tenant_id,
        name: form.name.trim().to_string(),
        description: form.description.trim().to_string(),
        data_source_id,
        target_object_type_id,
        mode,
        steps,
        enabled: form.enabled.is_some(),
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    validate_pipeline_definition(&pipeline).map_err(str::to_string)?;
    Ok(pipeline)
}
async fn sources_for(tenant_id: Uuid) -> Vec<DataSourceOption> {
    match build_studio_client() {
        Some(client) => client
            .list_data_sources(tenant_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|source| DataSourceOption { id: source.id.to_string(), name: source.name })
            .collect(),
        None => Vec::new(),
    }
}
async fn render_list(state: &AppState, headers: &HeaderMap, error: Option<String>) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let pipelines = match build_studio_client() {
        Some(client) => client
            .list_pipelines(session.tenant_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|pipeline| PipelineListItem {
                id: pipeline.id,
                name: pipeline.name,
                mode: mode_name(pipeline.mode),
                enabled: pipeline.enabled,
            })
            .collect(),
        None => Vec::new(),
    };
    Html(
        PipelinesPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            pipelines,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
async fn render_detail(
    state: &AppState,
    headers: &HeaderMap,
    pipeline: Option<PipelineDefinition>,
    error: Option<String>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    Html(
        PipelineDetailPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            pipeline: pipeline_view(pipeline),
            sources: sources_for(session.tenant_id).await,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
pub async fn get_pipelines(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render_list(&state, &headers, None).await
}
pub async fn get_new_pipeline(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render_detail(&state, &headers, None, None).await
}
pub async fn get_pipeline_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(client) = build_studio_client() else {
        return render_detail(
            &state,
            &headers,
            None,
            Some("Pipeline service is unavailable.".to_string()),
        )
        .await;
    };
    match client.get_pipeline(session.tenant_id, id).await {
        Ok(Some(pipeline)) => render_detail(&state, &headers, Some(pipeline), None).await,
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => render_detail(&state, &headers, None, Some(error.to_string())).await,
    }
}
pub async fn post_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PipelineDefinitionForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let pipeline = match pipeline_from_form(form, session.tenant_id) {
        Ok(value) => value,
        Err(error) => return render_detail(&state, &headers, None, Some(error)).await,
    };
    let Some(client) = build_studio_client() else {
        return render_detail(
            &state,
            &headers,
            None,
            Some("Pipeline service is unavailable.".to_string()),
        )
        .await;
    };
    match client.create_pipeline(session.role, &session.username, pipeline).await {
        Ok(pipeline) => Redirect::to(&format!("/build/pipelines/{}", pipeline.id)).into_response(),
        Err(error) => render_detail(&state, &headers, None, Some(error.to_string())).await,
    }
}
pub async fn post_update_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<PipelineDefinitionForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let Some(client) = build_studio_client() else {
        return render_detail(
            &state,
            &headers,
            None,
            Some("Pipeline service is unavailable.".to_string()),
        )
        .await;
    };
    let existing = match client.get_pipeline(session.tenant_id, id).await {
        Ok(Some(value)) => value,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return render_detail(&state, &headers, None, Some(error.to_string())).await,
    };
    let mut pipeline = match pipeline_from_form(form, session.tenant_id) {
        Ok(value) => value,
        Err(error) => return render_detail(&state, &headers, Some(existing), Some(error)).await,
    };
    pipeline.id = id;
    pipeline.version = existing.version;
    pipeline.created_at = existing.created_at;
    match client.update_pipeline(session.role, &session.username, pipeline).await {
        Ok(pipeline) => Redirect::to(&format!("/build/pipelines/{}", pipeline.id)).into_response(),
        Err(error) => {
            render_detail(&state, &headers, Some(existing), Some(error.to_string())).await
        }
    }
}
