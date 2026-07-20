use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySessionAuditWriter {
    pub recorded: Mutex<Vec<(Uuid, String, Uuid, String)>>,
}

#[async_trait]
impl SessionAuditWriter for InMemorySessionAuditWriter {
    async fn record_revocation(
        &self,
        tenant_id: Uuid,
        actor: &str,
        session_id: Uuid,
        revoked_username: &str,
    ) -> Result<(), SessionAuditWriterError> {
        self.recorded.lock().unwrap().push((
            tenant_id,
            actor.to_string(),
            session_id,
            revoked_username.to_string(),
        ));
        Ok(())
    }
}

pub struct FailingSessionAuditWriter;

#[async_trait]
impl SessionAuditWriter for FailingSessionAuditWriter {
    async fn record_revocation(
        &self,
        _tenant_id: Uuid,
        _actor: &str,
        _session_id: Uuid,
        _revoked_username: &str,
    ) -> Result<(), SessionAuditWriterError> {
        Err(SessionAuditWriterError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_writer_records_the_revocation() {
    let writer = InMemorySessionAuditWriter::default();
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    writer.record_revocation(tenant_id, "admin@example.com", session_id, "bob").await.unwrap();

    let recorded = writer.recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, tenant_id);
    assert_eq!(recorded[0].1, "admin@example.com");
    assert_eq!(recorded[0].2, session_id);
    assert_eq!(recorded[0].3, "bob");
}

#[tokio::test]
async fn failing_writer_returns_backend_error() {
    let writer = FailingSessionAuditWriter;
    let err = writer
        .record_revocation(Uuid::new_v4(), "admin@example.com", Uuid::new_v4(), "bob")
        .await
        .unwrap_err();
    assert!(matches!(err, SessionAuditWriterError::Backend(_)));
}
