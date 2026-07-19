#[path = "login_handler_test.rs"]
#[cfg(test)]
mod login_handler_test;

use crate::{AppState, Session, SESSION_COOKIE_NAME};
use askama::Template;
use axum::extract::{Form, State};
use axum::http::header::SET_COOKIE;
use axum::response::{Html, IntoResponse, Redirect, Response};

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    show_nav: bool,
    error: Option<String>,
}

pub async fn get_login() -> Response {
    Html(LoginTemplate { show_nav: false, error: None }.render().unwrap()).into_response()
}

#[derive(serde::Deserialize)]
pub struct LoginForm {
    tenant_name: String,
    username: String,
    password: String,
}

fn login_error(message: impl Into<String>) -> Response {
    Html(LoginTemplate { show_nav: false, error: Some(message.into()) }.render().unwrap())
        .into_response()
}

/// POST /login — the browser never talks to Auth Service directly; Console UI's backend does,
/// then establishes its own session (ADR-0014, since Auth Service has no session/cookie layer
/// of its own per ADR-0009).
pub async fn post_login(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let (bearer_token, tenant_id, role) = match state
        .auth_client
        .local_login(&form.tenant_name, &form.username, &form.password)
        .await
    {
        Ok(result) => result,
        Err(_) => return login_error("Invalid workspace, username, or password"),
    };

    let session = Session { bearer_token, tenant_id, username: form.username, role };
    let session_id = state.session_store.create(session).await;

    let cookie = format!("{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Strict");
    let mut response = Redirect::to("/overview").into_response();
    response.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    response
}
