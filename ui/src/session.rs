#[path = "session_test.rs"]
#[cfg(test)]
pub(crate) mod session_test;

use async_trait::async_trait;
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
}

#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: Mutex<HashMap<String, Session>>,
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(&self, session: Session) -> String {
        let session_id = Uuid::new_v4().to_string();
        self.sessions.lock().unwrap().insert(session_id.clone(), session);
        session_id
    }

    async fn get(&self, session_id: &str) -> Option<Session> {
        self.sessions.lock().unwrap().get(session_id).cloned()
    }

    async fn delete(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }
}
