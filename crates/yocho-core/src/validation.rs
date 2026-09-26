#[path = "validation_test.rs"]
#[cfg(test)]
mod validation_test;

use crate::signal::Signal;
use crate::signal_value::SignalValue;
use thiserror::Error;

/// Max length (bytes) of `signal_type`, entity type/id, and version strings, and of a
/// categorical value.
pub const MAX_IDENT_LEN: usize = 256;
/// Max number of dims per signal; dims are meant to be low-cardinality labels, not payload.
pub const MAX_DIMS: usize = 32;
pub const MAX_DIM_KEY_LEN: usize = 64;
pub const MAX_DIM_VALUE_LEN: usize = 256;

/// Why a signal was rejected. Validation never panics — every input, however malformed,
/// yields `Ok` or one of these.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignalError {
    #[error("`{field}` must not be empty")]
    Empty { field: &'static str },
    #[error("`{field}` exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("`{field}` must be a finite number")]
    NotFinite { field: &'static str },
    #[error("too many dims: {count} > {max}")]
    TooManyDims { count: usize, max: usize },
}

impl Signal {
    /// Checks the invariants every producer must hold before publishing on `signal.emitted`:
    /// non-empty, bounded identifiers; finite numeric values (NaN/inf would poison rollups);
    /// bounded dims; finite contribution weights and values.
    pub fn validate(&self) -> Result<(), SignalError> {
        ident("signal_type", &self.signal_type, MAX_IDENT_LEN)?;
        ident("entity_type", &self.entity.entity_type, MAX_IDENT_LEN)?;
        ident("entity_id", &self.entity.entity_id, MAX_IDENT_LEN)?;
        ident("extractor_version", &self.extractor_version, MAX_IDENT_LEN)?;
        ident("config_version", &self.config_version, MAX_IDENT_LEN)?;
        match &self.value {
            SignalValue::Gauge(v) | SignalValue::Score(v) => finite("value", *v)?,
            SignalValue::Categorical(label) => ident("value", label, MAX_IDENT_LEN)?,
            SignalValue::Counter(_) => {}
        }
        if self.dims.len() > MAX_DIMS {
            return Err(SignalError::TooManyDims { count: self.dims.len(), max: MAX_DIMS });
        }
        for (key, value) in &self.dims {
            ident("dims key", key, MAX_DIM_KEY_LEN)?;
            bounded("dims value", value, MAX_DIM_VALUE_LEN)?;
        }
        for source in &self.provenance.source_item_ids {
            ident("source_item_id", source, MAX_IDENT_LEN)?;
        }
        if let Some(model_version) = &self.provenance.model_version {
            ident("model_version", model_version, MAX_IDENT_LEN)?;
        }
        for contribution in &self.contributions {
            finite("contribution weight", contribution.weight)?;
            finite("contribution value", contribution.value)?;
        }
        Ok(())
    }
}

/// Non-blank (after trimming) and at most `max` bytes.
fn ident(field: &'static str, value: &str, max: usize) -> Result<(), SignalError> {
    if value.trim().is_empty() {
        return Err(SignalError::Empty { field });
    }
    bounded(field, value, max)
}

fn bounded(field: &'static str, value: &str, max: usize) -> Result<(), SignalError> {
    if value.len() > max {
        return Err(SignalError::TooLong { field, max });
    }
    Ok(())
}

fn finite(field: &'static str, value: f64) -> Result<(), SignalError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(SignalError::NotFinite { field })
    }
}
