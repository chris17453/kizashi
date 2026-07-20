#[path = "compliance_report_handler_test.rs"]
#[cfg(test)]
mod compliance_report_handler_test;

use crate::audit_log_client::AuditLogClient;
use crate::session_guard::require_session;
use crate::users_client::PasswordPolicySummary;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use chrono::{Duration, Utc};
use common::Role;

const RECENT_ACTIVITY_LOOKBACK_LIMIT: u32 = 200;
const BACKUP_HISTORY_LOOKBACK: usize = 20;

#[derive(Template)]
#[template(path = "compliance_report.html")]
struct ComplianceReportTemplate {
    show_nav: bool,
    is_admin: bool,
    generated_at: String,
    admin_count: usize,
    operator_count: usize,
    viewer_count: usize,
    mfa_enabled_count: usize,
    total_user_count: usize,
    password_policy: Option<PasswordPolicySummary>,
    retention_policy_count: usize,
    retention_enabled_count: usize,
    egress_domain_count: usize,
    recent_admin_activity_count: usize,
    failed_login_count_7d: usize,
    last_backup_status: Option<String>,
    last_backup_at: Option<String>,
    recent_backup_failure_count: usize,
    errors: Vec<String>,
}

async fn require_admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::Session, Response> {
    let session = require_session(state.session_store.as_ref(), headers).await?;
    if !session.role.at_least(Role::Admin) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(session)
}

async fn recent_admin_activity_count(
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

/// GET /security/compliance-report — a single downloadable (browser-printable to PDF) snapshot
/// covering the controls this session's compliance rubric has been closing one-by-one (ADR-0056):
/// RBAC distribution and MFA adoption, the live password policy, retention coverage, egress
/// allowlist size, recent admin activity, failed-login volume, and backup/DR status. Reuses the
/// exact same clients Security Overview (ADR-0047) already calls plus the two ADR-0053/0055
/// clients never folded into a dashboard before now — no new data-gathering, just assembled into
/// one auditor-facing document instead of five separate pages. `Admin`-only, since it aggregates
/// the same admin-gated data (login attempts, backup status) those pages already restrict.
pub async fn get_compliance_report(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let mut errors = Vec::new();

    let (mut admin_count, mut operator_count, mut viewer_count, mut mfa_enabled_count) =
        (0, 0, 0, 0);
    let mut total_user_count = 0;
    match state.users_client.list_users(session.tenant_id, session.role).await {
        Ok(users) => {
            total_user_count = users.len();
            for user in users {
                match user.role {
                    Role::Admin => admin_count += 1,
                    Role::Operator => operator_count += 1,
                    Role::Viewer => viewer_count += 1,
                }
                if user.mfa_enabled {
                    mfa_enabled_count += 1;
                }
            }
        }
        Err(e) => errors.push(format!("users: {e}")),
    }

    let password_policy = match state.users_client.password_policy().await {
        Ok(policy) => Some(policy),
        Err(e) => {
            errors.push(format!("password policy: {e}"));
            None
        }
    };

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

    let recent_admin_activity_count =
        recent_admin_activity_count(&state, session.tenant_id, &mut errors).await;

    let failed_login_count_7d = match state
        .login_attempts_client
        .list_recent(session.tenant_id, session.role, None)
        .await
    {
        Ok(attempts) => {
            let cutoff = Utc::now() - Duration::days(7);
            attempts.iter().filter(|a| !a.success && a.attempted_at >= cutoff).count()
        }
        Err(e) => {
            errors.push(format!("login attempts: {e}"));
            0
        }
    };

    let (mut last_backup_status, mut last_backup_at, mut recent_backup_failure_count) =
        (None, None, 0);
    match state.backup_status_client.list_recent(session.role, None).await {
        Ok(runs) => {
            if let Some(latest) = runs.first() {
                last_backup_status = Some(latest.status.clone());
                last_backup_at = Some(latest.started_at.to_rfc3339());
            }
            recent_backup_failure_count =
                runs.iter().take(BACKUP_HISTORY_LOOKBACK).filter(|r| r.status == "failed").count();
        }
        Err(e) => errors.push(format!("backup status: {e}")),
    }

    Html(
        ComplianceReportTemplate {
            show_nav: true,
            is_admin,
            generated_at: Utc::now().to_rfc3339(),
            admin_count,
            operator_count,
            viewer_count,
            mfa_enabled_count,
            total_user_count,
            password_policy,
            retention_policy_count,
            retention_enabled_count,
            egress_domain_count,
            recent_admin_activity_count,
            failed_login_count_7d,
            last_backup_status,
            last_backup_at,
            recent_backup_failure_count,
            errors,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
