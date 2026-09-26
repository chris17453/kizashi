//! Real-Postgres integration tests for `PgSubjectKeyStore` (CLAUDE.md §2). Requires
//! DATABASE_URL (local runs point at `kizashi_test`, ADR-0077).

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use yocho_crypto::{
    migrator, CryptoError, DekCache, KekId, KeyAuditAction, LocalKeyProvider, PgSubjectKeyStore,
    SubjectCrypto, SubjectKeyStore, SubjectRef,
};

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&url, "yocho_crypto")
        .await
        .expect("failed to connect to postgres");
    migrator().run(&pool).await.expect("failed to run migrations");
    pool
}

fn subject(tenant: Uuid) -> SubjectRef {
    SubjectRef::new(tenant, "contact", format!("c-{}", Uuid::new_v4()))
}

async fn service(pool: sqlx::PgPool) -> SubjectCrypto {
    SubjectCrypto::new(
        Arc::new(LocalKeyProvider::new([21; 32])),
        Arc::new(PgSubjectKeyStore::new(pool)),
        DekCache::new(Duration::from_secs(60)),
    )
}

#[tokio::test]
async fn encrypt_decrypt_round_trips_through_postgres_after_cache_eviction() {
    let pool = test_pool().await;
    let svc = service(pool.clone()).await;
    let s = subject(Uuid::new_v4());
    let ct = svc.encrypt(&s, b"alice@example.com").await.unwrap();
    svc.evict(&s);
    assert_eq!(svc.decrypt(&s, &ct).await.unwrap(), b"alice@example.com");

    // A fresh service (cold cache, e.g. another node) can decrypt from the stored wrapped DEK.
    let other = service(pool).await;
    assert_eq!(other.decrypt(&s, &ct).await.unwrap(), b"alice@example.com");
}

#[tokio::test]
async fn shred_tombstones_row_audits_and_decrypt_fails_on_every_node() {
    let pool = test_pool().await;
    let svc = service(pool.clone()).await;
    let other_node = service(pool.clone()).await;
    let s = subject(Uuid::new_v4());
    let ct = svc.encrypt(&s, b"secret").await.unwrap();
    let event = svc.shred(&s, "user:dpo").await.unwrap();
    assert_eq!(event.subject(), s);

    let (wrapped, shredded_at): (Option<Vec<u8>>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT wrapped_dek, shredded_at FROM yocho_subject_keys
             WHERE tenant_id = $1 AND subject_type = $2 AND subject_id = $3",
        )
        .bind(s.tenant_id)
        .bind(&s.subject_type)
        .bind(&s.subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(wrapped.is_none(), "wrapped DEK must be destroyed");
    assert!(shredded_at.is_some());

    for node in [&svc, &other_node] {
        let err = node.decrypt(&s, &ct).await.unwrap_err();
        assert!(matches!(err, CryptoError::SubjectShredded { .. }), "{err:?}");
    }
    let store = PgSubjectKeyStore::new(pool);
    let actions: Vec<_> =
        store.audit_log(&s).await.unwrap().into_iter().map(|e| e.action).collect();
    assert_eq!(actions, vec![KeyAuditAction::Created, KeyAuditAction::Shredded]);
}

#[tokio::test]
async fn shred_is_idempotent_and_repeat_is_audited() {
    let pool = test_pool().await;
    let store = PgSubjectKeyStore::new(pool);
    let s = subject(Uuid::new_v4());
    store.insert_if_absent(&s, b"wrapped", &KekId::new("k"), "sys").await.unwrap();
    let first = store.shred(&s, "a").await.unwrap();
    let second = store.shred(&s, "b").await.unwrap();
    assert!(first.newly_shredded);
    assert!(!second.newly_shredded);
    assert_eq!(first.shredded_at, second.shredded_at);
    let audit = store.audit_log(&s).await.unwrap();
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[2].action, KeyAuditAction::ShredRepeated);
    assert_eq!(audit[2].actor, "b");
}

#[tokio::test]
async fn shred_of_unknown_subject_blocks_later_key_creation() {
    let pool = test_pool().await;
    let svc = service(pool).await;
    let s = subject(Uuid::new_v4());
    svc.shred(&s, "user:dpo").await.unwrap();
    assert!(matches!(svc.encrypt(&s, b"x").await, Err(CryptoError::SubjectShredded { .. })));
}

