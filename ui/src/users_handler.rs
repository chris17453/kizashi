#[path = "users_handler_test.rs"]
#[cfg(test)]
mod users_handler_test;

use crate::session_guard::require_session;
use crate::users_client::UiUser;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::Role;
use uuid::Uuid;

/// Askama can't call arbitrary Rust methods (e.g. `Role`'s `Display` impl) from inside an
/// `{% if %}` comparison, so the role is pre-rendered to a plain string here rather than in
/// the template.
struct UserRow {
    id: Uuid,
    username: String,
    role_str: String,
    is_current: bool,
}

fn to_row(user: UiUser, current_username: &str) -> UserRow {
    UserRow {
        id: user.id,
        is_current: user.username == current_username,
        username: user.username,
        role_str: user.role.to_string(),
    }
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersTemplate {
    show_nav: bool,
    users: Vec<UserRow>,
    error: Option<String>,
}

/// Full-page access to `/users` is `Admin`-only, matching Auth Service's own enforcement
/// (ADR-0016 follow-up: user management/role-assignment is a step above the `Operator` bar
/// every other write path uses) — unlike `Viewer`-can-see-but-not-write pages elsewhere in the
/// Console UI, granting/revoking access to other users isn't something a lesser role should
/// even be able to browse.
async fn require_admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::Session, Response> {
    let session = require_session(state.session_store.as_ref(), headers).await?;
    if !session.role.at_least(Role::Admin) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(session)
}

pub async fn get_users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.users_client.list_users(session.tenant_id, session.role).await {
        Ok(users) => Html(
            UsersTemplate {
                show_nav: true,
                users: users.into_iter().map(|u| to_row(u, &session.username)).collect(),
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            UsersTemplate { show_nav: true, users: vec![], error: Some(e.to_string()) }
                .render()
                .unwrap(),
        )
        .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateUserForm {
    username: String,
    password: String,
    role: Role,
}

async fn rerender_with_error(
    state: &AppState,
    session: &crate::Session,
    error: String,
) -> Response {
    let users =
        state.users_client.list_users(session.tenant_id, session.role).await.unwrap_or_default();
    Html(
        UsersTemplate {
            show_nav: true,
            users: users.into_iter().map(|u| to_row(u, &session.username)).collect(),
            error: Some(error),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn post_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<CreateUserForm>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    if let Err(e) = state
        .users_client
        .create_user(session.tenant_id, session.role, &form.username, &form.password, form.role)
        .await
    {
        return rerender_with_error(&state, &session, e.to_string()).await;
    }

    Redirect::to("/users").into_response()
}

#[derive(serde::Deserialize)]
pub struct UpdateUserRoleForm {
    role: Role,
}

pub async fn post_update_user_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<UpdateUserRoleForm>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    if let Err(e) =
        state.users_client.update_user_role(session.tenant_id, session.role, id, form.role).await
    {
        return rerender_with_error(&state, &session, e.to_string()).await;
    }

    Redirect::to("/users").into_response()
}

pub async fn post_delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let _ = state.users_client.delete_user(session.tenant_id, session.role, id).await;
    Redirect::to("/users").into_response()
}
