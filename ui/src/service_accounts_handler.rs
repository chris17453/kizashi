#[path = "service_accounts_handler_test.rs"]
#[cfg(test)]
mod service_accounts_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, ServiceAccountSummary};
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::Role;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Default)]
pub struct ServiceAccountForm {
    pub label: String,
    pub role: String,
}

#[derive(Template)]
#[template(path = "service_accounts.html")]
struct ServiceAccountsTemplate {
    show_nav: bool,
    is_admin: bool,
    accounts: Vec<ServiceAccountSummary>,
    token: Option<String>,
    error: Option<String>,
    can_manage: bool,
}

async fn render(
    state: &AppState,
    headers: &HeaderMap,
    token: Option<String>,
    error: Option<String>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    let can_manage = session.role.at_least(Role::Admin);
    let accounts = if can_manage {
        state
            .auth_client
            .list_service_accounts(session.tenant_id, session.role)
            .await
            .unwrap_or_default()
    } else {
        vec![]
    };
    Html(
        ServiceAccountsTemplate {
            show_nav: true,
            is_admin: can_manage,
            accounts,
            token,
            error,
            can_manage,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn get_service_accounts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render(&state, &headers, None, None).await
}

pub async fn post_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ServiceAccountForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !session.role.at_least(Role::Admin) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let role = match form.role.parse() {
        Ok(v) => v,
        Err(_) => {
            return render(
                &state,
                &headers,
                None,
                Some("Choose a valid service-account role.".to_string()),
            )
            .await
        }
    };
    match state
        .auth_client
        .create_service_account(
            session.tenant_id,
            session.role,
            &form.label,
            role,
            &session.username,
        )
        .await
    {
        Ok((_, token)) => render(&state, &headers, Some(token), None).await,
        Err(error) => render(&state, &headers, None, Some(error.to_string())).await,
    }
}

pub async fn post_revoke_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !session.role.at_least(Role::Admin) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let result = state
        .auth_client
        .revoke_service_account(session.tenant_id, session.role, id, &session.username)
        .await;
    if result.is_err() {
        return render(
            &state,
            &headers,
            None,
            Some("Unable to revoke service account.".to_string()),
        )
        .await;
    }
    Redirect::to("/service-accounts").into_response()
}
