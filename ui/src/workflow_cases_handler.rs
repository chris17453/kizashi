#[cfg(test)]
#[path = "workflow_cases_handler_test.rs"]
mod workflow_cases_handler_test;

use crate::{session_guard::require_session, workflow_client, AppState};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use common::{Role, WorkflowCase, WorkflowCaseStatus};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "workflow_cases.html")]
struct WorkflowCasesPage {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    cases: Vec<WorkflowCaseView>,
    error: Option<String>,
}
struct WorkflowCaseView {
    id: Uuid,
    execution_id: Uuid,
    kind: String,
    status: String,
    summary: String,
    due_at: String,
    overdue: bool,
    resolved: bool,
}
#[derive(Debug, Deserialize)]
pub struct WorkflowDecisionForm {
    status: String,
    note: Option<String>,
}
fn status_name(value: WorkflowCaseStatus) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}
fn case_view(value: WorkflowCase) -> WorkflowCaseView {
    let due_at = value.due_at.map(|date| date.to_rfc3339()).unwrap_or_else(|| "No SLA".to_string());
    WorkflowCaseView {
        id: value.id,
        execution_id: value.execution_id,
        kind: serde_json::to_value(value.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default(),
        status: status_name(value.status),
        summary: value.summary,
        overdue: value.due_at.is_some_and(|due| due < chrono::Utc::now())
            && value.resolved_at.is_none(),
        due_at,
        resolved: value.resolved_at.is_some(),
    }
}
fn parse_decision(value: &str) -> Option<WorkflowCaseStatus> {
    match value {
        "approved" => Some(WorkflowCaseStatus::Approved),
        "rejected" => Some(WorkflowCaseStatus::Rejected),
        "resolved" => Some(WorkflowCaseStatus::Resolved),
        _ => None,
    }
}
async fn render(state: &AppState, headers: &HeaderMap, error: Option<String>) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let cases = match workflow_client::global() {
        Some(client) => client
            .list(session.tenant_id)
            .await
            .map(|cases| cases.into_iter().map(case_view).collect())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    Html(
        WorkflowCasesPage {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            can_manage: session.role.at_least(Role::Operator),
            cases,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
pub async fn get_workflow_cases(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render(&state, &headers, None).await
}
pub async fn post_workflow_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<WorkflowDecisionForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let status = match parse_decision(&form.status) {
        Some(value) => value,
        None => {
            return render(
                &state,
                &headers,
                Some("Choose an approval, rejection, or resolution decision.".to_string()),
            )
            .await
        }
    };
    let Some(client) = workflow_client::global() else {
        return render(&state, &headers, Some("Workflow runtime is unavailable.".to_string()))
            .await;
    };
    match client
        .decide(
            session.tenant_id,
            session.role,
            &session.username,
            id,
            status,
            form.note.filter(|note| !note.trim().is_empty()),
        )
        .await
    {
        Ok(_) => Redirect::to("/workflows").into_response(),
        Err(error) => render(&state, &headers, Some(error.to_string())).await,
    }
}
