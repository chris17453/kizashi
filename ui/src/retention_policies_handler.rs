#[path = "retention_policies_handler_test.rs"]
#[cfg(test)]
mod retention_policies_handler_test;

use crate::retention_policies_client::{DataClass, RetentionPolicy};
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "retention_policies.html")]
struct RetentionPoliciesTemplate {
    show_nav: bool,
    policies: Vec<RetentionPolicy>,
    can_write: bool,
    error: Option<String>,
    form_error: Option<String>,
}

/// GET /retention-policies — spec §7's "data lifecycle UI": this entity has had a full CRUD +
/// RBAC-enforced API since ADR-0011, but zero Console UI presence until now.
pub async fn get_retention_policies(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);

    match state.retention_policies_client.list_policies(session.tenant_id).await {
        Ok(policies) => Html(
            RetentionPoliciesTemplate {
                show_nav: true,
                policies,
                can_write,
                error: None,
                form_error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            RetentionPoliciesTemplate {
                show_nav: true,
                policies: vec![],
                can_write,
                error: Some(e.to_string()),
                form_error: None,
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
                    policies,
                    can_write,
                    error: None,
                    form_error: Some(msg.to_string()),
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

    match state.retention_policies_client.create_policy(session.role, policy).await {
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
                    policies,
                    can_write,
                    error: None,
                    form_error: Some(e.to_string()),
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
            let _ = state.retention_policies_client.update_policy(session.role, policy).await;
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
                policies,
                can_write,
                error: None,
                form_error: Some("TTL days must be a whole number".to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response();
    };

    if let Ok(policies) = state.retention_policies_client.list_policies(session.tenant_id).await {
        if let Some(mut policy) = policies.into_iter().find(|p| p.id == id) {
            policy.ttl_days = ttl_days;
            let _ = state.retention_policies_client.update_policy(session.role, policy).await;
        }
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

    let _ =
        state.retention_policies_client.delete_policy(session.role, session.tenant_id, id).await;
    Redirect::to("/retention-policies").into_response()
}
