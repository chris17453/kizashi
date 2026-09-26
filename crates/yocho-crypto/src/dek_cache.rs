//! In-memory, TTL-bounded cache of unwrapped DEKs (ADR-0204 §3) so the key store and KMS stay
//! off the hot path. Plaintext DEKs live only here, in process memory, and are zeroed when the
//! last `Arc` drops. Nothing is ever written to disk.
//!
//! A shred on another node is propagated by the `subject.shredded` event (consumers call
//! [`DekCache::evict`]); the TTL bounds the window if that event is delayed.

#[path = "dek_cache_test.rs"]
#[cfg(test)]
mod dek_cache_test;

use crate::{Dek, SubjectRef};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Default upper bound on cached DEKs; sized for thousands of mailboxes/contacts per node.
pub const DEFAULT_MAX_ENTRIES: usize = 100_000;

pub struct DekCache {
    ttl: Duration,
    max_entries: usize,
    entries: Mutex<HashMap<SubjectRef, (Arc<Dek>, Instant)>>,
}

impl DekCache {
    pub fn new(ttl: Duration) -> Self {
        Self::with_capacity(ttl, DEFAULT_MAX_ENTRIES)
    }

    pub fn with_capacity(ttl: Duration, max_entries: usize) -> Self {
        Self { ttl, max_entries: max_entries.max(1), entries: Mutex::new(HashMap::new()) }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<SubjectRef, (Arc<Dek>, Instant)>> {
        // A poisoned lock only means another thread panicked mid-operation; the map itself is
        // still a valid cache, so recover rather than propagate the panic.
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn get(&self, subject: &SubjectRef) -> Option<Arc<Dek>> {
        let mut entries = self.lock();
        match entries.get(subject) {
            Some((dek, inserted)) if inserted.elapsed() < self.ttl => Some(dek.clone()),
            Some(_) => {
                entries.remove(subject);
                None
            }
            None => None,
        }
    }

    pub fn insert(&self, subject: SubjectRef, dek: Arc<Dek>) {
        let mut entries = self.lock();
        if !entries.contains_key(&subject) && entries.len() >= self.max_entries {
            let ttl = self.ttl;
            entries.retain(|_, (_, inserted)| inserted.elapsed() < ttl);
            if entries.len() >= self.max_entries {
                let oldest = entries.iter().min_by_key(|(_, (_, at))| *at).map(|(k, _)| k.clone());
                if let Some(oldest) = oldest {
                    entries.remove(&oldest);
                }
            }
        }
        entries.insert(subject, (dek, Instant::now()));
    }

    pub fn evict(&self, subject: &SubjectRef) {
        self.lock().remove(subject);
    }

    pub fn evict_tenant(&self, tenant_id: Uuid) {
        self.lock().retain(|s, _| s.tenant_id != tenant_id);
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
