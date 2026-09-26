#[path = "stage_test.rs"]
#[cfg(test)]
mod stage_test;

use crate::signal::Signal;
use crate::validation::SignalError;
use async_trait::async_trait;
use common::RawRecord;
use thiserror::Error;

/// Failure of a Yochō pipeline stage (docs/yocho.md "Pipeline").
#[derive(Debug, Error)]
pub enum StageError {
    /// The input cannot be processed by this stage (wrong source, missing field). Not retried.
    #[error("unsupported input: {0}")]
    UnsupportedInput(String),
    /// A dependency (model runtime, Foundry, registry) is unavailable. Retryable.
    #[error("dependency unavailable: {0}")]
    Unavailable(String),
    /// The stage produced a signal that fails [`Signal::validate`].
    #[error("invalid signal: {0}")]
    InvalidSignal(#[from] SignalError),
}

/// Raw item → typed signals. Runs after entity resolution; nothing downstream of an extractor
/// sees raw content. `version` is stamped into each signal's `extractor_version`.
#[async_trait]
pub trait Extractor: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn extract(&self, item: &RawRecord) -> Result<Vec<Signal>, StageError>;
}

/// Window of one entity's signals → anomaly/flag signals (per-entity baselines). Outputs record
/// the input signals that drove them in `contributions`.
#[async_trait]
pub trait Detector: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn detect(&self, window: &[Signal]) -> Result<Vec<Signal>, StageError>;
}

/// Input signals → one composite `SignalValue::Score` signal whose `contributions` explain it.
#[async_trait]
pub trait Scorer: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn score(&self, inputs: &[Signal]) -> Result<Signal, StageError>;
}
