use super::*;
use crate::{KekId, KeyAuditAction, SubjectKeyStore, SubjectRef};
use uuid::Uuid;

fn subject() -> SubjectRef {
    SubjectRef::new(Uuid::new_v4(), "contact", "c-1")
}

#[tokio::test]
async fn insert_if_absent_keeps_first_writer_and_audits_once() {
    let store = InMemorySubjectKeyStore::default();
    let s = subject();
    let kek = KekId::new("local:v1:x");
    let first = store.insert_if_absent(&s, b"one", &kek, "sys").await.unwrap();
    let second = store.insert_if_absent(&s, b"two", &kek, "sys").await.unwrap();
    assert_eq!(first.wrapped_dek.as_deref(), Some(&b"one"[..]));
    assert_eq!(second.wrapped_dek.as_deref(), Some(&b"one"[..]));
    let audit = store.audit_log(&s).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, KeyAuditAction::Created);
}

#[tokio::test]
async fn shred_tombstones_and_audits_and_is_idempotent() {
    let store = InMemorySubjectKeyStore::default();
    let s = subject();
    store.insert_if_absent(&s, b"one", &KekId::new("k"), "sys").await.unwrap();
    let first = store.shred(&s, "user:ops").await.unwrap();
    assert!(first.newly_shredded);
    let second = store.shred(&s, "user:ops").await.unwrap();
    assert!(!second.newly_shredded);
    assert_eq!(first.shredded_at, second.shredded_at);
    let row = store.get(&s).await.unwrap().unwrap();
    assert!(row.wrapped_dek.is_none());
    assert!(row.is_shredded());
    let actions: Vec<_> =
        store.audit_log(&s).await.unwrap().into_iter().map(|e| e.action).collect();
    assert_eq!(
        actions,
        vec![KeyAuditAction::Created, KeyAuditAction::Shredded, KeyAuditAction::ShredRepeated]
    );
}

#[tokio::test]
async fn shred_of_unknown_subject_leaves_a_tombstone_that_blocks_key_creation() {
    let store = InMemorySubjectKeyStore::default();
    let s = subject();
    assert!(store.shred(&s, "user:ops").await.unwrap().newly_shredded);
    let row = store.insert_if_absent(&s, b"new", &KekId::new("k"), "sys").await.unwrap();
    assert!(row.is_shredded());
    assert!(row.wrapped_dek.is_none());
}

#[tokio::test]
async fn rows_are_tenant_scoped() {
    let store = InMemorySubjectKeyStore::default();
    let a = SubjectRef::new(Uuid::new_v4(), "contact", "same");
    let b = SubjectRef::new(Uuid::new_v4(), "contact", "same");
    store.insert_if_absent(&a, b"a", &KekId::new("k"), "sys").await.unwrap();
    assert!(store.get(&b).await.unwrap().is_none());
    assert!(store.audit_log(&b).await.unwrap().is_empty());
}
