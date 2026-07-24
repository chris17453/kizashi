#[path = "password_change_handler_test.rs"]
#[cfg(test)]
mod password_change_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};

#[derive(Template)]
#[template(path = "password_settings.html")]
struct PasswordSettingsTemplate {
    show_nav: bool,
    is_admin: bool,
    error: Option<String>,
    success: bool,
    username: String,
}

#[derive(serde::Deserialize)]
pub struct PasswordSettingsQuery {
    #[serde(default)]
    changed: bool,
}

/// GET /security/password — self-service password change form (ADR-0057), not admin-gated:
/// every user changes their own password, same access bar as `/security/mfa`. `?changed=1` is
/// the post-redirect flash flag set after a successful change, so the confirmation only shows
/// once, not on a plain reload.
pub async fn get_password_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PasswordSettingsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    Html(
        PasswordSettingsTemplate {
            show_nav: true,
            is_admin,
            error: None,
            success: query.changed,
            username: session.username,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct ChangePasswordForm {
    current_password: String,
    new_password: String,
    confirm_password: String,
}

/// POST /security/password — requires re-entering the current password (Auth Service enforces
/// this too; the UI-side check mirrors the backend, same defense-in-depth convention as MFA
/// disable). `confirm_password` is a UI-only check (a typo protection, not a security control)
/// -- Auth Service never sees it.
pub async fn post_password_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<ChangePasswordForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    if form.new_password != form.confirm_password {
        return Html(
            PasswordSettingsTemplate {
                show_nav: true,
                is_admin,
                error: Some("New password and confirmation do not match.".to_string()),
                success: false,
                username: session.username.clone(),
            }
            .render()
            .unwrap(),
        )
        .into_response();
    }

    if let Err(e) = state
        .users_client
        .change_password(
            session.tenant_id,
            &session.username,
            &form.current_password,
            &form.new_password,
        )
        .await
    {
        return Html(
            PasswordSettingsTemplate {
                show_nav: true,
                is_admin,
                error: Some(e.to_string()),
                success: false,
                username: session.username,
            }
            .render()
            .unwrap(),
        )
        .into_response();
    }

    Redirect::to("/security/password?changed=true").into_response()
}
