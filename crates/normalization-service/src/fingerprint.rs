#[path = "fingerprint_test.rs"]
#[cfg(test)]
mod fingerprint_test;

use sha2::{Digest, Sha256};

/// Computes an exact-duplicate fingerprint over `dedup_fields`' values in `normalized_payload`
/// (ADR-0112). `None` if `dedup_fields` is empty — dedup is opt-in per mapping. Fields are
/// looked up and hashed in sorted-name order regardless of the caller's ordering, so the same
/// set of dedup fields always produces the same fingerprint no matter how the mapping's
/// `dedup_fields` list happens to be ordered.
pub fn compute_fingerprint(
    dedup_fields: &[String],
    normalized_payload: &serde_json::Value,
) -> Option<String> {
    if dedup_fields.is_empty() {
        return None;
    }
    let mut sorted_fields: Vec<&String> = dedup_fields.iter().collect();
    sorted_fields.sort();

    let mut hasher = Sha256::new();
    for field in sorted_fields {
        let value = normalized_payload.get(field).unwrap_or(&serde_json::Value::Null);
        hasher.update(field.as_bytes());
        hasher.update([0u8]);
        hasher.update(value.to_string().as_bytes());
        hasher.update([0u8]);
    }
    Some(format!("{:x}", hasher.finalize()))
}
