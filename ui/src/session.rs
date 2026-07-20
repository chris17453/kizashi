#[path = "session_test.rs"]
#[cfg(test)]
pub(crate) mod session_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::Role;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub bearer_token: String,
    pub tenant_id: Uuid,
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
}

/// Auth Service has no session/cookie layer of its own (ADR-0009 — "that's Console UI's job
/// once built"); this is that job, kept as simple as correctness allows (ADR-0014): an
/// in-memory map keyed by a random session id set as an `HttpOnly` cookie, not a signed/JWT
/// scheme, since the UI process doesn't need distributed session validation for v1.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, session: Session) -> String;
    async fn get(&self, session_id: &str) -> Option<Session>;
    async fn delete(&self, session_id: &str);

    /// Every active session for a tenant, session id alongside session — powers the Console
    /// UI's `/security/sessions` admin page (ADR-0046). Single-instance-only, same as the rest
    /// of this in-memory store (ADR-0014): a multi-replica UI deployment would need a shared
    /// session backend before this can list sessions started on a different instance.
    async fn list_for_tenant(&self, tenant_id: Uuid) -> Vec<(String, Session)>;
}

/// A default enterprise idle-timeout: 30 minutes of no activity signs the session out. This is
/// a genuine gap, not a knob nobody needs -- until this change, a session lived until explicit
/// logout, admin revoke, or a process restart, no matter how long a browser tab sat idle.
const DEFAULT_IDLE_TIMEOUT_MINUTES: i64 = 30;

/// `last_active_at` is tracked alongside `Session` here, in the store, rather than as a field
/// on `Session` itself -- `Session` is constructed directly (not via a builder) across every
/// handler test in this crate, and adding a required field to it would mean touching every one
/// of those call sites for a concern that's purely about session-store bookkeeping, not the
/// session's own identity/claims.
pub struct InMemorySessionStore {
    sessions: Mutex<HashMap<String, (Session, DateTime<Utc>)>>,
    idle_timeout: chrono::Duration,
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            idle_timeout: chrono::Duration::minutes(DEFAULT_IDLE_TIMEOUT_MINUTES),
        }
    }
}

impl InMemorySessionStore {
    pub fn with_idle_timeout(idle_timeout: chrono::Duration) -> Self {
        Self { sessions: Mutex::new(HashMap::new()), idle_timeout }
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(&self, session: Session) -> String {
        let session_id = Uuid::new_v4().to_string();
        self.sessions.lock().unwrap().insert(session_id.clone(), (session, Utc::now()));
        session_id
    }

    /// Sliding idle timeout: every successful `get()` both checks whether the session has been
    /// idle longer than `idle_timeout` (deleting and returning `None` if so) and, if not,
    /// refreshes `last_active_at` to now -- continued activity keeps a session alive
    /// indefinitely, only genuine idleness expires it.
    async fn get(&self, session_id: &str) -> Option<Session> {
        let mut sessions = self.sessions.lock().unwrap();
        let expired = match sessions.get(session_id) {
            Some((_, last_active_at)) => Utc::now() - *last_active_at > self.idle_timeout,
            None => return None,
        };
        if expired {
            sessions.remove(session_id);
            return None;
        }
        let entry = sessions.get_mut(session_id).unwrap();
        entry.1 = Utc::now();
        Some(entry.0.clone())
    }

    async fn delete(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }

    /// Also prunes expired sessions as a side effect -- without this, `/security/sessions`
    /// (ADR-0046) would keep showing an idled-out session as "active" until something else
    /// happened to touch it via `get()`.
    async fn list_for_tenant(&self, tenant_id: Uuid) -> Vec<(String, Session)> {
        let now = Utc::now();
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, (_, last_active_at)| now - *last_active_at <= self.idle_timeout);
        sessions
            .iter()
            .filter(|(_, (session, _))| session.tenant_id == tenant_id)
            .map(|(id, (session, _))| (id.clone(), session.clone()))
            .collect()
    }
}
