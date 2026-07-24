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
    mfa_enrolled_count: usize,
    mfa_missing_count: usize,
    retention_policy_count: usize,
    retention_enabled_count: usize,
    egress_domain_count: usize,
    errors: Vec<String>,
    current_username: String,
    current_role: String,
    total_users: usize,
    posture_metrics: Vec<SecurityPostureMetric>,
    activity_bars: Vec<SecurityActivityBar>,
}

struct SecurityPostureMetric {
    label: String,
    count: usize,
    total: usize,
    percent: i32,
    href: String,
    tone: String,
}

struct SecurityActivityBar {
    date: String,
    count: usize,
    height_pct: i32,
    href: String,
}

/// Counts entries returned by `list_recent` across all three audit sources that fall within
/// the last 7 days -- an approximation, not an exact count (the underlying endpoints have no
/// dedicated count query, ADR-0045/0047), capped by `RECENT_ACTIVITY_LOOKBACK_LIMIT` per source.
/// Good enough for an at-a-glance dashboard tile; the Audit Log page itself is the source of
/// truth for exact history.
async fn recent_activity(
    state: &AppState,
    tenant_id: uuid::Uuid,
    errors: &mut Vec<String>,
) -> (usize, Vec<SecurityActivityBar>) {
    let cutoff = Utc::now() - Duration::days(7);
    let sources: [(&str, &std::sync::Arc<dyn AuditLogClient>); 3] = [
        ("config-admin-service", &state.config_audit_log_client),
        ("retention-service", &state.retention_audit_log_client),
        ("auth-service", &state.auth_audit_log_client),
    ];
    let mut entries = Vec::new();
    for (label, client) in sources {
        match client.list_recent(tenant_id, RECENT_ACTIVITY_LOOKBACK_LIMIT, None).await {
            Ok(source_entries) => {
                entries.extend(source_entries.into_iter().filter(|e| e.changed_at >= cutoff))
            }
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
    let mut daily = std::collections::BTreeMap::<String, usize>::new();
    for entry in &entries {
        *daily.entry(entry.changed_at.date_naive().to_string()).or_default() += 1;
    }
    let max = daily.values().copied().max().unwrap_or(1);
    let bars = daily
        .into_iter()
        .map(|(date, count)| SecurityActivityBar {
            href: format!("/audit-log?date={date}"),
            date,
            count,
            height_pct: ((count * 100) / max).max(8) as i32,
        })
        .collect();
    (entries.len(), bars)
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
    let (recent_activity_count, activity_bars) =
        recent_activity(&state, session.tenant_id, &mut errors).await;

    let (mut admin_count, mut operator_count, mut viewer_count) = (0, 0, 0);
    let (mut mfa_enrolled_count, mut mfa_missing_count) = (0, 0);
    match state.users_client.list_users(session.tenant_id, session.role).await {
        Ok(users) => {
            for user in users {
                if user.mfa_enabled {
                    mfa_enrolled_count += 1;
                } else {
                    mfa_missing_count += 1;
                }
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

    let total_users = admin_count + operator_count + viewer_count;
    let coverage =
        |label: &str, count: usize, total: usize, href: &str, tone: &str| SecurityPostureMetric {
            label: label.to_string(),
            count,
            total,
            percent: if total == 0 { 0 } else { (count * 100 / total) as i32 },
            href: href.to_string(),
            tone: tone.to_string(),
        };
    let posture_metrics = vec![
        coverage(
            "MFA enrolled",
            mfa_enrolled_count,
            total_users,
            "/users?mfa=missing",
            if mfa_missing_count > 0 { "risk" } else { "good" },
        ),
        coverage(
            "Retention enabled",
            retention_enabled_count,
            retention_policy_count,
            "/retention-policies",
            if retention_policy_count == 0 || retention_enabled_count < retention_policy_count {
                "risk"
            } else {
                "good"
            },
        ),
        coverage("Admins", admin_count, total_users, "/users", "neutral"),
        coverage("Operators", operator_count, total_users, "/users", "neutral"),
        coverage(
            "Egress controls",
            egress_domain_count,
            egress_domain_count.max(1),
            "/egress-allowlist",
            if egress_domain_count > 0 { "good" } else { "risk" },
        ),
    ];

    Html(
        SecurityOverviewTemplate {
            show_nav: true,
            is_admin,
            active_session_count,
            recent_activity_count,
            admin_count,
            operator_count,
            viewer_count,
            mfa_enrolled_count,
            mfa_missing_count,
            retention_policy_count,
            retention_enabled_count,
            egress_domain_count,
            errors,
            current_username: session.username,
            current_role: session.role.to_string(),
            total_users,
            posture_metrics,
            activity_bars,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
