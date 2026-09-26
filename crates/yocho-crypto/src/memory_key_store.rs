//! In-process [`SubjectKeyStore`] for unit tests and single-process dev tools. Same semantics as
//! the Postgres store (tombstones, idempotent shred, audit on every mutation), no durability.

#[path = "memory_key_store_test.rs"]
#[cfg(test)]
mod memory_key_store_test;

use crate::{
    CryptoError, KekId, KeyAuditAction, KeyAuditEntry, ShredResult, StoredSubjectKey,
    SubjectKeyStore, SubjectRef,
};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

#[derive(Default)]
struct State {
    keys: HashMap<SubjectRef, StoredSubjectKey>,
    audit: Vec<KeyAuditEntry>,
}

#[derive(Default)]
pub struct InMemorySubjectKeyStore {
    state: Mutex<State>,
}

impl InMemorySubjectKeyStore {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn audit_entry(
    subject: &SubjectRef,
    action: KeyAuditAction,
    kek_id: Option<KekId>,
    actor: &str,
) -> KeyAuditEntry {
    KeyAuditEntry {
        id: Uuid::new_v4(),
        subject: subject.clone(),
        action,
        kek_id,
        actor: actor.to_string(),
        occurred_at: Utc::now(),
    }
}

#[async_trait]
impl SubjectKeyStore for InMemorySubjectKeyStore {
    async fn get(&self, subject: &SubjectRef) -> Result<Option<StoredSubjectKey>, CryptoError> {
        Ok(self.lock().keys.get(subject).cloned())
    }

    async fn insert_if_absent(
        &self,
        subject: &SubjectRef,
        wrapped_dek: &[u8],
        kek_id: &KekId,
        actor: &str,
    ) -> Result<StoredSubjectKey, CryptoError> {
        let mut state = self.lock();
        if let Some(existing) = state.keys.get(subject) {
            return Ok(existing.clone());
        }
        let row = StoredSubjectKey {
            subject: subject.clone(),
            wrapped_dek: Some(wrapped_dek.to_vec()),
            kek_id: Some(kek_id.clone()),
            created_at: Utc::now(),
            shredded_at: None,
        };
        state.keys.insert(subject.clone(), row.clone());
        state.audit.push(audit_entry(
            subject,
            KeyAuditAction::Created,
            Some(kek_id.clone()),
            actor,
        ));
        Ok(row)
    }

    async fn shred(&self, subject: &SubjectRef, actor: &str) -> Result<ShredResult, CryptoError> {
        let mut state = self.lock();
        let now = Utc::now();
        let row = state.keys.entry(subject.clone()).or_insert_with(|| StoredSubjectKey {
            subject: subject.clone(),
            wrapped_dek: None,
            kek_id: None,
            created_at: now,
            shredded_at: None,
        });
        let result = match row.shredded_at {
            Some(at) => ShredResult { shredded_at: at, newly_shredded: false },
            None => {
                row.wrapped_dek = None;
                row.shredded_at = Some(now);
                ShredResult { shredded_at: now, newly_shredded: true }
            }
        };
        let kek_id = row.kek_id.clone();
        let action = if result.newly_shredded {
            KeyAuditAction::Shredded
        } else {
            KeyAuditAction::ShredRepeated
        };
        state.audit.push(audit_entry(subject, action, kek_id, actor));
        Ok(result)
    }

    async fn audit_log(&self, subject: &SubjectRef) -> Result<Vec<KeyAuditEntry>, CryptoError> {
        Ok(self.lock().audit.iter().filter(|e| &e.subject == subject).cloned().collect())
    }
}
