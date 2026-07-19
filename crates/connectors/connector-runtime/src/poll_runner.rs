#[path = "poll_runner_test.rs"]
#[cfg(test)]
mod poll_runner_test;

use crate::ingestion_client::IngestionClient;
use common::connector::Connector;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PollRunError {
    #[error("connector poll failed: {0}")]
    Poll(String),
}

#[derive(Debug, Default, PartialEq)]
pub struct PollSummary {
    pub polled: usize,
    pub ingested: usize,
    pub failed: usize,
    /// This connector's `Connector::checkpoint` for the just-polled records, if it supports
    /// one — the orchestrator persists this and passes it back on the next invocation so the
    /// connector can resume precisely instead of re-scanning its whole configured window.
    pub checkpoint: Option<String>,
}

/// One CronJob-scheduled poll cycle (spec §3, ADR-0013): poll the source, post every record to
/// Ingestion Gateway. A single record's post failure is logged and counted, not fatal to the
/// cycle — one bad record shouldn't block ingesting the rest of the batch.
pub async fn run_poll_cycle(
    connector: &dyn Connector,
    tenant_id: Uuid,
    ingestion_client: &dyn IngestionClient,
) -> Result<PollSummary, PollRunError> {
    let records = connector.poll(tenant_id).await.map_err(|e| PollRunError::Poll(e.to_string()))?;
    let checkpoint = connector.checkpoint(&records);

    let mut summary = PollSummary { polled: records.len(), checkpoint, ..Default::default() };
    for record in &records {
        match ingestion_client.ingest(record).await {
            Ok(()) => summary.ingested += 1,
            Err(e) => {
                tracing::error!(record_id = %record.id, error = %e, "failed to ingest polled record");
                summary.failed += 1;
            }
        }
    }
    Ok(summary)
}
