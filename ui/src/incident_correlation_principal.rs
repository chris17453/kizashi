use crate::session_guard::require_session;
use crate::{AppState, Session};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

pub(crate) struct ConsolePrincipal {
    pub(crate) tenant_id: Uuid,
    pub(crate) role: common::Role,
    pub(crate) username: String,
    pub(crate) bearer_token: String,
}

impl From<&Session> for ConsolePrincipal {
    fn from(session: &Session) -> Self {
        Self {
            tenant_id: session.tenant_id,
            role: session.role,
            username: session.username.clone(),
            bearer_token: session.bearer_token.clone(),
        }
    }
}

pub(crate) async fn require_api_principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ConsolePrincipal, Response> {
    let Some(value) = headers.get("authorization").and_then(|value| value.to_str().ok()) else {
        let session = require_session(state.session_store.as_ref(), headers).await?;
        return Ok(ConsolePrincipal::from(&session));
    };
    let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))
    else {
        return Err((axum::http::StatusCode::UNAUTHORIZED, "expected bearer token").into_response());
    };
    let principal = state.auth_client.introspect_service_account(token).await.map_err(|error| {
        let status = match error {
            crate::AuthClientError::InvalidCredentials => axum::http::StatusCode::UNAUTHORIZED,
            _ => axum::http::StatusCode::BAD_GATEWAY,
        };
        (status, error.to_string()).into_response()
    })?;
    Ok(ConsolePrincipal {
        tenant_id: principal.tenant_id,
        role: principal.role,
        username: format!("service-account:{}", principal.label),
        bearer_token: principal.query_token,
    })
}
