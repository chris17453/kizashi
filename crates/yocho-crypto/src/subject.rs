#[path = "subject_test.rs"]
#[cfg(test)]
mod subject_test;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const AAD_DOMAIN: &[u8] = b"yocho-subject-v1";

/// A data subject whose fields are encrypted under one DEK: `(tenant_id, subject_type,
/// subject_id)`. Opaque to this crate — `subject_type` is whatever the caller uses ("mailbox",
/// "contact", "account"); nothing here interprets it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubjectRef {
    pub tenant_id: Uuid,
    pub subject_type: String,
    pub subject_id: String,
}

impl SubjectRef {
    pub fn new(
        tenant_id: Uuid,
        subject_type: impl Into<String>,
        subject_id: impl Into<String>,
    ) -> Self {
        Self { tenant_id, subject_type: subject_type.into(), subject_id: subject_id.into() }
    }

    /// Additional authenticated data binding a ciphertext to this exact tenant and subject.
    /// Length-prefixed so `("ab","c")` and `("a","bc")` can never collide.
    pub(crate) fn aad(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            AAD_DOMAIN.len() + 16 + 8 + self.subject_type.len() + self.subject_id.len(),
        );
        out.extend_from_slice(AAD_DOMAIN);
        out.extend_from_slice(self.tenant_id.as_bytes());
        push_len_prefixed(&mut out, self.subject_type.as_bytes());
        push_len_prefixed(&mut out, self.subject_id.as_bytes());
        out
    }
}

pub(crate) fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}
