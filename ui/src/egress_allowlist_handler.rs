#[path = "egress_allowlist_handler_test.rs"]
#[cfg(test)]
mod egress_allowlist_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "egress_allowlist.html")]
struct EgressAllowlistTemplate {
    show_nav: bool,
    is_admin: bool,
    tenant_id: Uuid,
    domains: Vec<String>,
    can_write: bool,
    saved: bool,
    error: Option<String>,
}

fn parse_domains(raw: &str) -> Vec<String> {
    raw.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect()
}

/// GET /egress-allowlist — controls a tenant's egress domain allowlist (ADR-0021), the same
/// SSRF/exfiltration containment surface `egress-gateway` enforces on every outbound call from
/// a proxied connector/action. Full backend (`GET`/`PUT /v1/allowlist`, RBAC-enforced) existed
/// with zero Console UI presence until now.
pub async fn get_egress_allowlist(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    match state.egress_allowlist_client.get_allowlist(session.tenant_id).await {
        Ok(domains) => Html(
            EgressAllowlistTemplate {
                show_nav: true,
                is_admin,
                tenant_id: session.tenant_id,
                domains,
                can_write,
                saved: false,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            EgressAllowlistTemplate {
                show_nav: true,
                is_admin,
                tenant_id: session.tenant_id,
                domains: vec![],
                can_write,
                saved: false,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct PutAllowlistForm {
    #[serde(default)]
    domains: String,
}

/// POST /egress-allowlist — replaces the tenant's allowlist wholesale via `PUT
/// /v1/allowlist`, mirroring `AnalysisConfigClient`'s singleton-config UI pattern (one
/// resource per tenant, not row-based CRUD) since that's this backend's own shape. An empty
/// textarea is valid — `[]` means "no restriction configured," not an error.
pub async fn post_egress_allowlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PutAllowlistForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let domains = parse_domains(&form.domains);

    match state
        .egress_allowlist_client
        .put_allowlist(session.tenant_id, session.role, domains, &session.username)
        .await
    {
        Ok(domains) => Html(
            EgressAllowlistTemplate {
                show_nav: true,
                is_admin,
                tenant_id: session.tenant_id,
                domains,
                can_write: true,
                saved: true,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            EgressAllowlistTemplate {
                show_nav: true,
                is_admin,
                tenant_id: session.tenant_id,
                domains: parse_domains(&form.domains),
                can_write: true,
                saved: false,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
