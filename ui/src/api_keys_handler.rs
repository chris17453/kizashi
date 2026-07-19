#[path = "api_keys_handler_test.rs"]
#[cfg(test)]
mod api_keys_handler_test;

use crate::session_guard::require_session;
use crate::{ApiKeySummary, AppState};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "api_keys.html")]
struct ApiKeysTemplate {
    show_nav: bool,
    keys: Vec<ApiKeySummary>,
    /// Set only immediately after a successful create — the one and only render where the
    /// plaintext key is ever available to show the operator.
    created_key: Option<String>,
    error: Option<String>,
}

pub async fn get_api_keys(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.api_keys_client.list_api_keys(session.tenant_id).await {
        Ok(keys) => Html(
            ApiKeysTemplate { show_nav: true, keys, created_key: None, error: None }
                .render()
                .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            ApiKeysTemplate {
                show_nav: true,
                keys: vec![],
                created_key: None,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateApiKeyForm {
    label: String,
}

pub async fn post_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<CreateApiKeyForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let created_key = match state
        .api_keys_client
        .create_api_key(session.tenant_id, &form.label)
        .await
    {
        Ok(plaintext) => Some(plaintext),
        Err(e) => {
            let keys =
                state.api_keys_client.list_api_keys(session.tenant_id).await.unwrap_or_default();
            return Html(
                ApiKeysTemplate {
                    show_nav: true,
                    keys,
                    created_key: None,
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let keys = state.api_keys_client.list_api_keys(session.tenant_id).await.unwrap_or_default();
    Html(ApiKeysTemplate { show_nav: true, keys, created_key, error: None }.render().unwrap())
        .into_response()
}

pub async fn post_revoke_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let _ = state.api_keys_client.revoke_api_key(session.tenant_id, id).await;
    Redirect::to("/api-keys").into_response()
}
