#[path = "audit_log_handler_test.rs"]
#[cfg(test)]
mod audit_log_handler_test;

use crate::audit_log_client::AuditLogEntry;
use crate::ontology_client;
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

struct AuditChangeRow {
    label: String,
    count: usize,
    percentage: usize,
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

fn audit_posture(
    entries: &[AuditLogEntryView],
) -> (usize, usize, Option<DateTime<Utc>>, Vec<AuditChangeRow>) {
    let actors =
        entries.iter().map(|entry| entry.actor.as_str()).collect::<std::collections::HashSet<_>>();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for entry in entries {
        *counts.entry(entry.change_type.clone()).or_default() += 1;
    }
    let max = counts.values().copied().max().unwrap_or(0);
    let rows: Vec<AuditChangeRow> = counts
        .into_iter()
        .map(|(label, count)| AuditChangeRow {
            label,
            count,
            percentage: if max == 0 { 0 } else { (count * 100 / max).max(8) },
        })
        .collect();
    (actors.len(), rows.len(), entries.first().map(|entry| entry.changed_at), rows)
}

#[derive(Template)]
#[template(path = "audit_log.html")]
struct AuditLogTemplate {
    show_nav: bool,
    is_admin: bool,
    service: String,
    entity_id: Uuid,
    entries: Vec<AuditLogEntryView>,
    entry_count: usize,
    actor_count: usize,
    mutation_count: usize,
    latest_change_at: Option<DateTime<Utc>>,
    change_posture: Vec<AuditChangeRow>,
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
        "incident" => {
            return match state
                .incidents_client
                .list_audit_log_for_entity(session.tenant_id, entity_id)
                .await
            {
                Ok(entries) => Html(
                    {
                        let entries = entries.into_iter().map(to_view).collect::<Vec<_>>();
                        let (actor_count, mutation_count, latest_change_at, change_posture) =
                            audit_posture(&entries);
                        AuditLogTemplate {
                            show_nav: true,
                            is_admin,
                            service,
                            entity_id,
                            entry_count: entries.len(),
                            actor_count,
                            mutation_count,
                            latest_change_at,
                            change_posture,
                            entries,
                            error: None,
                        }
                    }
                    .render()
                    .unwrap(),
                )
                .into_response(),
                Err(e) => Html(
                    AuditLogTemplate {
                        show_nav: true,
                        is_admin,
                        service,
                        entity_id,
                        entries: vec![],
                        entry_count: 0,
                        actor_count: 0,
                        mutation_count: 0,
                        latest_change_at: None,
                        change_posture: vec![],
                        error: Some(e.to_string()),
                    }
                    .render()
                    .unwrap(),
                )
                .into_response(),
            };
        }
        "ontology" => {
            let Some(client) = ontology_client::global() else {
                return Html(
                    AuditLogTemplate {
                        show_nav: true,
                        is_admin,
                        service,
                        entity_id,
                        entries: vec![],
                        entry_count: 0,
                        actor_count: 0,
                        mutation_count: 0,
                        latest_change_at: None,
                        change_posture: vec![],
                        error: Some("ontology audit service is unavailable".to_string()),
                    }
                    .render()
                    .unwrap(),
                )
                .into_response();
            };
            let result = client.list_action_invocations(&session.bearer_token).await;
            return match result {
                Ok(invocations) => {
                    let entries: Vec<AuditLogEntryView> = invocations
                        .into_iter()
                        .filter(|invocation| {
                            invocation.target_object_ids.as_array().into_iter().flatten().any(
                                |target| {
                                    target.as_str().and_then(|value| value.parse::<Uuid>().ok())
                                        == Some(entity_id)
                                },
                            )
                        })
                        .map(|invocation| AuditLogEntryView {
                            change_type: "invoked".to_string(),
                            actor: invocation
                                .triggering_event_ref
                                .get("actor")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("system")
                                .to_string(),
                            before: None,
                            after: pretty_json(&serde_json::json!({
                                "action_type_id": invocation.action_type_id,
                                "target_object_ids": invocation.target_object_ids,
                                "parameters": invocation.parameters,
                                "outcome": invocation.outcome,
                                "triggering_event_ref": invocation.triggering_event_ref,
                            })),
                            changed_at: invocation.executed_at,
                        })
                        .collect();
                    let (actor_count, mutation_count, latest_change_at, change_posture) =
                        audit_posture(&entries);
                    Html(
                        AuditLogTemplate {
                            show_nav: true,
                            is_admin,
                            service,
                            entity_id,
                            entry_count: entries.len(),
                            actor_count,
                            mutation_count,
                            latest_change_at,
                            change_posture,
                            entries,
                            error: None,
                        }
                        .render()
                        .unwrap(),
                    )
                    .into_response()
                }
                Err(error) => Html(
                    AuditLogTemplate {
                        show_nav: true,
                        is_admin,
                        service,
                        entity_id,
                        entries: vec![],
                        entry_count: 0,
                        actor_count: 0,
                        mutation_count: 0,
                        latest_change_at: None,
                        change_posture: vec![],
                        error: Some(error.to_string()),
                    }
                    .render()
                    .unwrap(),
                )
                .into_response(),
            };
        }
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
                    entry_count: 0,
                    actor_count: 0,
                    mutation_count: 0,
                    latest_change_at: None,
                    change_posture: vec![],
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
            let entries: Vec<AuditLogEntryView> = entries.into_iter().map(to_view).collect();
            let (actor_count, mutation_count, latest_change_at, change_posture) =
                audit_posture(&entries);
            Html(
                AuditLogTemplate {
                    show_nav: true,
                    is_admin,
                    service,
                    entity_id,
                    entry_count: entries.len(),
                    actor_count,
                    mutation_count,
                    latest_change_at,
                    change_posture,
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
                entry_count: 0,
                actor_count: 0,
                mutation_count: 0,
                latest_change_at: None,
                change_posture: vec![],
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
