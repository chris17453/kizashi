#[path = "security_overview_handler_test.rs"]
#[cfg(test)]
mod security_overview_handler_test;

use crate::audit_log_client::AuditLogClient;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{Duration, Utc};

const RECENT_ACTIVITY_LOOKBACK_LIMIT: u32 = 200;

#[derive(Template)]
#[template(path = "security_overview.html")]
struct SecurityOverviewTemplate {
    show_nav: bool,
    is_admin: bool,
    active_session_count: usize,
    recent_activity_count: usize,
    admin_count: usize,
    operator_count: usize,
    viewer_count: usize,
    retention_policy_count: usize,
    retention_enabled_count: usize,
    egress_domain_count: usize,
    errors: Vec<String>,
}

/// Counts entries returned by `list_recent` across all three audit sources that fall within
/// the last 7 days -- an approximation, not an exact count (the underlying endpoints have no
/// dedicated count query, ADR-0045/0047), capped by `RECENT_ACTIVITY_LOOKBACK_LIMIT` per source.
/// Good enough for an at-a-glance dashboard tile; the Audit Log page itself is the source of
/// truth for exact history.
async fn recent_activity_count(
    state: &AppState,
    tenant_id: uuid::Uuid,
    errors: &mut Vec<String>,
) -> usize {
    let cutoff = Utc::now() - Duration::days(7);
    let sources: [(&str, &std::sync::Arc<dyn AuditLogClient>); 3] = [
        ("config-admin-service", &state.config_audit_log_client),
        ("retention-service", &state.retention_audit_log_client),
        ("auth-service", &state.auth_audit_log_client),
    ];
    let mut count = 0;
    for (label, client) in sources {
        match client.list_recent(tenant_id, RECENT_ACTIVITY_LOOKBACK_LIMIT, None).await {
            Ok(entries) => count += entries.iter().filter(|e| e.changed_at >= cutoff).count(),
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
    count
}

/// GET /security — a single-pane-of-glass compliance dashboard (ADR-0047): active sessions,
/// recent admin activity, RBAC distribution, retention policy coverage, and egress allowlist
/// size, each linking out to its own detail page. Aggregates data every one of those pages
/// already exposes individually -- this closes the "where do I start" gap for an auditor or new
/// admin who doesn't yet know which of the five separate Security & Compliance pages to check
/// first.
pub async fn get_security_overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let mut errors = Vec::new();

    let active_session_count = state.session_store.list_for_tenant(session.tenant_id).await.len();
    let recent_activity_count = recent_activity_count(&state, session.tenant_id, &mut errors).await;

    let (mut admin_count, mut operator_count, mut viewer_count) = (0, 0, 0);
    match state.users_client.list_users(session.tenant_id, session.role).await {
        Ok(users) => {
            for user in users {
                match user.role {
                    common::Role::Admin => admin_count += 1,
                    common::Role::Operator => operator_count += 1,
                    common::Role::Viewer => viewer_count += 1,
                }
            }
        }
        Err(e) => errors.push(format!("users: {e}")),
    }

    let (mut retention_policy_count, mut retention_enabled_count) = (0, 0);
    match state.retention_policies_client.list_policies(session.tenant_id).await {
        Ok(policies) => {
            retention_policy_count = policies.len();
            retention_enabled_count = policies.iter().filter(|p| p.enabled).count();
        }
        Err(e) => errors.push(format!("retention policies: {e}")),
    }

    let egress_domain_count =
        match state.egress_allowlist_client.get_allowlist(session.tenant_id).await {
            Ok(domains) => domains.len(),
            Err(e) => {
                errors.push(format!("egress allowlist: {e}"));
                0
            }
        };

    Html(
        SecurityOverviewTemplate {
            show_nav: true,
            is_admin,
            active_session_count,
            recent_activity_count,
            admin_count,
            operator_count,
            viewer_count,
            retention_policy_count,
            retention_enabled_count,
            egress_domain_count,
            errors,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
