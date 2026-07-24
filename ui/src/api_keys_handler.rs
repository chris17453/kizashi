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
    #[serde(default)]
    age: String,
}

/// Case-insensitive substring match on label -- same in-handler-filter shape as the Users page
/// search (ADR-0062): `ApiKeysClient::list_api_keys` has no search parameter, and a tenant's key
/// list is realistically small enough that filtering the already-fetched list is the right size
/// of fix, not a new backend query param.
fn key_age_band(key: &ApiKeySummary, now: chrono::DateTime<chrono::Utc>) -> &'static str {
    let age = now - key.created_at;
    if age < chrono::Duration::days(7) {
        "0_7"
    } else if age < chrono::Duration::days(31) {
        "8_30"
    } else if age < chrono::Duration::days(91) {
        "31_90"
    } else {
        "90_plus"
    }
}

fn matches_query(key: &ApiKeySummary, q: &str, age: &str) -> bool {
    (q.is_empty() || key.label.to_lowercase().contains(&q.to_lowercase()))
        && (age.is_empty() || key_age_band(key, chrono::Utc::now()) == age)
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
    age: String,
    active_count: usize,
    revoked_count: usize,
    recent_count: usize,
    age_metrics: Vec<ApiKeyAgeMetric>,
}

struct ApiKeyAgeMetric {
    key: String,
    label: String,
    count: usize,
    percent: usize,
}

fn key_posture(keys: &[ApiKeySummary]) -> (usize, usize, usize) {
    let active_count = keys.iter().filter(|key| key.revoked_at.is_none()).count();
    let revoked_count = keys.len().saturating_sub(active_count);
    let recent_cutoff = chrono::Utc::now() - chrono::Duration::days(30);
    let recent_count = keys.iter().filter(|key| key.created_at >= recent_cutoff).count();
    (active_count, revoked_count, recent_count)
}

fn key_age_metrics(keys: &[ApiKeySummary]) -> Vec<ApiKeyAgeMetric> {
    let now = chrono::Utc::now();
    let total = keys.len();
    [("0_7", "0–7 days"), ("8_30", "8–30 days"), ("31_90", "31–90 days"), ("90_plus", "90+ days")]
        .into_iter()
        .map(|(key, label)| {
            let count = keys.iter().filter(|item| key_age_band(item, now) == key).count();
            ApiKeyAgeMetric {
                key: key.to_string(),
                label: label.to_string(),
                count,
                percent: if total == 0 { 0 } else { count * 100 / total },
            }
        })
        .collect()
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
                keys.into_iter().filter(|k| matches_query(k, &query.q, &query.age)).collect();
            sort_rows(&mut keys, &query.sort, &query.dir);
            let (active_count, revoked_count, recent_count) = key_posture(&keys);
            let age_metrics = key_age_metrics(&keys);
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
                    age: query.age,
                    active_count,
                    revoked_count,
                    recent_count,
                    age_metrics,
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
                age: query.age,
                active_count: 0,
                revoked_count: 0,
                recent_count: 0,
                age_metrics: vec![],
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
            let (active_count, revoked_count, recent_count) = key_posture(&keys);
            let age_metrics = key_age_metrics(&keys);
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
                    age: String::new(),
                    active_count,
                    revoked_count,
                    recent_count,
                    age_metrics,
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let keys = state.api_keys_client.list_api_keys(session.tenant_id).await.unwrap_or_default();
    let (active_count, revoked_count, recent_count) = key_posture(&keys);
    let age_metrics = key_age_metrics(&keys);
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
            age: String::new(),
            active_count,
            revoked_count,
            recent_count,
            age_metrics,
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
