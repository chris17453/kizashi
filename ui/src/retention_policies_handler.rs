#[path = "retention_policies_handler_bulk_delete_test.rs"]
#[cfg(test)]
mod retention_policies_handler_bulk_delete_test;
#[path = "retention_policies_handler_mutations_test.rs"]
#[cfg(test)]
mod retention_policies_handler_mutations_test;
#[path = "retention_policies_handler_test.rs"]
#[cfg(test)]
mod retention_policies_handler_test;

use crate::retention_policies_client::{ComplianceHold, DataClass, RetentionPolicy};
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "retention_policies.html")]
struct RetentionPoliciesTemplate {
    show_nav: bool,
    is_admin: bool,
    policies: Vec<RetentionPolicy>,
    holds: Vec<ComplianceHold>,
    can_write: bool,
    error: Option<String>,
    form_error: Option<String>,
    notice: String,
    count: usize,
    data_class: String,
    enabled_count: usize,
    disabled_count: usize,
    active_hold_count: usize,
    configured_class_count: usize,
    shortest_ttl: i32,
    longest_ttl: i32,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct RetentionQuery {
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub data_class: String,
}

/// GET /retention-policies — spec §7's "data lifecycle UI": this entity has had a full CRUD +
/// RBAC-enforced API since ADR-0011, but zero Console UI presence until now.
pub async fn get_retention_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RetentionQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);
    let is_admin = session.role.at_least(common::Role::Admin);
    let selected_data_class = parse_data_class(query.data_class.trim()).ok();
    let holds = state
        .retention_policies_client
        .list_holds(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|hold| {
            selected_data_class.is_none() || Some(hold.data_class) == selected_data_class
        })
        .collect::<Vec<_>>();
    let active_hold_count = holds.iter().filter(|hold| hold.active).count();

    match state.retention_policies_client.list_policies(session.tenant_id).await {
        Ok(policies) => {
            let policies = policies
                .into_iter()
                .filter(|policy| {
                    selected_data_class.is_none() || Some(policy.data_class) == selected_data_class
                })
                .collect::<Vec<_>>();
            let enabled_count = policies.iter().filter(|policy| policy.enabled).count();
            let disabled_count = policies.len().saturating_sub(enabled_count);
            let configured_class_count = [DataClass::Raw, DataClass::Normalized, DataClass::Event]
                .iter()
                .filter(|data_class| {
                    policies.iter().any(|policy| policy.data_class == **data_class)
                })
                .count();
            let shortest_ttl = policies.iter().map(|policy| policy.ttl_days).min().unwrap_or(0);
            let longest_ttl = policies.iter().map(|policy| policy.ttl_days).max().unwrap_or(0);
            Html(
                RetentionPoliciesTemplate {
                    show_nav: true,
                    is_admin,
                    policies,
                    holds,
                    can_write,
                    error: None,
                    form_error: None,
                    notice: query.notice.clone(),
                    count: query.count,
                    data_class: query.data_class,
                    enabled_count,
                    disabled_count,
                    active_hold_count,
                    configured_class_count,
                    shortest_ttl,
                    longest_ttl,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            RetentionPoliciesTemplate {
                show_nav: true,
                is_admin,
                policies: vec![],
                holds,
                can_write,
                error: Some(e.to_string()),
                form_error: None,
                notice: query.notice,
                count: 0,
                data_class: query.data_class,
                enabled_count: 0,
                disabled_count: 0,
                active_hold_count,
                configured_class_count: 0,
                shortest_ttl: 0,
                longest_ttl: 0,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct PostPolicyForm {
    data_class: String,
    ttl_days: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ReimportForm {
    archive_key: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct HoldForm {
    data_class: String,
    reason: String,
}

pub async fn post_create_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<HoldForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let data_class = match parse_data_class(form.data_class.trim()) {
        Ok(data_class) => data_class,
        Err(_) => return Redirect::to("/retention-policies?notice=invalid-hold").into_response(),
    };
    if form.reason.trim().is_empty() {
        return Redirect::to("/retention-policies?notice=invalid-hold").into_response();
    }
    match state
        .retention_policies_client
        .create_hold(
            session.role,
            session.tenant_id,
            data_class,
            form.reason.trim(),
            &session.username,
        )
        .await
    {
        Ok(_) => Redirect::to("/retention-policies?notice=hold-created").into_response(),
        Err(_) => Redirect::to("/retention-policies?notice=hold-failed").into_response(),
    }
}

pub async fn post_release_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let _ = state
        .retention_policies_client
        .release_hold(session.role, session.tenant_id, id, &session.username)
        .await;
    Redirect::to("/retention-policies?notice=hold-released").into_response()
}

/// Reimport is explicitly tenant-scoped at the Console boundary. Archive keys are generated
/// as `archive/<tenant_id>/...`; rejecting every other prefix prevents an operator from using
/// the UI as a cross-tenant replay primitive even though retention-service's internal endpoint
/// is shared by the scheduler.
pub async fn post_reimport_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ReimportForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let archive_key = form.archive_key.trim();
    let tenant_prefix = format!("archive/{}/", session.tenant_id);
    if !archive_key.starts_with(&tenant_prefix) || archive_key.contains("..") {
        return Redirect::to("/retention-policies?notice=invalid-archive").into_response();
    }
    match state.retention_policies_client.reimport_archive(session.tenant_id, archive_key).await {
        Ok(summary) => Redirect::to(&format!(
            "/retention-policies?notice=reimported&count={}",
            summary.records_reimported
        ))
        .into_response(),
        Err(_) => Redirect::to("/retention-policies?notice=reimport-failed").into_response(),
    }
}

fn parse_data_class(raw: &str) -> Result<DataClass, &'static str> {
    match raw {
        "raw" => Ok(DataClass::Raw),
        "normalized" => Ok(DataClass::Normalized),
        "event" => Ok(DataClass::Event),
        _ => Err("data class must be one of raw, normalized, event"),
    }
}

pub async fn post_retention_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PostPolicyForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);
    let is_admin = session.role.at_least(common::Role::Admin);
    if !can_write {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let form_result = parse_data_class(form.data_class.trim()).and_then(|data_class| {
        form.ttl_days
            .trim()
            .parse::<i32>()
            .map(|ttl_days| (data_class, ttl_days))
            .map_err(|_| "TTL days must be a whole number")
    });

    let (data_class, ttl_days) = match form_result {
        Ok(parsed) => parsed,
        Err(msg) => {
            let policies = state
                .retention_policies_client
                .list_policies(session.tenant_id)
                .await
                .unwrap_or_default();
            return Html(
                RetentionPoliciesTemplate {
                    show_nav: true,
                    is_admin,
                    policies,
                    holds: vec![],
                    can_write,
                    error: None,
                    form_error: Some(msg.to_string()),
                    notice: String::new(),
                    count: 0,
                    data_class: String::new(),
                    enabled_count: 0,
                    disabled_count: 0,
                    active_hold_count: 0,
                    configured_class_count: 0,
                    shortest_ttl: 0,
                    longest_ttl: 0,
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let policy = RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id: session.tenant_id,
        data_class,
        ttl_days,
        enabled: true,
    };

    match state
        .retention_policies_client
        .create_policy(session.role, policy, &session.username)
        .await
    {
        Ok(_) => Redirect::to("/retention-policies").into_response(),
        Err(e) => {
            let policies = state
                .retention_policies_client
                .list_policies(session.tenant_id)
                .await
                .unwrap_or_default();
            Html(
                RetentionPoliciesTemplate {
                    show_nav: true,
                    is_admin,
                    policies,
                    holds: vec![],
                    can_write,
                    error: None,
                    form_error: Some(e.to_string()),
                    notice: String::new(),
                    count: 0,
                    data_class: String::new(),
                    enabled_count: 0,
                    disabled_count: 0,
                    active_hold_count: 0,
                    configured_class_count: 0,
                    shortest_ttl: 0,
                    longest_ttl: 0,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    }
}

/// POST /retention-policies/:id/toggle — flips a policy's enabled flag, mirroring
/// `AgentsClient`'s toggle convention: fetch the current list, find the matching row, flip
/// `enabled`, persist the whole record via `update_policy`.
pub async fn post_toggle_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    if let Ok(policies) = state.retention_policies_client.list_policies(session.tenant_id).await {
        if let Some(mut policy) = policies.into_iter().find(|p| p.id == id) {
            policy.enabled = !policy.enabled;
            let _ = state
                .retention_policies_client
                .update_policy(session.role, policy, &session.username)
                .await;
        }
    }
    Redirect::to("/retention-policies").into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct EditPolicyForm {
    ttl_days: String,
}

/// POST /retention-policies/:id/edit — updates a policy's TTL in place (HTML forms can't send
/// a real `PUT`, so this dedicated route stands in for it, same shape as the toggle route
/// above). `data_class`/`enabled` are preserved from the existing row — only `ttl_days`
/// changes here.
pub async fn post_edit_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<EditPolicyForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);
    let is_admin = session.role.at_least(common::Role::Admin);
    if !can_write {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let Ok(ttl_days) = form.ttl_days.trim().parse::<i32>() else {
        let policies = state
            .retention_policies_client
            .list_policies(session.tenant_id)
            .await
            .unwrap_or_default();
        return Html(
            RetentionPoliciesTemplate {
                show_nav: true,
                is_admin,
                policies,
                holds: vec![],
                can_write,
                error: None,
                form_error: Some("TTL days must be a whole number".to_string()),
                notice: String::new(),
                count: 0,
                data_class: String::new(),
                enabled_count: 0,
                disabled_count: 0,
                active_hold_count: 0,
                configured_class_count: 0,
                shortest_ttl: 0,
                longest_ttl: 0,
            }
            .render()
            .unwrap(),
        )
        .into_response();
    };

    if let Ok(policies) = state.retention_policies_client.list_policies(session.tenant_id).await {
        if let Some(mut policy) = policies.into_iter().find(|p| p.id == id) {
            policy.ttl_days = ttl_days;
            let _ = state
                .retention_policies_client
                .update_policy(session.role, policy, &session.username)
                .await;
        }
    }
    Redirect::to("/retention-policies").into_response()
}

/// `axum::extract::Form` deserializes via `serde_urlencoded`, which -- unlike some other form
/// crates -- does NOT collect repeated same-named fields (one checkbox per row, all named
/// `ids`) into a `Vec`; it only supports flat scalar struct fields. Parsing the raw body as a
/// flat list of `(key, value)` pairs instead and filtering for `"ids"` sidesteps that limitation
/// without adding a new dependency (`serde_urlencoded` is already a direct dependency). Same
/// pattern as API Keys' `post_bulk_revoke_api_keys`, Sensors' `post_bulk_delete_sensors`, and
/// Users' `post_bulk_delete_users` (ADR-0065/ADR-0095).
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

/// POST /retention-policies/bulk-delete — removes every selected policy (same bulk-action
/// pattern API Keys/Sensors/Users already have): loop over the existing single-item
/// `RetentionPoliciesClient::delete_policy` rather than a new bulk backend endpoint.
pub async fn post_bulk_delete_retention_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    for id in parse_ids(&body) {
        let _ = state
            .retention_policies_client
            .delete_policy(session.role, session.tenant_id, id, &session.username)
            .await;
    }
    Redirect::to("/retention-policies").into_response()
}

/// POST /retention-policies/:id/delete — mirrors `AgentsClient::delete_agent`'s convention.
pub async fn post_delete_retention_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let _ = state
        .retention_policies_client
        .delete_policy(session.role, session.tenant_id, id, &session.username)
        .await;
    Redirect::to("/retention-policies").into_response()
}
