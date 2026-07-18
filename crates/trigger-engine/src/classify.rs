#[path = "classify_test.rs"]
#[cfg(test)]
mod classify_test;

use common::AnalyzedRecord;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub event_type: String,
    pub numeric_value: f64,
}

/// Derives candidate event types from an AnalyzedRecord's analysis output (ADR-0006): every
/// top-level numeric key in `analysis` becomes one candidate event, named after that key.
/// Never panics on whatever shape a model returns — non-numeric/non-object analysis values are
/// simply not candidates, not an error.
pub fn candidates(record: &AnalyzedRecord) -> Vec<Candidate> {
    let Some(obj) = record.analysis.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(key, value)| {
            value.as_f64().map(|v| Candidate { event_type: key.clone(), numeric_value: v })
        })
        .collect()
}

/// Derives the group_key/entity_ref a candidate event clusters under (ADR-0006): the record's
/// normalized `entity_ref` field when a NormalizationMapping has populated one, otherwise the
/// record's own id (so ungrouped records still get their own, stable single-member group
/// rather than silently colliding under an empty string).
pub fn group_key(record: &AnalyzedRecord) -> String {
    record
        .record
        .normalized_payload
        .as_ref()
        .and_then(|p| p.get("entity_ref"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| record.record.id.to_string())
}
