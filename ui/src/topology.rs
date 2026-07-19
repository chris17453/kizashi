#[path = "topology_test.rs"]
#[cfg(test)]
mod topology_test;

use crate::backlog_client::QueueDepthSummary;
use crate::health_client::PlatformHealthSummary;

/// The ingest → normalize → analyze → act chain (spec §3), in pipeline order — matches
/// `observability::pipeline_queues::PIPELINE_QUEUES`' stage keys and the `SERVICE_REGISTRY`
/// names each service registers under (docker-compose.yml). Shared by the Pipeline Map page
/// and the Overview dashboard's compact preview of the same topology.
pub(crate) const PIPELINE_STAGES: &[(&str, &str)] = &[
    ("ingestion-service", "Ingestion"),
    ("normalization-service", "Normalization"),
    ("analysis-service", "Analysis"),
    ("trigger-engine", "Trigger Engine"),
    ("action-executor", "Action Executor"),
];

/// `(stage_key, edge_label)` — `stage_key` matches `QueueDepthSummary::stage`;
/// `edge_label` is what's shown on the connector between the two stage boxes it sits between.
pub(crate) const PIPELINE_EDGES: &[(&str, &str)] = &[
    ("ingest_to_normalize", "record.ingested"),
    ("normalize_to_analyze", "record.normalized"),
    ("analyze_to_trigger", "record.analyzed"),
    ("trigger_to_action", "event.created"),
];

/// Above this many queued messages an edge is `critical`; above zero but below this it's
/// `warn` — thresholds chosen as a reasonable default for a platform this size, not derived
/// from any SLA (there isn't one defined yet).
pub(crate) const CRITICAL_BACKLOG_THRESHOLD: u64 = 50;

/// A flat, already-interleaved [stage, edge, stage, edge, ..., stage] sequence — built here
/// rather than left for a template to zip/index, since Askama's expression grammar makes
/// index arithmetic (`edges[loop.index0 - 1]`) fragile to get right.
pub(crate) enum TopologyItem {
    Stage { label: &'static str, status: String },
    Edge { label: &'static str, messages: Option<u64>, severity: &'static str },
}

pub(crate) fn severity_for(messages: u64) -> &'static str {
    if messages == 0 {
        "ok"
    } else if messages < CRITICAL_BACKLOG_THRESHOLD {
        "warn"
    } else {
        "critical"
    }
}

/// Builds the interleaved stage/edge sequence from a health summary and backlog depths — the
/// core logic both the full Pipeline Map page and the Overview dashboard's preview render.
pub(crate) fn build_topology_items(
    health: &PlatformHealthSummary,
    depths: &[QueueDepthSummary],
) -> Vec<TopologyItem> {
    let stage_status = |key: &str| -> String {
        health
            .services
            .iter()
            .find(|s| s.name == key)
            .map(|s| s.status.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };

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
    items
}
