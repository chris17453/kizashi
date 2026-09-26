#[path = "entity_ref_test.rs"]
#[cfg(test)]
mod entity_ref_test;

use serde::{Deserialize, Serialize};

/// The *source identity* a signal is about (ADR-0208 §1): the thing the extractor actually saw
/// — an email address, a domain, a Zendesk org id, an ERP customer number. Deliberately not a
/// customer id: attribution to customer/BU is late-bound through the entity resolver's
/// versioned mapping, so fixing a match heals history without rewriting signals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    /// Identity kind, e.g. `email`, `domain`, `zendesk_org`, `erp_customer`.
    pub entity_type: String,
    /// Identity value as seen at the source.
    pub entity_id: String,
}

impl EntityRef {
    pub fn new(entity_type: impl Into<String>, entity_id: impl Into<String>) -> Self {
        Self { entity_type: entity_type.into(), entity_id: entity_id.into() }
    }
}
