#[path = "mfa_settings_handler_test.rs"]
#[cfg(test)]
mod mfa_settings_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};

#[derive(Template)]
#[template(path = "mfa_settings.html")]
struct MfaSettingsTemplate {
    show_nav: bool,
    enabled: bool,
    error: Option<String>,
    enrollment: Option<EnrollmentView>,
}

struct EnrollmentView {
    secret_base32: String,
    qr_code_base64_png: String,
}

async fn render_status(state: &AppState, headers: &HeaderMap, error: Option<String>) -> Response {
    let session = match require_session(state.session_store.as_ref(), headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let enabled =
        state.mfa_client.status(session.tenant_id, &session.username).await.unwrap_or(false);

    Html(MfaSettingsTemplate { show_nav: true, enabled, error, enrollment: None }.render().unwrap())
        .into_response()
}

/// GET /security/mfa — shows the caller's own MFA status (self-service, not admin-gated: every
/// user manages their own second factor). If enrollment is in progress (`mfa_enroll` was just
/// called but not yet confirmed), this page doesn't remember that -- re-enrolling always starts
/// fresh (ADR-0051), so the QR/secret only ever appears immediately after `POST
/// /security/mfa/enroll`, never on a plain reload.
pub async fn get_mfa_settings(State(state): State<AppState>, headers: HeaderMap) -> Response {
    render_status(&state, &headers, None).await
}

/// POST /security/mfa/enroll — generates a fresh secret and renders it immediately (QR code +
/// manual-entry text) alongside a verification form, rather than redirecting, since the secret
/// is only ever shown this once.
pub async fn post_mfa_enroll(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.mfa_client.enroll(session.tenant_id, &session.username).await {
        Ok(enrollment) => Html(
            MfaSettingsTemplate {
                show_nav: true,
                enabled: false,
                error: None,
                enrollment: Some(EnrollmentView {
                    secret_base32: enrollment.secret_base32,
                    qr_code_base64_png: enrollment.qr_code_base64_png,
                }),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => render_status(&state, &headers, Some(e.to_string())).await,
    }
}

#[derive(serde::Deserialize)]
pub struct MfaVerifyForm {
    code: String,
}

/// POST /security/mfa/verify — confirms enrollment. On a wrong code, the pending secret from
/// `enroll` is still stored server-side, but this page doesn't re-show the QR (the secret isn't
/// returned again) -- the user must click "Enable MFA" again, which generates a fresh secret and
/// starts over. Simple, if not maximally convenient; acceptable for a v1 (ADR-0051).
pub async fn post_mfa_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<MfaVerifyForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    if let Err(e) = state.mfa_client.verify(session.tenant_id, &session.username, &form.code).await
    {
        return render_status(
            &state,
            &headers,
            Some(format!("Incorrect code -- {e}. Click \"Enable MFA\" to try again.")),
        )
        .await;
    }

    Redirect::to("/security/mfa").into_response()
}

#[derive(serde::Deserialize)]
pub struct MfaDisableForm {
    password: String,
}

/// POST /security/mfa/disable — requires re-entering the account password (Auth Service enforces
/// this too; the UI-side check is defense-in-depth, matching every other write path's
/// client-mirrors-backend convention).
pub async fn post_mfa_disable(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<MfaDisableForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    if let Err(e) =
        state.mfa_client.disable(session.tenant_id, &session.username, &form.password).await
    {
        return render_status(&state, &headers, Some(e.to_string())).await;
    }

    Redirect::to("/security/mfa").into_response()
}
