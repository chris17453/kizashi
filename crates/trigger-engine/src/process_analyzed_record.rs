#[path = "process_analyzed_record_test.rs"]
#[cfg(test)]
mod process_analyzed_record_test;

use crate::classify::{candidates, group_key};
use crate::event_publisher::EventPublisher;
use crate::event_store::EventStore;
use crate::signal_repository::{AnalyzedSignal, SignalRepository};
use crate::trigger_repository::TriggerRepository;
use common::{AnalyzedRecord, Event, TriggerCondition, TriggerDefinition};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to record signal: {0}")]
    SignalRecord(String),
    #[error("failed to look up triggers: {0}")]
    TriggerLookup(String),
    #[error("failed to read window stats: {0}")]
    WindowStats(String),
    #[error("failed to write event: {0}")]
    EventWrite(String),
}

#[derive(Clone)]
pub struct TriggerDeps {
    pub trigger_repository: Arc<dyn TriggerRepository>,
    pub signal_repository: Arc<dyn SignalRepository>,
    pub event_store: Arc<dyn EventStore>,
    pub publisher: Arc<dyn EventPublisher>,
}

/// One `record.analyzed` message all the way through to zero or more `Event`s (spec §6,
/// service #6): derive candidate event types (ADR-0006), record each as a durable signal,
/// evaluate every enabled TriggerDefinition matching that event type against the signal's
/// window, and write+publish an Event for every trigger that fires. Returns the number of
/// Events created.
pub async fn process_analyzed_record(
    deps: &TriggerDeps,
    record: &AnalyzedRecord,
) -> Result<usize, ProcessError> {
    let tenant_id = record.record.tenant_id;
    let group_key = group_key(record);
    let mut events_created = 0;

    for candidate in candidates(record) {
        let signal = AnalyzedSignal {
            id: Uuid::new_v4(),
            tenant_id,
            record_id: record.record.id,
            event_type: candidate.event_type.clone(),
            group_key: group_key.clone(),
            entity_ref: group_key.clone(),
            numeric_value: Some(candidate.numeric_value),
            source_connector_id: record.record.connector_id.clone(),
            occurred_at: record.analyzed_at,
        };
        deps.signal_repository
            .record_signal(&signal)
            .await
            .map_err(|e| ProcessError::SignalRecord(e.to_string()))?;

        let triggers = deps
            .trigger_repository
            .active_triggers_for(tenant_id, &candidate.event_type)
            .await
            .map_err(|e| ProcessError::TriggerLookup(e.to_string()))?;

        for trigger in triggers {
            let (fired, window_record_ids) = evaluate_trigger(
                &deps.signal_repository,
                &trigger,
                tenant_id,
                &candidate.event_type,
                &group_key,
            )
            .await?;
            if !fired {
                continue;
            }

            let event = Event::new(
                tenant_id,
                candidate.event_type.clone(),
                group_key.clone(),
                group_key.clone(),
                serde_json::json!({"triggered_by": trigger.id, "value": candidate.numeric_value}),
                record.analyzed_at,
            )
            .with_record_ids(window_record_ids);

            deps.event_store
                .insert_event(&event)
                .await
                .map_err(|e| ProcessError::EventWrite(e.to_string()))?;

            if let Err(e) = deps.publisher.publish_event_created(&event).await {
                tracing::error!(event_id = %event.id, error = %e, "failed to publish event.created");
            }
            events_created += 1;
        }
    }

    Ok(events_created)
}

/// Evaluates one trigger against a group_key's real, already-recorded signal history — the
/// core logic shared by the live `record.analyzed` path above and the `/v1/triggers/:id/test`
/// dry-run endpoint (ADR-0030), so a dry run can never drift from what production actually
/// does. For the two single-event-type shapes this is one `window_stats` call for
/// `candidate_event_type` (the live path's newly-arrived candidate, or the dry-run's
/// `trigger.event_type_match`). For `CorrelatedOverWindow` (ADR-0027), `window_stats` is
/// called once per listed event type — not just the candidate's — since every leg must
/// independently meet its own `min_count`; the returned record ids are the union across all
/// legs, so a fired Event's lineage covers every signal that contributed to satisfying it.
pub async fn evaluate_trigger(
    signal_repository: &Arc<dyn SignalRepository>,
    trigger: &TriggerDefinition,
    tenant_id: Uuid,
    candidate_event_type: &str,
    group_key: &str,
) -> Result<(bool, Vec<Uuid>), ProcessError> {
    match &trigger.condition {
        TriggerCondition::CorrelatedOverWindow { conditions } => {
            let mut counts = HashMap::new();
            let mut record_ids = Vec::new();
            for leg in conditions {
                let (count, _values, leg_record_ids) = signal_repository
                    .window_stats(tenant_id, &leg.event_type, group_key, trigger.window_seconds)
                    .await
                    .map_err(|e| ProcessError::WindowStats(e.to_string()))?;
                counts.insert(leg.event_type.clone(), count);
                record_ids.extend(leg_record_ids);
            }
            Ok((trigger.evaluate_correlated(&counts), record_ids))
        }
        _ => {
            let (count, values, record_ids) = signal_repository
                .window_stats(tenant_id, candidate_event_type, group_key, trigger.window_seconds)
                .await
                .map_err(|e| ProcessError::WindowStats(e.to_string()))?;
            Ok((trigger.evaluate(count, &values), record_ids))
        }
    }
}
