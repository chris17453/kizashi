#[path = "health_handler_test.rs"]
#[cfg(test)]
mod health_handler_test;

use crate::backlog_client::QueueDepthSummary;
use crate::session_guard::require_session;
use crate::{AppState, ServiceHealthSummary};
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Clone)]
struct ServiceHealthView {
    name: String,
    status: String,
    description: String,
    href: String,
    next_step: String,
}

struct QueueHealthView {
    label: String,
    queue_name: String,
    messages: u64,
    severity: String,
    severity_label: String,
    pressure_pct: usize,
    href: String,
}

fn queue_href(stage: &str) -> String {
    match stage {
        "ingest_to_normalize" => "/data?normalized=false".to_string(),
        "normalize_to_analyze" => "/data?normalized=true".to_string(),
        "analyze_to_trigger" => "/events?status=new".to_string(),
        "trigger_to_action" => "/actions?outcome=review".to_string(),
        _ => "/pipeline".to_string(),
    }
}

struct DependencyLaneView {
    stage: String,
    service_name: String,
    service_status: String,
    service_href: String,
    queue_href: String,
    queue_label: String,
    queue_messages: u64,
    queue_severity_label: String,
}

struct HealthFunnelStage {
    label: String,
    detail: String,
    count: usize,
    percent: usize,
    href: String,
}

fn data_plane_funnel(
    connector_count: usize,
    record_count: i64,
    signal_count: usize,
    open_case_count: usize,
    modeled_object_count: usize,
    action_contract_count: usize,
) -> Vec<HealthFunnelStage> {
    let counts = [
        ("Connect", "registered connectors", connector_count, "/sensors"),
        ("Normalize", "ingested records", record_count.max(0) as usize, "/data"),
        ("Understand", "generated signals", signal_count, "/events"),
        ("Detect", "active cases", open_case_count, "/incidents?status=active"),
        ("Model", "ontology entities", modeled_object_count, "/ontology"),
        ("Respond", "action contracts", action_contract_count, "/actions/library"),
    ];
    let max = counts.iter().map(|(_, _, count, _)| *count).max().unwrap_or(0);
    counts
        .into_iter()
        .map(|(label, detail, count, href)| HealthFunnelStage {
            label: label.to_string(),
            detail: detail.to_string(),
            count,
            percent: if max == 0 { 0 } else { (count * 100 / max).max(4) },
            href: href.to_string(),
        })
        .collect()
}

