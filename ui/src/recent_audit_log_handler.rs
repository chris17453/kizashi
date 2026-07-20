#[path = "recent_audit_log_handler_test.rs"]
#[cfg(test)]
mod recent_audit_log_handler_test;

use crate::audit_log_client::{AuditLogClient, AuditLogEntry};
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use std::sync::Arc;

const PAGE_SIZE: u32 = 50;

struct RecentAuditLogEntryView {
    service: &'static str,
    change_type: String,
    entity_type: String,
    actor: String,
    changed_at: DateTime<Utc>,
}

#[derive(serde::Deserialize)]
pub struct RecentAuditLogQuery {
    before: Option<DateTime<Utc>>,
}

#[derive(Template)]
#[template(path = "recent_audit_log.html")]
struct RecentAuditLogTemplate {
    show_nav: bool,
    entries: Vec<RecentAuditLogEntryView>,
    next_before: Option<DateTime<Utc>>,
    error: Option<String>,
}

/// GET /audit-log — a single, browsable, filterable-by-nothing-required activity feed merging
/// every tenant-scoped audit trail (config-admin-service's triggers/mappings/agents/analysis
/// config, retention-service's retention policies, auth-service's users/branding), most-recent
/// first (ADR-0045). Unlike `/audit-log/:service/:entity_id`, this needs no prior knowledge of
/// which entity to inspect -- the baseline enterprise-compliance expectation of "show me every
/// admin action recently," not just "show me this one record's history."
pub async fn get_recent_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RecentAuditLogQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let sources: [(&'static str, &Arc<dyn AuditLogClient>); 3] = [
        ("config-admin-service", &state.config_audit_log_client),
        ("retention-service", &state.retention_audit_log_client),
        ("auth-service", &state.auth_audit_log_client),
    ];

    let mut merged: Vec<(&'static str, AuditLogEntry)> = Vec::new();
    let mut errors = Vec::new();
    for (label, client) in sources {
        match client.list_recent(session.tenant_id, PAGE_SIZE, query.before).await {
            Ok(entries) => merged.extend(entries.into_iter().map(|e| (label, e))),
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
    merged.sort_by_key(|(_, e)| std::cmp::Reverse(e.changed_at));
    merged.truncate(PAGE_SIZE as usize);

    let next_before = merged.last().map(|(_, e)| e.changed_at);
    let entries = merged
        .into_iter()
        .map(|(service, e)| RecentAuditLogEntryView {
            service,
            change_type: e.change_type,
            entity_type: e.entity_type,
            actor: e.actor,
            changed_at: e.changed_at,
        })
        .collect();
    let error = if errors.is_empty() { None } else { Some(errors.join("; ")) };

    Html(RecentAuditLogTemplate { show_nav: true, entries, next_before, error }.render().unwrap())
        .into_response()
}
