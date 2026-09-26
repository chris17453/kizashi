#[path = "security_overview_handler_test.rs"]
#[cfg(test)]
mod security_overview_handler_test;

use crate::audit_log_client::AuditLogClient;
use crate::auth_client::TenantOidcProviderSummary;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use serde::Serialize;

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
    mfa_policy_required: Option<bool>,
    oidc_provider: Option<String>,
    oidc_available_providers: Vec<String>,
    tenant_oidc_configs: Vec<TenantOidcProviderSummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SecurityPostureMetric {
    label: String,
    count: usize,
    total: usize,
    percent: i32,
    href: String,
    tone: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SecurityActivityBar {
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

#[derive(Debug, Serialize)]
pub(crate) struct SecurityOverviewSummary {
    pub active_session_count: usize,
    pub recent_activity_count: usize,
    pub admin_count: usize,
    pub operator_count: usize,
    pub viewer_count: usize,
    pub mfa_enrolled_count: usize,
    pub mfa_missing_count: usize,
    pub retention_policy_count: usize,
    pub retention_enabled_count: usize,
    pub egress_domain_count: usize,
    pub total_users: usize,
    pub posture_metrics: Vec<SecurityPostureMetric>,
    pub activity_bars: Vec<SecurityActivityBar>,
    pub errors: Vec<String>,
}

pub(crate) async fn security_overview_for_tenant(
    state: &AppState,
    tenant_id: uuid::Uuid,
    role: common::Role,
) -> SecurityOverviewSummary {
    let mut errors = Vec::new();
    let active_session_count = state.session_store.list_for_tenant(tenant_id).await.len();
    let (recent_activity_count, activity_bars) =
        recent_activity(state, tenant_id, &mut errors).await;

    let (mut admin_count, mut operator_count, mut viewer_count) = (0, 0, 0);
    let (mut mfa_enrolled_count, mut mfa_missing_count) = (0, 0);
    match state.users_client.list_users(tenant_id, role).await {
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
    match state.retention_policies_client.list_policies(tenant_id).await {
        Ok(policies) => {
            retention_policy_count = policies.len();
            retention_enabled_count = policies.iter().filter(|p| p.enabled).count();
        }
        Err(e) => errors.push(format!("retention policies: {e}")),
    }

    let egress_domain_count = match state.egress_allowlist_client.get_allowlist(tenant_id).await {
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

    SecurityOverviewSummary {
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
        total_users,
        posture_metrics,
        activity_bars,
        errors,
    }
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
    let summary = security_overview_for_tenant(&state, session.tenant_id, session.role).await;
    let mfa_policy_required = if session.role.at_least(common::Role::Admin) {
        state.auth_client.get_mfa_policy(session.tenant_id, session.role).await.ok()
    } else {
        None
    };
    let (oidc_provider, oidc_available_providers) = if session.role.at_least(common::Role::Admin) {
        state
            .auth_client
            .get_oidc_provider_policy(session.tenant_id, session.role)
            .await
            .unwrap_or((None, Vec::new()))
    } else {
        (None, Vec::new())
    };
    let tenant_oidc_configs = if session.role.at_least(common::Role::Admin) {
        state
            .auth_client
            .list_tenant_oidc_providers(session.tenant_id, session.role)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Html(
        SecurityOverviewTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            active_session_count: summary.active_session_count,
            recent_activity_count: summary.recent_activity_count,
            admin_count: summary.admin_count,
            operator_count: summary.operator_count,
            viewer_count: summary.viewer_count,
            mfa_enrolled_count: summary.mfa_enrolled_count,
            mfa_missing_count: summary.mfa_missing_count,
            retention_policy_count: summary.retention_policy_count,
            retention_enabled_count: summary.retention_enabled_count,
            egress_domain_count: summary.egress_domain_count,
            errors: summary.errors,
            current_username: session.username,
            current_role: session.role.to_string(),
            total_users: summary.total_users,
            posture_metrics: summary.posture_metrics,
            activity_bars: summary.activity_bars,
            mfa_policy_required,
            oidc_provider,
            oidc_available_providers,
            tenant_oidc_configs,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct MfaPolicyForm {
    required: bool,
}

#[derive(serde::Deserialize)]
pub struct OidcProviderPolicyForm {
    provider: String,
}

#[derive(serde::Deserialize)]
pub struct TenantOidcConfigForm {
    provider: String,
    client_id: String,
    client_secret: String,
    auth_url: String,
    token_url: String,
    userinfo_url: String,
    redirect_url: String,
}

pub async fn post_mfa_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<MfaPolicyForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (axum::http::StatusCode::FORBIDDEN, "admin role required").into_response();
    }
    match state
        .auth_client
        .set_mfa_policy(session.tenant_id, session.role, form.required, &session.username)
        .await
    {
        Ok(_) => Redirect::to("/security?notice=mfa_policy_updated").into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn post_oidc_provider_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<OidcProviderPolicyForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (axum::http::StatusCode::FORBIDDEN, "admin role required").into_response();
    }
    let provider = (!form.provider.trim().is_empty()).then_some(form.provider.trim());
    match state
        .auth_client
        .set_oidc_provider_policy(session.tenant_id, session.role, provider, &session.username)
        .await
    {
        Ok(_) => Redirect::to("/security?notice=oidc_policy_updated").into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn post_tenant_oidc_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<TenantOidcConfigForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (axum::http::StatusCode::FORBIDDEN, "admin role required").into_response();
    }
    match state
        .auth_client
        .set_tenant_oidc_provider(
            session.tenant_id,
            session.role,
            &form.provider,
            &form.client_id,
            &form.client_secret,
            &form.auth_url,
            &form.token_url,
            &form.userinfo_url,
            &form.redirect_url,
            &session.username,
        )
        .await
    {
        Ok(_) => Redirect::to("/security?notice=oidc_config_updated").into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct DeleteTenantOidcConfigForm {
    provider: String,
}

pub async fn post_delete_tenant_oidc_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<DeleteTenantOidcConfigForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (axum::http::StatusCode::FORBIDDEN, "admin role required").into_response();
    }
    match state
        .auth_client
        .delete_tenant_oidc_provider(
            session.tenant_id,
            session.role,
            &form.provider,
            &session.username,
        )
        .await
    {
        Ok(_) => Redirect::to("/security?notice=oidc_config_deleted").into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}