fn dependency_lanes(
    services: &[ServiceHealthView],
    queues: &[QueueHealthView],
) -> Vec<DependencyLaneView> {
    crate::topology::PIPELINE_STAGES
        .iter()
        .enumerate()
        .map(|(index, (service_key, stage))| {
            let service = services
                .iter()
                .find(|item| item.name.to_ascii_lowercase().replace(' ', "-") == *service_key)
                .or_else(|| {
                    services.iter().find(|item| {
                        item.name
                            .to_ascii_lowercase()
                            .contains(service_key.split('-').next().unwrap_or(service_key))
                    })
                })
                .cloned();
            let queue = queues.get(index);
            DependencyLaneView {
                stage: (*stage).to_string(),
                service_name: service
                    .as_ref()
                    .map(|item| item.name.clone())
                    .unwrap_or_else(|| (*service_key).to_string()),
                service_status: service
                    .as_ref()
                    .map(|item| item.status.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                service_href: service
                    .as_ref()
                    .map(|item| item.href.clone())
                    .unwrap_or_else(|| "/pipeline".to_string()),
                queue_href: queue
                    .map(|item| item.href.clone())
                    .unwrap_or_else(|| "/pipeline".to_string()),
                queue_label: queue
                    .map(|item| item.queue_name.clone())
                    .unwrap_or_else(|| "no upstream queue".to_string()),
                queue_messages: queue.map(|item| item.messages).unwrap_or(0),
                queue_severity_label: queue
                    .map(|item| item.severity_label.clone())
                    .unwrap_or_else(|| "empty".to_string()),
            }
        })
        .collect()
}

fn queue_views(depths: Vec<QueueDepthSummary>) -> Vec<QueueHealthView> {
    let mut views = crate::topology::PIPELINE_EDGES
        .iter()
        .map(|(stage, label)| {
            let depth = depths.iter().find(|item| item.stage == *stage);
            let messages = depth.map(|item| item.messages).unwrap_or(0);
            let severity = crate::topology::severity_for(messages).to_string();
            QueueHealthView {
                label: (*label).to_string(),
                queue_name: depth
                    .map(|item| item.queue_name.clone())
                    .unwrap_or_else(|| (*stage).to_string()),
                messages,
                severity_label: crate::topology::severity_label(&severity).to_string(),
                severity,
                pressure_pct: 0,
                href: queue_href(stage),
            }
        })
        .collect::<Vec<_>>();
    let max_messages = views.iter().map(|view| view.messages).max().unwrap_or(0);
    for view in &mut views {
        view.pressure_pct = if max_messages == 0 {
            0
        } else {
            (((view.messages.saturating_mul(100)) / max_messages).max(3)) as usize
        };
    }
    views
}

fn service_view(service: ServiceHealthSummary) -> ServiceHealthView {
    let key = service.name.to_ascii_lowercase();
    let (description, href, next_step) = if key.contains("ingest") || key.contains("connector") {
        (
            "Connector intake and raw record delivery".to_string(),
            "/sensors".to_string(),
            "Inspect connectors".to_string(),
        )
    } else if key.contains("query") || key.contains("event") {
        (
            "Search, event generation, and signal retrieval".to_string(),
            "/events".to_string(),
            "Open event queue".to_string(),
        )
    } else if key.contains("ontology") {
        (
            "Modeled objects, relationships, and governed actions".to_string(),
            "/ontology".to_string(),
            "Inspect ontology".to_string(),
        )
    } else if key.contains("action") || key.contains("executor") {
        (
            "Execution boundary for approved operational changes".to_string(),
            "/actions".to_string(),
            "Review action ledger".to_string(),
        )
    } else if key.contains("incident") {
        (
            "Case management and investigation state".to_string(),
            "/incidents".to_string(),
            "Open case queue".to_string(),
        )
    } else {
        (
            "Runtime dependency monitored by the platform".to_string(),
            "/pipeline".to_string(),
            "Open pipeline map".to_string(),
        )
    };
    ServiceHealthView { name: service.name, status: service.status, description, href, next_step }
}

#[derive(Template)]
#[template(path = "health.html")]
struct HealthTemplate {
    show_nav: bool,
    is_admin: bool,
    platform_status: Option<String>,
    services: Vec<ServiceHealthView>,
    queues: Vec<QueueHealthView>,
    services_up: usize,
    services_down: usize,
    queue_total: u64,
    critical_queue_count: usize,
    max_queue: u64,
    backlog_error: Option<String>,
    error: Option<String>,
    dependency_lanes: Vec<DependencyLaneView>,
    data_plane_funnel: Vec<HealthFunnelStage>,
    connector_count: usize,
    record_count: i64,
    signal_count: usize,
    open_case_count: usize,
    modeled_object_count: usize,
    action_contract_count: usize,
}

pub async fn get_health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let (queues, backlog_error) = match state.backlog_client.queue_depths().await {
        Ok(depths) => (queue_views(depths), None),
        Err(error) => (queue_views(vec![]), Some(error.to_string())),
    };

    // Pair infrastructure health with the tenant's live data plane. A platform can be
    // entirely "up" while no records are arriving or cases are being created; keeping these
    // counts on the same snapshot makes that distinction visible to an operator.
    let (sensors, connector_stats, events, incidents) = tokio::join!(
        state.sensors_client.list_sensors(session.tenant_id, 1000, 0),
        state.stats_client.connector_stats(session.tenant_id),
        state.events_client.list_events(&session.bearer_token, 1000, 0, None, None),
        state.incidents_client.list_incidents(session.tenant_id, None),
    );
    let connector_count = sensors.map(|page| page.sensors.len()).unwrap_or(0);
    let record_count =
        connector_stats.map(|items| items.iter().map(|item| item.record_count).sum()).unwrap_or(0);
    let signal_count = events.map(|page| page.events.len()).unwrap_or(0);
    let open_case_count = incidents
        .map(|items| {
            items
                .iter()
                .filter(|item| item.incident.status != common::IncidentStatus::Resolved)
                .count()
        })
        .unwrap_or(0);
    let (modeled_object_count, action_contract_count) =
        if let Some(client) = crate::ontology_client::global() {
            let (objects, actions) = tokio::join!(
                client.list_objects(&session.bearer_token, None),
                client.list_action_types(&session.bearer_token),
            );
            (
                objects.map(|items| items.len()).unwrap_or(0),
                actions.map(|items| items.len()).unwrap_or(0),
            )
        } else {
            (0, 0)
        };
    let data_plane_funnel = data_plane_funnel(
        connector_count,
        record_count,
        signal_count,
        open_case_count,
        modeled_object_count,
        action_contract_count,
    );

    match state.health_client.platform_health().await {
        Ok(summary) => Html(
            {
                let services_up =
                    summary.services.iter().filter(|service| service.status == "up").count();
                let services_down = summary.services.len().saturating_sub(services_up);
                let queue_total = queues.iter().map(|queue| queue.messages).sum();
                let critical_queue_count =
                    queues.iter().filter(|queue| queue.severity == "critical").count();
                let max_queue = queues.iter().map(|queue| queue.messages).max().unwrap_or(0);
                let services: Vec<ServiceHealthView> =
                    summary.services.into_iter().map(service_view).collect();
                let dependency_lanes = dependency_lanes(&services, &queues);
                HealthTemplate {
                    show_nav: true,
                    is_admin,
                    platform_status: Some(summary.status),
                    services,
                    queues,
                    services_up,
                    services_down,
                    queue_total,
                    critical_queue_count,
                    max_queue,
                    backlog_error,
                    error: None,
                    dependency_lanes,
                    data_plane_funnel,
                    connector_count,
                    record_count,
                    signal_count,
                    open_case_count,
                    modeled_object_count,
                    action_contract_count,
                }
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            {
                let dependency_lanes = dependency_lanes(&[], &queues);
                HealthTemplate {
                    show_nav: true,
                    is_admin,
                    platform_status: None,
                    services: vec![],
                    queues,
                    services_up: 0,
                    services_down: 0,
                    queue_total: 0,
                    critical_queue_count: 0,
                    max_queue: 0,
                    backlog_error,
                    error: Some(e.to_string()),
                    dependency_lanes,
                    data_plane_funnel,
                    connector_count,
                    record_count,
                    signal_count,
                    open_case_count,
                    modeled_object_count,
                    action_contract_count,
                }
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
