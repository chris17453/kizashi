#[path = "mfa_login_handler_test.rs"]
#[cfg(test)]
mod mfa_login_handler_test;

use crate::login_handler::{MFA_CHALLENGE_COOKIE_NAME, MFA_USERNAME_COOKIE_NAME};
use crate::session_guard::session_cookie_value_named;
use crate::{AppState, Session, SESSION_COOKIE_NAME};
use askama::Template;
use axum::extract::{Form, State};
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};

#[derive(Template)]
#[template(path = "login_mfa.html")]
struct MfaChallengeTemplate {
    show_nav: bool,
    error: Option<String>,
}

fn expire_cookie(name: &str) -> String {
    format!("{name}=; Path=/; HttpOnly; Max-Age=0")
}

/// GET /login/mfa — the second step of a local login for a user with MFA enabled (ADR-0051).
/// Redirects back to `/login` if the bridging cookies `POST /login` sets aren't present, since
/// this page is meaningless without an in-flight challenge (someone navigating here directly,
/// or after the challenge cookies already expired/were consumed).
pub async fn get_mfa_challenge(headers: HeaderMap) -> Response {
    if session_cookie_value_named(&headers, MFA_CHALLENGE_COOKIE_NAME).is_none() {
        return Redirect::to("/login").into_response();
    }
    Html(MfaChallengeTemplate { show_nav: false, error: None }.render().unwrap()).into_response()
}

#[derive(serde::Deserialize)]
pub struct MfaChallengeForm {
    code: String,
}

fn challenge_error(message: impl Into<String>) -> Response {
    Html(MfaChallengeTemplate { show_nav: false, error: Some(message.into()) }.render().unwrap())
        .into_response()
}

/// POST /login/mfa — completes the login started by `POST /login`, exchanging the challenge
/// cookie plus a submitted code for a real session, the same way `post_login` does for a
/// non-MFA user.
pub async fn post_mfa_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<MfaChallengeForm>,
) -> Response {
    let Some(challenge_token) = session_cookie_value_named(&headers, MFA_CHALLENGE_COOKIE_NAME)
    else {
        return Redirect::to("/login").into_response();
    };
    let username = session_cookie_value_named(&headers, MFA_USERNAME_COOKIE_NAME)
        .unwrap_or_else(|| "unknown".to_string());

    let (bearer_token, tenant_id, role) =
        match state.mfa_client.challenge(&challenge_token, &form.code).await {
            Ok(result) => result,
            Err(_) => return challenge_error("Invalid or expired code"),
        };

    let session =
        Session { bearer_token, tenant_id, username, role, created_at: chrono::Utc::now() };
    let session_id = state.session_store.create(session).await;

    let secure = crate::cookie_secure_suffix(crate::cookie_secure());
    let cookie =
        format!("{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Strict{secure}");
    let mut response = Redirect::to("/overview").into_response();
    response.headers_mut().append(SET_COOKIE, cookie.parse().unwrap());
    response
        .headers_mut()
        .append(SET_COOKIE, expire_cookie(MFA_CHALLENGE_COOKIE_NAME).parse().unwrap());
    response
        .headers_mut()
        .append(SET_COOKIE, expire_cookie(MFA_USERNAME_COOKIE_NAME).parse().unwrap());
    response
}
