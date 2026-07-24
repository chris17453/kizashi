#[path = "pipeline_handler_test.rs"]
#[cfg(test)]
mod pipeline_handler_test;

use crate::session_guard::require_session;
use crate::topology::{build_topology_items, TopologyItem};
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "pipeline.html")]
struct PipelineTemplate {
    show_nav: bool,
    is_admin: bool,
    items: Vec<TopologyItem>,
    connector_count: usize,
    record_count: i64,
    event_count: usize,
    open_incident_count: usize,
    trigger_count: usize,
    error: Option<String>,
    stages: Vec<StageDiagnostic>,
    queues: Vec<QueueDiagnostic>,
    queue_total: u64,
    critical_queue_count: usize,
    max_queue: u64,
    backlog_error: Option<String>,
    severity: String,
}

struct StageDiagnostic {
    label: String,
    service: String,
    status: String,
    href: String,
}
struct QueueDiagnostic {
    label: String,
    queue_name: String,
    messages: u64,
    severity: String,
    severity_label: String,
    href: String,
    pressure_pct: usize,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct PipelineQuery {
    #[serde(default)]
    severity: String,
}

fn normalize_pressure_scope(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" | "warn" | "ok" => value.trim().to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub async fn get_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PipelineQuery>,
) -> Response {
    let session = require_session(state.session_store.as_ref(), &headers).await;
    let session = match session {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let (sensors, stats, events, incidents, triggers) = tokio::join!(
        state.sensors_client.list_sensors(session.tenant_id, 1000, 0),
        state.stats_client.connector_stats(session.tenant_id),
        state.events_client.list_events(&session.bearer_token, 1000, 0, None, None),
        state.incidents_client.list_incidents(session.tenant_id, None),
        state.triggers_client.list_triggers(session.tenant_id, 1000, 0),
    );
    let connector_count = sensors.map(|page| page.sensors.len()).unwrap_or(0);
    let record_count =
        stats.map(|items| items.iter().map(|item| item.record_count).sum()).unwrap_or(0);
    let event_count = events.map(|page| page.events.len()).unwrap_or(0);
    let open_incident_count = incidents
        .map(|items| {
            items
                .iter()
                .filter(|item| item.incident.status != common::IncidentStatus::Resolved)
                .count()
        })
        .unwrap_or(0);
    let trigger_count =
        triggers.map(|page| page.triggers.iter().filter(|item| item.enabled).count()).unwrap_or(0);

    let health = match state.health_client.platform_health().await {
        Ok(summary) => summary,
        Err(e) => {
            return Html(
                PipelineTemplate {
                    show_nav: true,
                    is_admin,
                    items: vec![],
                    connector_count,
                    record_count,
                    event_count,
                    open_incident_count,
                    trigger_count,
                    error: Some(e.to_string()),
                    stages: vec![],
                    queues: vec![],
                    queue_total: 0,
                    critical_queue_count: 0,
                    max_queue: 0,
                    backlog_error: None,
                    severity: normalize_pressure_scope(&query.severity),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    // Backlog is a lower-value signal than up/down health — a lookup failure degrades this
    // page to "no backlog numbers" rather than an error page, since the topology itself is
    // still meaningful without it.
    let (depths, backlog_error) = match state.backlog_client.queue_depths().await {
        Ok(value) => (value, None),
        Err(error) => (vec![], Some(error.to_string())),
    };
    let stage_status = |service: &str| {
        health
            .services
            .iter()
            .find(|item| item.name == service)
            .map(|item| item.status.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };
    let stage_href = |service: &str| match service {
        "ingestion-service" => "/sensors",
        "normalization-service" => "/normalization-mappings",
        "analysis-service" => "/analysis-config",
        "trigger-engine" => "/triggers",
        "action-executor" => "/actions",
        _ => "/pipeline",
    };
    let stages = crate::topology::PIPELINE_STAGES
        .iter()
        .map(|(service, label)| StageDiagnostic {
            label: (*label).to_string(),
            service: (*service).to_string(),
            status: stage_status(service),
            href: stage_href(service).to_string(),
        })
        .collect();
    let mut queues = crate::topology::PIPELINE_EDGES
        .iter()
        .map(|(stage, label)| {
            let messages = depths
                .iter()
                .find(|depth| depth.stage == *stage)
                .map(|depth| depth.messages)
                .unwrap_or(0);
            let severity = crate::topology::severity_for(messages).to_string();
            QueueDiagnostic {
                label: (*label).to_string(),
                queue_name: depths
                    .iter()
                    .find(|depth| depth.stage == *stage)
                    .map(|depth| depth.queue_name.clone())
                    .unwrap_or_else(|| (*stage).to_string()),
                messages,
                severity_label: crate::topology::severity_label(&severity).to_string(),
                severity,
                href: match *stage {
                    "ingest_to_normalize" => "/data?normalized=false".to_string(),
                    "normalize_to_analyze" => "/data?normalized=true".to_string(),
                    "analyze_to_trigger" => "/events?status=new".to_string(),
                    "trigger_to_action" => "/actions?outcome=review".to_string(),
                    _ => "/pipeline".to_string(),
                },
                pressure_pct: 0,
            }
        })
        .collect::<Vec<_>>();
    let max_queue = queues.iter().map(|queue| queue.messages).max().unwrap_or(0);
    for queue in &mut queues {
        queue.pressure_pct =
            if max_queue == 0 { 0 } else { (((queue.messages * 100) / max_queue).max(3)) as usize };
    }
    let severity = normalize_pressure_scope(&query.severity);
    if !severity.is_empty() {
        queues.retain(|queue| queue.severity == severity);
    }
    let queue_total = queues.iter().map(|queue| queue.messages).sum();
    let critical_queue_count = queues.iter().filter(|queue| queue.severity == "critical").count();
    let items = build_topology_items(&health, &depths);

    Html(
        PipelineTemplate {
            show_nav: true,
            is_admin,
            items,
            connector_count,
            record_count,
            event_count,
            open_incident_count,
            trigger_count,
            error: None,
            stages,
            queues,
            queue_total,
            critical_queue_count,
            max_queue,
            backlog_error,
            severity,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