#[tokio::test]
async fn insert_if_absent_keeps_the_first_writer() {
    let pool = test_pool().await;
    let store = PgSubjectKeyStore::new(pool);
    let s = subject(Uuid::new_v4());
    store.insert_if_absent(&s, b"first", &KekId::new("k"), "sys").await.unwrap();
    let row = store.insert_if_absent(&s, b"second", &KekId::new("k"), "sys").await.unwrap();
    assert_eq!(row.wrapped_dek.as_deref(), Some(&b"first"[..]));
    assert_eq!(store.audit_log(&s).await.unwrap().len(), 1);
}

#[tokio::test]
async fn tenant_isolation_same_subject_id_in_two_tenants() {
    let pool = test_pool().await;
    let svc = service(pool.clone()).await;
    let store = PgSubjectKeyStore::new(pool);
    let id = format!("shared-{}", Uuid::new_v4());
    let a = SubjectRef::new(Uuid::new_v4(), "contact", id.clone());
    let b = SubjectRef::new(Uuid::new_v4(), "contact", id);
    let ct_a = svc.encrypt(&a, b"tenant a").await.unwrap();
    let ct_b = svc.encrypt(&b, b"tenant b").await.unwrap();

    assert!(matches!(svc.decrypt(&b, &ct_a).await, Err(CryptoError::DecryptFailed)));
    svc.shred(&a, "ops").await.unwrap();
    assert_eq!(svc.decrypt(&b, &ct_b).await.unwrap(), b"tenant b");
    assert!(store.get(&b).await.unwrap().unwrap().wrapped_dek.is_some());
    let b_actions: Vec<_> =
        store.audit_log(&b).await.unwrap().into_iter().map(|e| e.action).collect();
    assert_eq!(b_actions, vec![KeyAuditAction::Created], "tenant A's shred never touches B");
}

#[tokio::test]
async fn audit_table_rejects_update_delete_and_truncate() {
    let pool = test_pool().await;
    let store = PgSubjectKeyStore::new(pool.clone());
    let s = subject(Uuid::new_v4());
    store.insert_if_absent(&s, b"w", &KekId::new("k"), "sys").await.unwrap();

    let update = sqlx::query("UPDATE yocho_key_audit SET actor = 'forged' WHERE tenant_id = $1")
        .bind(s.tenant_id)
        .execute(&pool)
        .await;
    assert!(update.unwrap_err().to_string().contains("append-only"));
    let delete = sqlx::query("DELETE FROM yocho_key_audit WHERE tenant_id = $1")
        .bind(s.tenant_id)
        .execute(&pool)
        .await;
    assert!(delete.unwrap_err().to_string().contains("append-only"));
    let truncate = sqlx::query("TRUNCATE yocho_key_audit").execute(&pool).await;
    assert!(truncate.unwrap_err().to_string().contains("append-only"));
    assert_eq!(store.audit_log(&s).await.unwrap().len(), 1);
}

#[tokio::test]
async fn key_rows_cannot_be_deleted_or_unshredded() {
    let pool = test_pool().await;
    let store = PgSubjectKeyStore::new(pool.clone());
    let s = subject(Uuid::new_v4());
    store.insert_if_absent(&s, b"w", &KekId::new("k"), "sys").await.unwrap();

    let delete = sqlx::query("DELETE FROM yocho_subject_keys WHERE tenant_id = $1")
        .bind(s.tenant_id)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "live key rows cannot be deleted");
    let swap = sqlx::query("UPDATE yocho_subject_keys SET wrapped_dek = 'x' WHERE tenant_id = $1")
        .bind(s.tenant_id)
        .execute(&pool)
        .await;
    assert!(swap.is_err(), "wrapped DEK cannot be replaced in place");

    store.shred(&s, "ops").await.unwrap();
    let revive = sqlx::query(
        "UPDATE yocho_subject_keys SET wrapped_dek = 'x', shredded_at = NULL WHERE tenant_id = $1",
    )
    .bind(s.tenant_id)
    .execute(&pool)
    .await;
    assert!(revive.unwrap_err().to_string().contains("immutable"));
    let delete = sqlx::query("DELETE FROM yocho_subject_keys WHERE tenant_id = $1")
        .bind(s.tenant_id)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "tombstones cannot be deleted");
}
