#[path = "api_keys_handler_mutations_test.rs"]
#[cfg(test)]
mod api_keys_handler_mutations_test;
#[path = "api_keys_handler_test.rs"]
#[cfg(test)]
mod api_keys_handler_test;

use crate::session_guard::require_session;
use crate::{ApiKeySummary, AppState};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use uuid::Uuid;

#[derive(serde::Deserialize, Default)]
pub struct ApiKeysQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
}

/// Case-insensitive substring match on label -- same in-handler-filter shape as the Users page
/// search (ADR-0062): `ApiKeysClient::list_api_keys` has no search parameter, and a tenant's key
/// list is realistically small enough that filtering the already-fetched list is the right size
/// of fix, not a new backend query param.
fn matches_query(key: &ApiKeySummary, q: &str) -> bool {
    q.is_empty() || key.label.to_lowercase().contains(&q.to_lowercase())
}

/// Same shape as Sensors' sortable columns (ADR-0070): applied after the search filter, on the
/// already-fetched full list (this page has no server-side pagination to preserve ordering
/// across). An unset `sort` keeps `list_api_keys`' own default (creation order).
fn sort_rows(rows: &mut [ApiKeySummary], sort: &str, dir: &str) {
    match sort {
        "created_at" => rows.sort_by_key(|k| k.created_at),
        "label" => rows.sort_by_key(|k| k.label.to_lowercase()),
        _ => return,
    }
    if dir == "desc" {
        rows.reverse();
    }
}

#[derive(Template)]
#[template(path = "api_keys.html")]
struct ApiKeysTemplate {
    show_nav: bool,
    is_admin: bool,
    keys: Vec<ApiKeySummary>,
    /// Set only immediately after a successful create — the one and only render where the
    /// plaintext key is ever available to show the operator.
    created_key: Option<String>,
    /// RBAC v1 (ADR-0016): hides the create form and revoke buttons from a `Viewer` — matches
    /// server-side enforcement (`ingestion-gateway`'s `api_key_handlers.rs` calls
    /// `require_operator` on create/revoke), this is presentation-layer convenience, not the
    /// only gate.
    can_write: bool,
    error: Option<String>,
    q: String,
    sort: String,
    dir: String,
}

pub async fn get_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ApiKeysQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    match state.api_keys_client.list_api_keys(session.tenant_id).await {
        Ok(keys) => {
            let mut keys: Vec<ApiKeySummary> =
                keys.into_iter().filter(|k| matches_query(k, &query.q)).collect();
            sort_rows(&mut keys, &query.sort, &query.dir);
            Html(
                ApiKeysTemplate {
                    show_nav: true,
                    is_admin,
                    keys,
                    created_key: None,
                    can_write,
                    error: None,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            ApiKeysTemplate {
                show_nav: true,
                is_admin,
                keys: vec![],
                created_key: None,
                can_write,
                error: Some(e.to_string()),
                q: query.q,
                sort: query.sort,
                dir: query.dir,
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
    let is_admin = session.role.at_least(common::Role::Admin);
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let can_write = true;

    let created_key = match state
        .api_keys_client
        .create_api_key(session.tenant_id, session.role, &form.label, &session.username)
        .await
    {
        Ok(plaintext) => Some(plaintext),
        Err(e) => {
            let keys =
                state.api_keys_client.list_api_keys(session.tenant_id).await.unwrap_or_default();
            return Html(
                ApiKeysTemplate {
                    show_nav: true,
                    is_admin,
                    keys,
                    created_key: None,
                    can_write,
                    error: Some(e.to_string()),
                    q: String::new(),
                    sort: String::new(),
                    dir: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let keys = state.api_keys_client.list_api_keys(session.tenant_id).await.unwrap_or_default();
    Html(
        ApiKeysTemplate {
            show_nav: true,
            is_admin,
            keys,
            created_key,
            can_write,
            error: None,
            q: String::new(),
            sort: String::new(),
            dir: String::new(),
        }
        .render()
        .unwrap(),
    )
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
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let _ = state
        .api_keys_client
        .revoke_api_key(session.tenant_id, session.role, id, &session.username)
        .await;
    Redirect::to("/api-keys").into_response()
}

/// `axum::extract::Form` deserializes via `serde_urlencoded`, which -- unlike some other form
/// crates -- does NOT collect repeated same-named fields (one checkbox per row, all named
/// `ids`) into a `Vec`; it only supports flat scalar struct fields. Parsing the raw body as a
/// flat list of `(key, value)` pairs instead and filtering for `"ids"` sidesteps that limitation
/// without adding a new dependency (`serde_urlencoded` is already a direct dependency).
fn parse_ids(raw_body: &[u8]) -> Vec<Uuid> {
    let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body) else {
        return Vec::new();
    };
    pairs
        .into_iter()
        .filter(|(key, _)| key == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect()
}

/// POST /api-keys/bulk-revoke — revokes every selected key (ADR-0065's bulk-action pattern:
/// loop over the existing single-item `ApiKeysClient::revoke_api_key` rather than adding a new
/// bulk backend endpoint, since a handful of sequential revoke calls triggered by one admin
/// click is not a real performance concern at this scale). Best-effort per key, same as the
/// single-revoke handler above -- one key's revoke failing (e.g. already revoked) doesn't stop
/// the rest from being processed. Empty (nothing selected) is a legitimate no-op, not an error.
pub async fn post_bulk_revoke_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    for id in parse_ids(&body) {
        let _ = state
            .api_keys_client
            .revoke_api_key(session.tenant_id, session.role, id, &session.username)
            .await;
    }
    Redirect::to("/api-keys").into_response()
}
