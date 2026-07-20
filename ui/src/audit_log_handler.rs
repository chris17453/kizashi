#[path = "audit_log_handler_test.rs"]
#[cfg(test)]
mod audit_log_handler_test;

use crate::audit_log_client::AuditLogEntry;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Askama templates can't call arbitrary Rust functions, so `before`/`after` are pretty-printed
/// to strings here rather than in the template.
struct AuditLogEntryView {
    change_type: String,
    actor: String,
    before: Option<String>,
    after: String,
    changed_at: DateTime<Utc>,
}

fn to_view(entry: AuditLogEntry) -> AuditLogEntryView {
    AuditLogEntryView {
        change_type: entry.change_type,
        actor: entry.actor,
        before: entry.before.map(|v| pretty_json(&v)),
        after: pretty_json(&entry.after),
        changed_at: entry.changed_at,
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[derive(Template)]
#[template(path = "audit_log.html")]
struct AuditLogTemplate {
    show_nav: bool,
    is_admin: bool,
    service: String,
    entity_id: Uuid,
    entries: Vec<AuditLogEntryView>,
    error: Option<String>,
}

/// GET /audit-log/:service/:entity_id — the immutable audit trail CLAUDE.md §5 requires for
/// every admin/config mutation. `:service` selects which backend owns the entity (`config`:
/// triggers/mappings/agents/analysis-config, `retention`: retention policies, `auth`: users/
/// tenant branding, `ingestion`: API keys, or `egress`: the tenant egress allowlist) — every
/// write page already writes to this trail
/// via `record_audit_entry`, but until now
/// nothing in the Console UI could read it back.
pub async fn get_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((service, entity_id)): Path<(String, Uuid)>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let client = match service.as_str() {
        "config" => &state.config_audit_log_client,
        "retention" => &state.retention_audit_log_client,
        "auth" => &state.auth_audit_log_client,
        "ingestion" => &state.ingestion_audit_log_client,
        "egress" => &state.egress_audit_log_client,
        _ => {
            return Html(
                AuditLogTemplate {
                    show_nav: true,
                    is_admin,
                    service,
                    entity_id,
                    entries: vec![],
                    error: Some("unknown audit log service".to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    match client.list_for_entity(session.tenant_id, entity_id).await {
        Ok(entries) => {
            let entries = entries.into_iter().map(to_view).collect();
            Html(
                AuditLogTemplate {
                    show_nav: true,
                    is_admin,
                    service,
                    entity_id,
                    entries,
                    error: None,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            AuditLogTemplate {
                show_nav: true,
                is_admin,
                service,
                entity_id,
                entries: vec![],
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
