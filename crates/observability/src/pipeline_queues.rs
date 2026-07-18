/// The queues each pipeline-stage consumer declares and binds (spec §3's
/// ingest → normalize → analyze → act chain) — one well-known name per stage, matching the
/// `<consumer>.<message-type>` naming convention each service's `main.rs` already uses.
/// Centralized here so `backlog.rs` and its tests share one source of truth instead of
/// duplicating the queue name strings.
pub const PIPELINE_QUEUES: &[(&str, &str)] = &[
    ("ingest_to_normalize", "normalization-service.record.ingested"),
    ("normalize_to_analyze", "analysis-service.record.normalized"),
    ("analyze_to_trigger", "trigger-engine.record.analyzed"),
    ("trigger_to_action", "action-executor.event.created"),
];
