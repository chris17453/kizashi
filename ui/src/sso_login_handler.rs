#[path = "sso_login_handler_test.rs"]
#[cfg(test)]
mod sso_login_handler_test;

use crate::pending_oidc_flow::PendingOidcFlow;
use crate::{AppState, Session, SESSION_COOKIE_NAME};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};

const OIDC_FLOW_COOKIE_NAME: &str = "kizashi_oidc_flow";

/// Only provider wired up so far — mirrors `entra_oidc_client()` being the only OIDC client
/// Auth Service configures today (ADR-0009). Extending this to a per-tenant provider picker is
/// a follow-up, not a v1 requirement.
const DEFAULT_PROVIDER: &str = "entra";

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    show_nav: bool,
    is_admin: bool,
    error: Option<String>,
    tenant_name: String,
    product_name: String,
    logo_url: String,
    accent_color: String,
}

fn sso_error(message: impl Into<String>) -> Response {
    Html(
        LoginTemplate {
            show_nav: false,
            is_admin: false,
            error: Some(message.into()),
            tenant_name: String::new(),
            product_name: String::new(),
            logo_url: String::new(),
            accent_color: String::new(),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct SsoLoginQuery {
    tenant_name: String,
    #[serde(default)]
    provider: Option<String>,
}

/// GET /login/sso — starts the OIDC login flow: asks Auth Service for an authorization URL
/// (ADR-0009), stashes what `/login/sso/callback` will need to finish the exchange behind a
/// short-lived, single-use, `HttpOnly` cookie (there is nowhere else to keep it between the two
/// browser hops), then redirects the browser to the identity provider.
pub async fn get_sso_login(
    State(state): State<AppState>,
    Query(query): Query<SsoLoginQuery>,
) -> Response {
    let provider = query.provider.as_deref().unwrap_or(DEFAULT_PROVIDER);

    let authorization = match state.oidc_client.authorize(provider).await {
        Ok(authorization) => authorization,
        Err(e) => {
            tracing::warn!(error = %e, "sso authorize failed");
            return sso_error(
                "Single sign-on is not available for this workspace right now. Use your username and password instead.",
            );
        }
    };

    let flow = PendingOidcFlow {
        provider: provider.to_string(),
        csrf_token: authorization.csrf_token,
        code_verifier: authorization.code_verifier,
        tenant_name: query.tenant_name,
    };
    let flow_id = state.pending_oidc_flow_store.create(flow).await;

    // SameSite=Lax (not Strict, unlike the main session cookie): the browser leaves this site
    // for the IdP and comes straight back via a top-level GET redirect, which is exactly the
    // navigation Strict cookies are dropped on — Lax still blocks it from being sent on
    // cross-site subrequests/POSTs, just not this top-level GET.
    let secure = crate::cookie_secure_suffix(crate::cookie_secure());
    let cookie = format!(
        "{OIDC_FLOW_COOKIE_NAME}={flow_id}; Path=/login/sso; HttpOnly; SameSite=Lax; Max-Age=600{secure}"
    );
    let mut response = Redirect::to(&authorization.authorization_url).into_response();
    response.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    response
}

#[derive(serde::Deserialize)]
pub struct SsoCallbackQuery {
    code: String,
    state: String,
}

fn flow_id_from_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&format!("{OIDC_FLOW_COOKIE_NAME}=")) {
            return Some(value.to_string());
        }
    }
    None
}

/// GET /login/sso/callback — the identity provider redirects the browser back here with an
/// authorization code. Verifies the `state` query param against the CSRF token stashed by
/// `/login/sso` (rejecting the request outright on mismatch or a missing/expired flow, since
/// that's exactly the cross-site-request-forgery scenario the OIDC `state` parameter exists to
/// catch), completes the code exchange via Auth Service, then mints the same kind of session
/// local login does (ADR-0014).
pub async fn get_sso_callback(
    State(state): State<AppState>,
    Query(query): Query<SsoCallbackQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(flow_id) = flow_id_from_cookie(&headers) else {
        return sso_error("Sign-in request could not be verified. Please try again.");
    };

    // `take`, not `get`: single-use, so a captured/replayed callback URL can't mint a second
    // session from the same authorization.
    let Some(flow) = state.pending_oidc_flow_store.take(&flow_id).await else {
        return sso_error("Sign-in request could not be verified. Please try again.");
    };

    if query.state != flow.csrf_token {
        return sso_error("Sign-in request could not be verified. Please try again.");
    }

    let session = match state
        .oidc_client
        .callback(&flow.provider, &query.code, &flow.code_verifier, &flow.tenant_name)
        .await
    {
        Ok(session) => session,
        Err(e) => {
            tracing::warn!(error = %e, "sso callback exchange failed");
            return sso_error("Sign-in failed. Please try again.");
        }
    };

    let username = session.username.unwrap_or_else(|| format!("sso:{}", flow.tenant_name));
    let session_id = state
        .session_store
        .create(Session {
            bearer_token: session.bearer_token,
            tenant_id: session.tenant_id,
            username,
            role: session.role,
            created_at: chrono::Utc::now(),
        })
        .await;

    let secure = crate::cookie_secure_suffix(crate::cookie_secure());
    let cookie =
        format!("{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Strict{secure}");
    let mut response = Redirect::to("/overview").into_response();
    response.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    response
}
