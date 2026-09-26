use super::*;
use crate::{
    CryptoError, DekCache, InMemorySubjectKeyStore, KeyAuditAction, LocalKeyProvider,
    SubjectKeyStore, SubjectRef,
};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn service_with_store() -> (SubjectCrypto, Arc<InMemorySubjectKeyStore>) {
    let store = Arc::new(InMemorySubjectKeyStore::default());
    let svc = SubjectCrypto::new(
        Arc::new(LocalKeyProvider::new([11; 32])),
        store.clone(),
        DekCache::new(Duration::from_secs(60)),
    );
    (svc, store)
}

fn subject() -> SubjectRef {
    SubjectRef::new(Uuid::new_v4(), "contact", "c-1")
}

#[tokio::test]
async fn encrypt_then_decrypt_round_trips_and_persists_only_a_wrapped_dek() {
    let (svc, store) = service_with_store();
    let s = subject();
    let ct = svc.encrypt(&s, b"alice@example.com").await.unwrap();
    assert_eq!(svc.decrypt(&s, &ct).await.unwrap(), b"alice@example.com");
    let row = store.get(&s).await.unwrap().unwrap();
    let wrapped = row.wrapped_dek.unwrap();
    assert!(wrapped.len() > 32, "stored DEK is wrapped, not raw");
}

#[tokio::test]
async fn decrypt_works_after_cache_eviction_by_unwrapping_from_store() {
    let (svc, _) = service_with_store();
    let s = subject();
    let ct = svc.encrypt(&s, b"x").await.unwrap();
    svc.evict(&s);
    assert_eq!(svc.decrypt(&s, &ct).await.unwrap(), b"x");
}

#[tokio::test]
async fn second_encrypt_reuses_the_same_subject_key() {
    let (svc, store) = service_with_store();
    let s = subject();
    let a = svc.encrypt(&s, b"a").await.unwrap();
    svc.evict(&s);
    let b = svc.encrypt(&s, b"b").await.unwrap();
    assert_eq!(svc.decrypt(&s, &a).await.unwrap(), b"a");
    assert_eq!(svc.decrypt(&s, &b).await.unwrap(), b"b");
    assert_eq!(store.audit_log(&s).await.unwrap().len(), 1, "one key creation");
}

#[tokio::test]
async fn decrypt_for_unknown_subject_is_key_not_found() {
    let (svc, _) = service_with_store();
    let err = svc.decrypt(&subject(), &[1, 2, 3]).await.unwrap_err();
    assert!(matches!(err, CryptoError::KeyNotFound));
}

#[tokio::test]
async fn shred_makes_decrypt_fail_with_typed_error_even_if_cached() {
    let (svc, store) = service_with_store();
    let s = subject();
    let ct = svc.encrypt(&s, b"secret").await.unwrap();
    let event = svc.shred(&s, "user:dpo").await.unwrap();
    assert_eq!(event.subject(), s);
    assert_eq!(event.actor, "user:dpo");
    let err = svc.decrypt(&s, &ct).await.unwrap_err();
    assert!(matches!(err, CryptoError::SubjectShredded { .. }), "{err:?}");
    let actions: Vec<_> =
        store.audit_log(&s).await.unwrap().into_iter().map(|e| e.action).collect();
    assert_eq!(actions, vec![KeyAuditAction::Created, KeyAuditAction::Shredded]);
}

#[tokio::test]
async fn encrypt_after_shred_is_refused() {
    let (svc, _) = service_with_store();
    let s = subject();
    svc.encrypt(&s, b"x").await.unwrap();
    svc.shred(&s, "user:dpo").await.unwrap();
    assert!(matches!(svc.encrypt(&s, b"new data").await, Err(CryptoError::SubjectShredded { .. })));
}

#[tokio::test]
async fn shred_is_idempotent_and_reports_original_timestamp() {
    let (svc, _) = service_with_store();
    let s = subject();
    svc.encrypt(&s, b"x").await.unwrap();
    let first = svc.shred(&s, "a").await.unwrap();
    let second = svc.shred(&s, "a").await.unwrap();
    assert_eq!(first.shredded_at, second.shredded_at);
}

#[tokio::test]
async fn shredding_one_subject_leaves_others_decryptable() {
    let (svc, _) = service_with_store();
    let tenant = Uuid::new_v4();
    let a = SubjectRef::new(tenant, "contact", "a");
    let b = SubjectRef::new(tenant, "contact", "b");
    svc.encrypt(&a, b"a").await.unwrap();
    let ct_b = svc.encrypt(&b, b"b").await.unwrap();
    svc.shred(&a, "ops").await.unwrap();
    assert_eq!(svc.decrypt(&b, &ct_b).await.unwrap(), b"b");
}

#[tokio::test]
async fn ciphertext_from_one_tenant_does_not_decrypt_under_another() {
    let (svc, _) = service_with_store();
    let a = SubjectRef::new(Uuid::new_v4(), "contact", "same");
    let b = SubjectRef::new(Uuid::new_v4(), "contact", "same");
    let ct = svc.encrypt(&a, b"a").await.unwrap();
    svc.encrypt(&b, b"b").await.unwrap();
    assert!(matches!(svc.decrypt(&b, &ct).await, Err(CryptoError::DecryptFailed)));
}

#[tokio::test]
async fn blind_indexer_differs_per_tenant_and_is_stable() {
    let (svc, _) = service_with_store();
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    let i1 = svc.blind_indexer(t1).await.unwrap().index_email("Bob@Example.com");
    let again = svc.blind_indexer(t1).await.unwrap().index_email("bob@example.com");
    let i2 = svc.blind_indexer(t2).await.unwrap().index_email("bob@example.com");
    assert_eq!(i1, again);
    assert_ne!(i1, i2);
}
