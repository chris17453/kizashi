#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use crate::raw_record::{RawRecord, SourceType};
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

/// Shared trait every connector/agent crate implements (spec §6, service #1). Connectors
/// are the only components allowed to reach into a source system directly; everything past
/// this boundary talks in `RawRecord`s over the Ingestion Gateway.
#[async_trait]
pub trait Connector: Send + Sync {
    fn connector_id(&self) -> &str;
    fn source_type(&self) -> SourceType;
    async fn poll(&self, tenant_id: Uuid) -> Result<Vec<RawRecord>, ConnectorError>;

    /// A high-water mark this connector's *next* poll can resume from, derived from the
    /// records this poll actually found — e.g. the highest IMAP UID seen. `None` by default:
    /// most connectors have no such concept and keep re-polling whatever window their static
    /// config describes (their orchestrator is responsible for narrowing that window some
    /// other way, if at all). A connector that returns `Some` here is opting in to the
    /// orchestrator persisting and replaying this value on the next invocation, instead of
    /// re-scanning from scratch every time.
    fn checkpoint(&self, _records: &[RawRecord]) -> Option<String> {
        None
    }
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("auth failed for connector: {0}")]
    AuthFailed(String),
    #[error("upstream source unavailable: {0}")]
    SourceUnavailable(String),
    #[error("malformed record from source: {0}")]
    MalformedRecord(String),
    #[error("rate limited by source, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}
