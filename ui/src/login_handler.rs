#[path = "login_handler_test.rs"]
#[cfg(test)]
pub(crate) mod login_handler_test;

use crate::{AppState, Session, SESSION_COOKIE_NAME};
use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{Html, IntoResponse, Redirect, Response};

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    show_nav: bool,
    error: Option<String>,
    tenant_name: String,
    product_name: String,
    logo_url: String,
    accent_color: String,
}

#[derive(serde::Deserialize)]
pub struct GetLoginQuery {
    #[serde(default)]
    tenant_name: String,
}

fn default_login_template(error: Option<String>) -> LoginTemplate {
    LoginTemplate {
        show_nav: false,
        error,
        tenant_name: String::new(),
        product_name: String::new(),
        logo_url: String::new(),
        accent_color: String::new(),
    }
}

/// GET /login — when a `tenant_name` is present (the workspace field's `onblur` reloads with
/// it, see login.html), looks up that workspace's white-label branding (ADR-... branding
/// feature) and applies it to this render. Branding lookup failures (unknown workspace, backend
/// down) are silent — falling back to platform defaults, not an error, since this happens as
/// the user is still typing/hasn't necessarily finished entering a real workspace name yet.
pub async fn get_login(
    State(state): State<AppState>,
    Query(query): Query<GetLoginQuery>,
) -> Response {
    if query.tenant_name.trim().is_empty() {
        return Html(default_login_template(None).render().unwrap()).into_response();
    }

    let branding =
        state.branding_client.get_branding(query.tenant_name.trim()).await.unwrap_or_default();

    Html(
        LoginTemplate {
            show_nav: false,
            error: None,
            tenant_name: query.tenant_name,
            product_name: branding.product_name.unwrap_or_default(),
            logo_url: branding.logo_url.unwrap_or_default(),
            accent_color: branding.accent_color.unwrap_or_default(),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct LoginForm {
    tenant_name: String,
    username: String,
    password: String,
}

fn login_error(message: impl Into<String>) -> Response {
    Html(default_login_template(Some(message.into())).render().unwrap()).into_response()
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
