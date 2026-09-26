#[path = "provenance_test.rs"]
#[cfg(test)]
mod provenance_test;

use crate::signal_id::SignalId;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Where a signal came from (docs/yocho.md "Lineage and drill-down"): the evidence items it
/// was extracted from and the model that produced it, plus an opaque encrypted payload for
/// anything sensitive (participants, subject). Core never encrypts or decrypts — the
/// ciphertext is produced by `yocho-crypto` and carried here verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// Source-stable ids of the evidence items (e.g. email `Message-ID`, ticket id).
    pub source_item_ids: Vec<String>,
    /// Model version, for model-produced signals; `None` for deterministic extractors.
    pub model_version: Option<String>,
    /// Encrypted provenance detail, base64 on the wire; `None` when there is nothing sensitive.
    pub encrypted: Option<EncryptedBlob>,
}

/// Opaque ciphertext bytes. Serialized as a standard base64 string so the JSON stays compact
/// (a raw `Vec<u8>` would serialize as a number array).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedBlob(pub Vec<u8>);

impl Serialize for EncryptedBlob {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for EncryptedBlob {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map(Self).map_err(serde::de::Error::custom)
    }
}

/// One input's share of a score or detector output (docs/yocho.md "Lineage and drill-down"):
/// which input signal drove it, the weight applied, and the resulting contribution value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    pub signal_id: SignalId,
    pub weight: f64,
    pub value: f64,
}
