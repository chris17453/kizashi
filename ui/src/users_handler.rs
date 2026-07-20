#[path = "users_handler_mutations_test.rs"]
#[cfg(test)]
mod users_handler_mutations_test;
#[path = "users_handler_test.rs"]
#[cfg(test)]
mod users_handler_test;

use crate::session_guard::require_session;
use crate::users_client::UiUser;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, Query, State};
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
    is_admin: bool,
    users: Vec<UserRow>,
    error: Option<String>,
    q: String,
    sort: String,
    dir: String,
}

#[derive(serde::Deserialize, Default)]
pub struct UsersQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
}

/// Case-insensitive substring match on username -- `UsersClient::list_users` has no search
/// parameter of its own (it's a full-tenant list, same shape since ADR-0016), so this filters
/// the already-fetched list in-handler rather than adding a new backend query param for what a
/// tenant's user list realistically stays small enough to filter client-side-of-the-fetch.
fn matches_query(row: &UserRow, q: &str) -> bool {
    q.is_empty() || row.username.to_lowercase().contains(&q.to_lowercase())
}

/// Sorts by whichever column header was clicked (`?sort=username|role`, default `username`),
/// same in-handler shape as the search filter above -- no backend change, since this is a
/// client-side-of-the-fetch operation on an already-small list. `dir=desc` reverses; anything
/// else (including absent) is ascending.
fn sort_rows(rows: &mut [UserRow], sort: &str, dir: &str) {
    match sort {
        "role" => rows.sort_by(|a, b| a.role_str.cmp(&b.role_str)),
        _ => rows.sort_by_key(|a| a.username.to_lowercase()),
    }
    if dir == "desc" {
        rows.reverse();
    }
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

pub async fn get_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UsersQuery>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(Role::Admin);

    match state.users_client.list_users(session.tenant_id, session.role).await {
        Ok(users) => {
            let mut rows: Vec<UserRow> = users
                .into_iter()
                .map(|u| to_row(u, &session.username))
                .filter(|row| matches_query(row, &query.q))
                .collect();
            sort_rows(&mut rows, &query.sort, &query.dir);
            Html(
                UsersTemplate {
                    show_nav: true,
                    is_admin,
                    users: rows,
                    error: None,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            UsersTemplate {
                show_nav: true,
                is_admin,
                users: vec![],
                error: Some(e.to_string()),
                q: query.q,
                sort: query.sort,
                dir: query.dir,
            }
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
    let is_admin = session.role.at_least(Role::Admin);
    let users =
        state.users_client.list_users(session.tenant_id, session.role).await.unwrap_or_default();
    Html(
        UsersTemplate {
            show_nav: true,
            is_admin,
            users: users.into_iter().map(|u| to_row(u, &session.username)).collect(),
            error: Some(error),
            q: String::new(),
            sort: String::new(),
            dir: String::new(),
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
        .create_user(
            session.tenant_id,
            session.role,
            &form.username,
            &form.password,
            form.role,
            &session.username,
        )
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

    if let Err(e) = state
        .users_client
        .update_user_role(session.tenant_id, session.role, id, form.role, &session.username)
        .await
    {
        return rerender_with_error(&state, &session, e.to_string()).await;
    }

    Redirect::to("/users").into_response()
}

/// `axum::extract::Form` deserializes via `serde_urlencoded`, which -- unlike some other form
/// crates -- does NOT collect repeated same-named fields (one checkbox per row, all named
/// `ids`) into a `Vec`; it only supports flat scalar struct fields. Parsing the raw body as a
/// flat list of `(key, value)` pairs instead and filtering for `"ids"` sidesteps that limitation
/// without adding a new dependency (`serde_urlencoded` is already a direct dependency). Same
/// pattern as API Keys' `post_bulk_revoke_api_keys` and Sensors' `post_bulk_delete_sensors`
/// (ADR-0065/ADR-0095).
fn parse_ids(raw_body: &[u8]) -> Vec<Uuid> {
    let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body) else {
        return Vec::new();
    };
    pairs
        .into_iter()
        .filter(|(key, _)| key == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect()
}

/// POST /users/bulk-delete — removes every selected user (same bulk-action pattern API Keys and
/// Sensors already have, ADR-0065/ADR-0095: loop over the existing single-item
/// `UsersClient::delete_user` rather than a new bulk backend endpoint). Best-effort per user,
/// same as the single-delete handler below -- the backend's own last-admin/self-delete
/// protections (ADR-0031) still apply per call, this handler adds no new authorization logic.
pub async fn post_bulk_delete_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    for id in parse_ids(&body) {
        let _ = state
            .users_client
            .delete_user(session.tenant_id, session.role, id, &session.username)
            .await;
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

    let _ = state
        .users_client
        .delete_user(session.tenant_id, session.role, id, &session.username)
        .await;
    Redirect::to("/users").into_response()
}

/// GET /users/:id/export — a downloadable data-subject export (ADR-0054): the account record,
/// its audit trail, and its login attempts, as returned verbatim by Auth Service.
pub async fn get_export_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.users_client.export_user_data(session.tenant_id, session.role, id).await {
        Ok(bytes) => {
            let disposition = format!("attachment; filename=\"user-{id}-export.json\"");
            ([("content-type", "application/json"), ("content-disposition", &disposition)], bytes)
                .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}
