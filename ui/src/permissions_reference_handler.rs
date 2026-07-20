#[path = "permissions_reference_handler_test.rs"]
#[cfg(test)]
mod permissions_reference_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

struct PermissionRow {
    area: String,
    viewer: String,
    operator: String,
    admin: String,
    note: Option<String>,
}

#[derive(Template)]
#[template(path = "permissions_reference.html")]
struct PermissionsReferenceTemplate {
    show_nav: bool,
    rows: Vec<PermissionRow>,
}

fn row(area: &str, viewer: &str, operator: &str, admin: &str, note: Option<&str>) -> PermissionRow {
    PermissionRow {
        area: area.to_string(),
        viewer: viewer.to_string(),
        operator: operator.to_string(),
        admin: admin.to_string(),
        note: note.map(str::to_string),
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
) -> Response {
    if let Err(response) = require_session(state.session_store.as_ref(), &headers).await {
        return response;
    }

    Html(PermissionsReferenceTemplate { show_nav: true, rows: permission_rows() }.render().unwrap())
        .into_response()
}
