#[path = "signal_value_test.rs"]
#[cfg(test)]
mod signal_value_test;

use serde::{Deserialize, Serialize};

/// A signal's measured value; the four value kinds of the signal registry (docs/yocho.md
/// "Signal storage and retention"). Serialized adjacently tagged —
/// `{"kind": "gauge", "value": 1.5}` — so consumers can dispatch on `kind` without guessing
/// from the JSON number type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SignalValue {
    /// Point-in-time measurement (e.g. reply latency hours).
    Gauge(f64),
    /// Non-negative count (e.g. tickets opened); negative counts are unrepresentable.
    Counter(u64),
    /// One label out of a set (e.g. sentiment bucket).
    Categorical(String),
    /// Composite or model score; per-input contributions live on [`crate::Signal::contributions`].
    Score(f64),
}

impl SignalValue {
    /// The registry value-kind name, identical to the serialized `kind` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Gauge(_) => "gauge",
            Self::Counter(_) => "counter",
            Self::Categorical(_) => "categorical",
            Self::Score(_) => "score",
        }
    }
}
