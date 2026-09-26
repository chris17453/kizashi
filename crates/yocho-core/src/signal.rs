#[path = "signal_test.rs"]
#[cfg(test)]
mod signal_test;

use crate::entity_ref::EntityRef;
use crate::provenance::{Contribution, Provenance};
use crate::signal_id::SignalId;
use crate::signal_value::SignalValue;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

/// One Yochō signal: a narrow, append-only measurement about a source identity (ADR-0205 §1),
/// and the body of every `signal.emitted` bus message. Validate with [`Signal::validate`]
/// before publishing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub signal_id: SignalId,
    /// Every signal row is tenant-scoped (ADR-0200 §3, CLAUDE.md §5).
    pub tenant_id: Uuid,
    /// Source identity, not customer (ADR-0208).
    pub entity: EntityRef,
    /// Registry name of the signal, e.g. `email.reply_latency_hours`.
    pub signal_type: String,
    pub ts: DateTime<Utc>,
    pub value: SignalValue,
    /// Low-cardinality dimensions (e.g. `direction=inbound`); a `BTreeMap` so the order is
    /// deterministic on the wire and in the idempotency key.
    pub dims: BTreeMap<String, String>,
    pub extractor_version: String,
    pub config_version: String,
    pub provenance: Provenance,
    /// Which input signals drove this one, and by how much. Set by scorers and detectors;
    /// empty for extracted signals. Kept outside the encrypted provenance so lineage
    /// drill-down (score → contributions → signals) is queryable without decryption.
    #[serde(default)]
    pub contributions: Vec<Contribution>,
}

impl Signal {
    /// Deterministic key identifying the *logical* measurement, used by the signal writer's
    /// `ReplacingMergeTree` to collapse duplicate deliveries and replays (ADR-0205 §4).
    ///
    /// `signal_id` cannot serve: it is a fresh ULID per emission, so an at-least-once redelivery
    /// or a replay of the same source item would get a new id and a duplicate row. The key is a
    /// SHA-256 (lowercase hex) over the tenant, signal type, entity, timestamp, dims, the sorted
    /// and de-duplicated source item ids, and the extractor version. Each field is length-
    /// prefixed so field boundaries can't be forged by concatenation. Deliberately excluded:
    /// the value, `config_version` and `model_version` — reprocessing the same evidence with the
    /// same extractor replaces the earlier row instead of adding a second one; a semantic change
    /// to what an extractor emits must bump `extractor_version`, which yields new keys.
    pub fn idempotency_key(&self) -> String {
        let mut sources: Vec<&str> =
            self.provenance.source_item_ids.iter().map(String::as_str).collect();
        sources.sort_unstable();
        sources.dedup();

        let mut hasher = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        };
        field(b"yocho.signal.idempotency.v1");
        field(self.tenant_id.as_bytes());
        field(self.signal_type.as_bytes());
        field(self.entity.entity_type.as_bytes());
        field(self.entity.entity_id.as_bytes());
        field(&self.ts.timestamp().to_be_bytes());
        field(&self.ts.timestamp_subsec_nanos().to_be_bytes());
        field(&(self.dims.len() as u64).to_be_bytes());
        for (key, value) in &self.dims {
            field(key.as_bytes());
            field(value.as_bytes());
        }
        field(&(sources.len() as u64).to_be_bytes());
        for source in sources {
            field(source.as_bytes());
        }
        field(self.extractor_version.as_bytes());

        hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
