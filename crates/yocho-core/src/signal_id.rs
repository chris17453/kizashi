#[path = "signal_id_test.rs"]
#[cfg(test)]
mod signal_id_test;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use ulid::Ulid;

/// Unique id of one signal row (ADR-0205 §1). A ULID, so ids sort by creation time and the
/// lineage drill-down can page through them without a separate timestamp index. Serialized as
/// the canonical 26-character Crockford base32 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignalId(Ulid);

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid signal id: {0}")]
pub struct ParseSignalIdError(String);

impl SignalId {
    /// A fresh id stamped with the current time.
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    pub fn as_ulid(&self) -> Ulid {
        self.0
    }
}

impl Default for SignalId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SignalId {
    type Err = ParseSignalIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ulid::from_string(s).map(Self).map_err(|_| ParseSignalIdError(s.to_string()))
    }
}
