#[path = "session_audit_writer_test.rs"]
#[cfg(test)]
pub(crate) mod session_audit_writer_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SessionAuditWriterError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Console UI's session store is a purely in-memory, per-process map (ADR-0014's `SessionStore`
/// trait) -- it has no database of its own, so a revoked session leaves no durable trail unless
/// something else records it. Auth Service already owns `auth_audit_log` (user/branding
/// mutations), so a session revocation is recorded here too, under `entity_type = "session"`,
/// rather than standing up a new service just for this one write. Unlike the other
/// `*_repository.rs` audit writes, there's no accompanying row mutation in this service's own
/// tables -- the entity being audited (the session) lives in a different process entirely -- so
/// this is a write-only "record that this happened" call, not a repository method wrapping a
/// CRUD operation.
#[async_trait]
pub trait SessionAuditWriter: Send + Sync {
    async fn record_revocation(
        &self,
        tenant_id: Uuid,
        actor: &str,
        session_id: Uuid,
        revoked_username: &str,
    ) -> Result<(), SessionAuditWriterError>;
}

pub struct PostgresSessionAuditWriter {
    pool: sqlx::PgPool,
}

impl PostgresSessionAuditWriter {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionAuditWriter for PostgresSessionAuditWriter {
    async fn record_revocation(
        &self,
        tenant_id: Uuid,
        actor: &str,
        session_id: Uuid,
        revoked_username: &str,
    ) -> Result<(), SessionAuditWriterError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| SessionAuditWriterError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "session".to_string(),
                entity_id: session_id,
                change_type: ChangeType::Deleted,
                actor: actor.to_string(),
                before: None,
                after: serde_json::json!({"revoked_username": revoked_username}),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| SessionAuditWriterError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| SessionAuditWriterError::Backend(e.to_string()))?;
        Ok(())
    }
}
