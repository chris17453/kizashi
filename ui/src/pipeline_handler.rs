#[path = "pipeline_handler_test.rs"]
#[cfg(test)]
mod pipeline_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

/// The ingest → normalize → analyze → act chain (spec §3), in pipeline order — matches
/// `observability::pipeline_queues::PIPELINE_QUEUES`' stage keys and the `SERVICE_REGISTRY`
/// names each service registers under (docker-compose.yml), so this is the single place that
/// turns those internal keys into a human-readable topology.
const PIPELINE_STAGES: &[(&str, &str)] = &[
    ("ingestion-service", "Ingestion"),
    ("normalization-service", "Normalization"),
    ("analysis-service", "Analysis"),
    ("trigger-engine", "Trigger Engine"),
    ("action-executor", "Action Executor"),
];

/// `(stage_key, edge_label)` — `stage_key` matches `QueueDepthSummary::stage`;
/// `edge_label` is what's shown on the connector between the two stage boxes it sits between.
const PIPELINE_EDGES: &[(&str, &str)] = &[
    ("ingest_to_normalize", "record.ingested"),
    ("normalize_to_analyze", "record.normalized"),
    ("analyze_to_trigger", "record.analyzed"),
    ("trigger_to_action", "event.created"),
];

/// Above this many queued messages an edge is `critical`; above zero but below this it's
/// `warn` — thresholds chosen as a reasonable default for a platform this size, not derived
/// from any SLA (there isn't one defined yet).
const CRITICAL_BACKLOG_THRESHOLD: u64 = 50;

/// A flat, already-interleaved [stage, edge, stage, edge, ..., stage] sequence — built here
/// rather than left for the template to zip/index, since Askama's expression grammar makes
/// index arithmetic (`edges[loop.index0 - 1]`) fragile to get right.
enum TopologyItem {
    Stage { label: &'static str, status: String },
    Edge { label: &'static str, messages: Option<u64>, severity: &'static str },
}

#[derive(Template)]
#[template(path = "pipeline.html")]
struct PipelineTemplate {
    show_nav: bool,
    items: Vec<TopologyItem>,
    error: Option<String>,
}

fn severity_for(messages: u64) -> &'static str {
    if messages == 0 {
        "ok"
    } else if messages < CRITICAL_BACKLOG_THRESHOLD {
        "warn"
    } else {
        "critical"
    }
}

pub async fn get_pipeline(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = require_session(state.session_store.as_ref(), &headers).await;
    if let Err(response) = session {
        return response;
    }

    let health = match state.health_client.platform_health().await {
        Ok(summary) => summary,
        Err(e) => {
            return Html(
                PipelineTemplate { show_nav: true, items: vec![], error: Some(e.to_string()) }
                    .render()
                    .unwrap(),
            )
            .into_response();
        }
    };

    let stage_status = |key: &str| -> String {
        health
            .services
            .iter()
            .find(|s| s.name == key)
            .map(|s| s.status.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };

    // Backlog is a lower-value signal than up/down health — a lookup failure degrades this
    // page to "no backlog numbers" rather than an error page, since the topology itself is
    // still meaningful without it.
    let depths = state.backlog_client.queue_depths().await.unwrap_or_default();
    let edge_for = |key: &str, label: &'static str| -> TopologyItem {
        let messages = depths.iter().find(|d| d.stage == key).map(|d| d.messages);
        let severity = messages.map(severity_for).unwrap_or("unknown");
        TopologyItem::Edge { label, messages, severity }
    };

    let mut items = Vec::with_capacity(PIPELINE_STAGES.len() + PIPELINE_EDGES.len());
    for (i, (stage_key, stage_label)) in PIPELINE_STAGES.iter().enumerate() {
        if i > 0 {
            let (edge_key, edge_label) = PIPELINE_EDGES[i - 1];
            items.push(edge_for(edge_key, edge_label));
        }
        items.push(TopologyItem::Stage { label: stage_label, status: stage_status(stage_key) });
    }

    Html(PipelineTemplate { show_nav: true, items, error: None }.render().unwrap()).into_response()
}
