#[path = "permissions_reference_handler_test.rs"]
#[cfg(test)]
mod permissions_reference_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

struct PermissionRow {
    area: String,
    href: String,
    viewer: String,
    operator: String,
    admin: String,
    note: Option<String>,
    viewer_state: String,
    operator_state: String,
    admin_state: String,
}

#[derive(Template)]
#[template(path = "permissions_reference.html")]
struct PermissionsReferenceTemplate {
    show_nav: bool,
    is_admin: bool,
    rows: Vec<PermissionRow>,
    viewer_allowed_count: usize,
    operator_allowed_count: usize,
    admin_allowed_count: usize,
    active_role: String,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct PermissionsQuery {
    #[serde(default)]
    role: String,
}

fn normalize_role(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "viewer" | "operator" | "admin" => value.trim().to_ascii_lowercase(),
        _ => "".to_string(),
    }
}

fn area_href(area: &str) -> String {
    match area {
        "Sensors / Connectors" => "/sensors",
        "Triggers" => "/triggers",
        "Field Mappings" => "/normalization-mappings",
        "Retention Policies" => "/retention-policies",
        "Egress Allowlist" => "/egress-allowlist",
        "AI Analysis Config" => "/analysis-config",
        "API Keys" => "/api-keys",
        "Users / RBAC" => "/users",
        "Audit Log" => "/audit-log",
        "Active Sessions" => "/security/sessions",
        "Login Attempts" => "/security/login-attempts",
        "Backups" => "/security/backups",
        "Compliance Report" => "/security/compliance-report",
        "Security Overview" => "/security",
        "Branding" => "/branding",
        "Saved Searches" => "/search",
        _ => "/security/permissions",
    }
    .to_string()
}

fn access_state(value: &str) -> String {
    if value == "No access" {
        "deny".to_string()
    } else if value == "View" || value.contains("Full read") {
        "read".to_string()
    } else {
        "write".to_string()
    }
}

fn row(area: &str, viewer: &str, operator: &str, admin: &str, note: Option<&str>) -> PermissionRow {
    PermissionRow {
        area: area.to_string(),
        href: area_href(area),
        viewer: viewer.to_string(),
        operator: operator.to_string(),
        admin: admin.to_string(),
        note: note.map(str::to_string),
        viewer_state: access_state(viewer),
        operator_state: access_state(operator),
        admin_state: access_state(admin),
    }
}

/// The reference table below is a direct transcription of what each backend service actually
/// enforces (`role.at_least(Role::Operator)` / `Role::Admin` checks in config-admin-service,
/// retention-service, egress-gateway, auth-service, ingestion-gateway), verified against the
/// code as of ADR-0048 -- not an aspirational or design-doc description. If a future change
/// alters what a role can do, this table must be updated in the same PR, the same discipline
/// CLAUDE.md §5 already requires for audit-log-writing config changes.
fn permission_rows() -> Vec<PermissionRow> {
    vec![
        row("Sensors / Connectors", "View", "View + create/edit/delete/toggle", "(same as Operator)", None),
        row("Triggers", "View", "View + create/edit", "(same as Operator)", None),
        row("Field Mappings", "View", "View + create/edit", "(same as Operator)", None),
        row("Retention Policies", "View", "View + create/edit/delete/toggle", "(same as Operator)", None),
        row("Egress Allowlist", "View", "View + replace", "(same as Operator)", None),
        row(
            "AI Analysis Config",
            "View",
            "View + update (provider, model, endpoint, API key)",
            "(same as Operator)",
            Some("The configured API key is never returned by the read endpoint, to any role (ADR-0048)."),
        ),
        row("API Keys", "View", "View + create/revoke", "(same as Operator)", None),
        row(
            "Users / RBAC",
            "No access",
            "No access",
            "View, create, change roles, delete",
            Some("The only area where even viewing requires Admin, not just writing. A tenant's sole remaining Admin can never be demoted or deleted."),
        ),
        row(
            "Audit Log",
            "Full read access",
            "(same as Viewer)",
            "(same as Viewer)",
            Some("No role restriction at all -- every authenticated tenant member can read the full audit trail, by design (it's a transparency control, not a privileged one)."),
        ),
        row(
            "Active Sessions",
            "No access",
            "No access",
            "View and revoke any session in the tenant",
            None,
        ),
        row(
            "Login Attempts",
            "No access",
            "No access",
            "View (success and failure history)",
            None,
        ),
        row(
            "Backups",
            "No access",
            "No access",
            "View run history",
            Some("Platform-wide (a whole-database backup), not scoped to one tenant."),
        ),
        row(
            "Compliance Report",
            "No access",
            "No access",
            "View (RBAC distribution, retention/egress/backup posture)",
            None,
        ),
        row(
            "Security Overview",
            "View",
            "View",
            "View",
            Some("Every role can view; sections that aggregate Admin-only data (like RBAC counts) simply show zero for a non-Admin caller rather than the real numbers."),
        ),
        row("Branding", "View", "View", "View + update (product name, logo, accent color)", None),
        row(
            "Saved Searches",
            "Full read/write access",
            "(same as Viewer)",
            "(same as Viewer)",
            Some("No role restriction -- a personal convenience feature, not an admin/config entity."),
        ),
    ]
}

/// GET /security/permissions — a written reference for what each of the three roles
/// (Viewer/Operator/Admin, ADR-0016) can actually do, transcribed directly from each backend
/// service's own enforcement code (ADR-0048). Auditors and new admins otherwise have no way to
/// answer "what does an Operator have access to" without reading source code area by area.
pub async fn get_permissions_reference(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PermissionsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let rows = permission_rows();
    let viewer_allowed_count = rows.iter().filter(|row| row.viewer_state != "deny").count();
    let operator_allowed_count = rows.iter().filter(|row| row.operator_state != "deny").count();
    let admin_allowed_count = rows.iter().filter(|row| row.admin_state != "deny").count();
    Html(
        PermissionsReferenceTemplate {
            show_nav: true,
            is_admin,
            rows,
            viewer_allowed_count,
            operator_allowed_count,
            admin_allowed_count,
            active_role: normalize_role(&query.role),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
