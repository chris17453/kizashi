#[path = "attention_summary_handler_test.rs"]
#[cfg(test)]
mod attention_summary_handler_test;

use crate::backlog_client::QueueDepthSummary;
use crate::incidents_client::IncidentDetail;
use crate::ontology_client;
use crate::session_guard::require_session;
use crate::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use common::ontology::ActionInvocation;
use common::{IncidentSeverity, IncidentStatus};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AttentionSummary {
    pub open_incidents: usize,
    pub critical_incidents: usize,
    pub unassigned_incidents: usize,
    pub review_actions: usize,
    pub critical_queues: usize,
    pub sla_breaches: usize,
    pub stale_connectors: usize,
    pub attention_count: usize,
}

fn incident_sla_breached(incident: &common::Incident, now: chrono::DateTime<chrono::Utc>) -> bool {
    let target = match incident.severity {
        IncidentSeverity::Critical => chrono::Duration::minutes(15),
        IncidentSeverity::High => chrono::Duration::hours(1),
        IncidentSeverity::Medium => chrono::Duration::hours(4),
        IncidentSeverity::Low => chrono::Duration::hours(24),
    };
    let end = incident.resolved_at.unwrap_or(now);
    end.signed_duration_since(incident.created_at) > target
}

pub(crate) fn unique_attention_case_count(
    incidents: &[IncidentDetail],
    now: chrono::DateTime<chrono::Utc>,
) -> usize {
    incidents
        .iter()
        .filter(|item| item.incident.status != IncidentStatus::Resolved)
        .filter(|item| {
            item.incident.severity == IncidentSeverity::Critical
                || item.incident.assigned_to.is_none()
                || incident_sla_breached(&item.incident, now)
        })
        .map(|item| item.incident.id)
        .collect::<std::collections::HashSet<_>>()
        .len()
}

fn build_attention_summary(
    incidents: &[IncidentDetail],
    actions: &[ActionInvocation],
    queues: &[QueueDepthSummary],
    stale_connectors: usize,
) -> AttentionSummary {
    let active = incidents.iter().filter(|item| item.incident.status != IncidentStatus::Resolved);
    let active_items = active.collect::<Vec<_>>();
    let critical_incidents = active_items
        .iter()
        .filter(|item| item.incident.severity == IncidentSeverity::Critical)
        .count();
    let unassigned_incidents =
        active_items.iter().filter(|item| item.incident.assigned_to.is_none()).count();
    let review_actions =
        actions.iter().filter(|item| !item.outcome.eq_ignore_ascii_case("completed")).count();
    let critical_queues = queues
        .iter()
        .filter(|item| crate::topology::severity_for(item.messages) == "critical")
        .count();
    let sla_breaches = active_items
        .iter()
        .filter(|item| incident_sla_breached(&item.incident, chrono::Utc::now()))
        .count();
    AttentionSummary {
        open_incidents: active_items.len(),
        critical_incidents,
        unassigned_incidents,
        review_actions,
        critical_queues,
        sla_breaches,
        stale_connectors,
        attention_count: unique_attention_case_count(incidents, chrono::Utc::now())
            + review_actions
            + critical_queues
            + stale_connectors,
    }
}

pub async fn get_attention_summary(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let action_future = async {
        match ontology_client::global() {
            Some(client) => {
                client.list_action_invocations(&session.bearer_token).await.unwrap_or_default()
            }
            None => Vec::new(),
        }
    };
    let (incidents, queues, actions, sensors, connector_stats) = tokio::join!(
        state.incidents_client.list_incidents(session.tenant_id, None),
        state.backlog_client.queue_depths(),
        action_future,
        state.sensors_client.list_sensors(session.tenant_id, 1000, 0),
        state.stats_client.connector_stats(session.tenant_id),
    );
    let now = chrono::Utc::now();
    let stale_connectors = match (sensors, connector_stats) {
        (Ok(page), Ok(stats)) => page
            .sensors
            .iter()
            .filter(|sensor| sensor.enabled)
            .filter(|sensor| {
                stats
                    .iter()
                    .find(|stat| stat.connector_id == sensor.name)
                    .map(|stat| now - stat.last_ingested_at > chrono::Duration::hours(1))
                    .unwrap_or(true)
            })
            .count(),
        _ => 0,
    };
    let summary = build_attention_summary(
        &incidents.unwrap_or_default(),
        &actions,
        &queues.unwrap_or_default(),
        stale_connectors,
    );
    axum::Json(summary).into_response()
}
