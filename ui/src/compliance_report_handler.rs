#[path = "compliance_report_handler_test.rs"]
#[cfg(test)]
mod compliance_report_handler_test;

use crate::audit_log_client::AuditLogClient;
use crate::session_guard::require_session;
use crate::users_client::PasswordPolicySummary;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
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
    enabled_connector_count: usize,
    stale_connector_count: usize,
    total_record_count: usize,
    normalized_record_count: usize,
    errors: Vec<String>,
    control_score: usize,
    control_total: usize,
    controls: Vec<ComplianceControlView>,
    state: String,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct ComplianceQuery {
    #[serde(default)]
    state: String,
}

fn normalize_control_state(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "ready" | "attention" | "unknown" => value.trim().to_ascii_lowercase(),
        _ => String::new(),
    }
}

struct ComplianceControlView {
    label: String,
    state: String,
    detail: String,
    href: String,
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
pub async fn get_compliance_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ComplianceQuery>,
) -> Response {
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

    let (enabled_connector_count, stale_connector_count) = {
        let (sensors, stats) = tokio::join!(
            state.sensors_client.list_sensors(session.tenant_id, 1000, 0),
            state.stats_client.connector_stats(session.tenant_id),
        );
        match (sensors, stats) {
            (Ok(page), Ok(stats)) => {
                let now = Utc::now();
                let enabled =
                    page.sensors.iter().filter(|sensor| sensor.enabled).collect::<Vec<_>>();
                let stale = enabled
                    .iter()
                    .filter(|sensor| {
                        stats
                            .iter()
                            .find(|stat| stat.connector_id == sensor.name)
                            .map(|stat| now - stat.last_ingested_at > Duration::hours(1))
                            .unwrap_or(true)
                    })
                    .count();
                (enabled.len(), stale)
            }
            (sensors, stats) => {
                if let Err(error) = sensors {
                    errors.push(format!("connectors: {error}"));
                }
                if let Err(error) = stats {
                    errors.push(format!("connector stats: {error}"));
                }
                (0, 0)
            }
        }
    };

    let (total_record_count, normalized_record_count) = {
        let mappings = state.normalization_mappings_client.list_mappings(session.tenant_id).await;
        let records = state
            .stats_client
            .search_records(
                session.tenant_id,
                &crate::ingestion_stats_client::RecordSearchFilter {
                    limit: 1000,
                    ..Default::default()
                },
            )
            .await;
        match (mappings, records) {
            (Ok(_mappings), Ok(result)) => {
                let total = result.records.len();
                let normalized =
                    result.records.iter().filter(|record| record.is_normalized()).count();
                (total, normalized)
            }
            (mappings, records) => {
                if let Err(error) = mappings {
                    errors.push(format!("normalization mappings: {error}"));
                }
                if let Err(error) = records {
                    errors.push(format!("normalization records: {error}"));
                }
                (0, 0)
            }
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

    let mut controls = vec![];
    let mfa_state = if total_user_count == 0 {
        "unknown"
    } else if mfa_enabled_count == total_user_count {
        "ready"
    } else {
        "attention"
    };
    controls.push(ComplianceControlView {
        label: "MFA adoption".to_string(),
        state: mfa_state.to_string(),
        detail: format!("{mfa_enabled_count} of {total_user_count} accounts protected"),
        href: "/users".to_string(),
    });
    controls.push(ComplianceControlView {
        label: "Password policy".to_string(),
        state: if password_policy.is_some() { "ready" } else { "unknown" }.to_string(),
        detail: if password_policy.is_some() { "Policy loaded" } else { "Unavailable" }.to_string(),
        href: "/security/password".to_string(),
    });
    controls.push(ComplianceControlView {
        label: "Retention enforcement".to_string(),
        state: if retention_enabled_count > 0 { "ready" } else { "attention" }.to_string(),
        detail: format!("{retention_enabled_count} of {retention_policy_count} policies enabled"),
        href: "/retention-policies".to_string(),
    });
    controls.push(ComplianceControlView {
        label: "Egress boundary".to_string(),
        state: if egress_domain_count > 0 { "ready" } else { "attention" }.to_string(),
        detail: format!("{egress_domain_count} allowed domain entries"),
        href: "/egress-allowlist".to_string(),
    });
    let connector_state = if enabled_connector_count == 0 {
        "unknown"
    } else if stale_connector_count == 0 {
        "ready"
    } else {
        "attention"
    };
    controls.push(ComplianceControlView {
        label: "Connector freshness".to_string(),
        state: connector_state.to_string(),
        detail: format!(
            "{enabled_connector_count} enabled · {stale_connector_count} stale or silent"
        ),
        href: "/sensors?health=stale".to_string(),
    });
    let normalization_state = if total_record_count == 0 {
        "unknown"
    } else if normalized_record_count == total_record_count {
        "ready"
    } else {
        "attention"
    };
    controls.push(ComplianceControlView {
        label: "Normalization completeness".to_string(),
        state: normalization_state.to_string(),
        detail: format!("{normalized_record_count} of {total_record_count} records normalized"),
        href: "/normalization-mappings?coverage=pending".to_string(),
    });
    let backup_ready = last_backup_status.as_deref() == Some("success");
    controls.push(ComplianceControlView {
        label: "Backup recovery".to_string(),
        state: if backup_ready { "ready" } else { "attention" }.to_string(),
        detail: last_backup_status.clone().unwrap_or_else(|| "No run recorded".to_string()),
        href: "/security/backups".to_string(),
    });
    controls.push(ComplianceControlView {
        label: "Login anomaly signal".to_string(),
        state: if failed_login_count_7d == 0 { "ready" } else { "attention" }.to_string(),
        detail: format!("{failed_login_count_7d} failed attempts in 7 days"),
        href: "/security/login-attempts".to_string(),
    });
    let control_total = controls.len();
    let control_score = controls.iter().filter(|control| control.state == "ready").count();
    let state = normalize_control_state(&query.state);
    let controls = if state.is_empty() {
        controls
    } else {
        controls.into_iter().filter(|control| control.state == state).collect()
    };

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
            enabled_connector_count,
            stale_connector_count,
            total_record_count,
            normalized_record_count,
            errors,
            control_score,
            control_total,
            controls,
            state,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
