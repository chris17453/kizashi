#[path = "login_handler_test.rs"]
#[cfg(test)]
pub(crate) mod login_handler_test;

use crate::auth_client::{AuthClientError, LocalLoginResult};
use crate::{AppState, Session, SESSION_COOKIE_NAME, WORKSPACE_COOKIE_NAME};
use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{Html, IntoResponse, Redirect, Response};

/// The two cookies bridging `POST /login`'s password check and `GET`/`POST /login/mfa`'s code
/// check across separate HTTP round trips (ADR-0051) -- both `HttpOnly` and short-lived by
/// construction (the challenge token itself expires server-side after 5 minutes; the cookies
/// aren't independently time-limited beyond that, matching the SSO flow cookie's precedent).
pub const MFA_CHALLENGE_COOKIE_NAME: &str = "kizashi_mfa_challenge";
pub const MFA_USERNAME_COOKIE_NAME: &str = "kizashi_mfa_username";

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    show_nav: bool,
    is_admin: bool,
    error: Option<String>,
    tenant_name: String,
    username: String,
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
        is_admin: false,
        error,
        tenant_name: String::new(),
        username: String::new(),
        product_name: String::new(),
        logo_url: String::new(),
        accent_color: String::new(),
    }
}

/// GET /login — when a `tenant_name` is present, looks up that workspace's white-label branding (ADR-... branding
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
            is_admin: false,
            error: None,
            tenant_name: query.tenant_name,
            username: String::new(),
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

fn login_error(form: &LoginForm, message: impl Into<String>) -> Response {
    let mut template = default_login_template(Some(message.into()));
    template.tenant_name = form.tenant_name.clone();
    template.username = form.username.clone();
    Html(template.render().unwrap()).into_response()
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
        Ok(LocalLoginResult::LoggedIn { token, tenant_id, role }) => (token, tenant_id, role),
        Ok(LocalLoginResult::MfaRequired { challenge_token }) => {
            let secure = crate::cookie_secure_suffix(crate::cookie_secure());
            let mut response = Redirect::to("/login/mfa").into_response();
            response.headers_mut().append(
                SET_COOKIE,
                format!(
                    "{MFA_CHALLENGE_COOKIE_NAME}={challenge_token}; Path=/; HttpOnly; SameSite=Strict{secure}"
                )
                .parse()
                .unwrap(),
            );
            response.headers_mut().append(
                SET_COOKIE,
                format!(
                    "{MFA_USERNAME_COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Strict{secure}",
                    form.username
                )
                .parse()
                .unwrap(),
            );
            response.headers_mut().append(
                SET_COOKIE,
                format!(
                    "{WORKSPACE_COOKIE_NAME}={}; Path=/; SameSite=Strict{secure}",
                    form.tenant_name
                )
                .parse()
                .unwrap(),
            );
            return response;
        }
        Err(AuthClientError::InvalidCredentials) => {
            return login_error(&form, "Invalid workspace, username, or password")
        }
        Err(AuthClientError::Unreachable(_)) => {
            return login_error(
                &form,
                "Sign-in service is unavailable. Check platform health and try again.",
            )
        }
    };

    let session = Session {
        bearer_token,
        tenant_id,
        username: form.username,
        role,
        created_at: chrono::Utc::now(),
    };
    let session_id = state.session_store.create(session).await;

    let secure = crate::cookie_secure_suffix(crate::cookie_secure());
    let cookie =
        format!("{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Strict{secure}");
    let mut response = Redirect::to("/overview").into_response();
    response.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    response.headers_mut().append(
        SET_COOKIE,
        format!("{WORKSPACE_COOKIE_NAME}={}; Path=/; SameSite=Strict{secure}", form.tenant_name)
            .parse()
            .unwrap(),
    );
    response
}
