//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use ingestion_gateway::{
    hash_api_key, ApiKeyStore, AuditLogReader, ChangeType, PostgresApiKeyStore,
    PostgresAuditLogReader,
};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "ingestion_gateway")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");
    pool
}

#[tokio::test]
async fn resolves_a_stored_key_to_its_tenant_and_rejects_unknown_or_revoked_keys() {
    let pool = test_pool().await;
    let store = PostgresApiKeyStore::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let api_key = format!("test-key-{}", Uuid::new_v4());
    let key_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO api_keys (id, tenant_id, key_hash, label, created_at) VALUES ($1, $2, $3, $4, now())",
    )
    .bind(key_id)
    .bind(tenant_id)
    .bind(hash_api_key(&api_key))
    .bind("integration-test-key")
    .execute(&pool)
    .await
    .expect("failed to insert api key");

    let resolved = store.tenant_for_key(&api_key).await.unwrap();
    assert_eq!(resolved, Some(tenant_id));

    let unknown = store.tenant_for_key("never-issued-key").await.unwrap();
    assert_eq!(unknown, None);

    sqlx::query("UPDATE api_keys SET revoked_at = now() WHERE id = $1")
        .bind(key_id)
        .execute(&pool)
        .await
        .expect("failed to revoke key");

    let revoked = store.tenant_for_key(&api_key).await.unwrap();
    assert_eq!(revoked, None, "a revoked key must no longer resolve to its tenant");
}

#[tokio::test]
async fn create_writes_a_created_audit_row_and_the_key_resolves() {
    let pool = test_pool().await;
    let store = PostgresApiKeyStore::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());
    let tenant_id = Uuid::new_v4();

    let (summary, plaintext) = store.create(tenant_id, "ci-agent", "alice").await.unwrap();

    assert_eq!(summary.label, "ci-agent");
    assert_eq!(store.tenant_for_key(&plaintext).await.unwrap(), Some(tenant_id));

    let entries = audit_reader.list_for_entity(tenant_id, summary.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[0].entity_type, "api_key");
    assert_eq!(
        entries[0].actor, "alice",
        "audit actor must be the real user who performed the action, not the tenant_id"
    );
    assert_ne!(
        entries[0].actor,
        tenant_id.to_string(),
        "audit actor must never fall back to the tenant_id"
    );
}

#[tokio::test]
async fn revoke_writes_a_deleted_audit_row_and_the_key_stops_resolving() {
    let pool = test_pool().await;
    let store = PostgresApiKeyStore::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let (summary, plaintext) = store.create(tenant_id, "to-revoke", "alice").await.unwrap();

    store.revoke(tenant_id, summary.id, "bob").await.unwrap();

    assert_eq!(store.tenant_for_key(&plaintext).await.unwrap(), None);
    let entries = audit_reader.list_for_entity(tenant_id, summary.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    let deleted = entries.iter().find(|e| e.change_type == ChangeType::Deleted).unwrap();
    assert_eq!(
        deleted.actor, "bob",
        "the Deleted audit row's actor must be the user who revoked the key"
    );
}

#[tokio::test]
async fn revoking_an_already_revoked_key_is_a_no_op_not_an_error() {
    let pool = test_pool().await;
    let store = PostgresApiKeyStore::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let (summary, _plaintext) = store.create(tenant_id, "double-revoke", "alice").await.unwrap();

    store.revoke(tenant_id, summary.id, "alice").await.unwrap();
    store.revoke(tenant_id, summary.id, "alice").await.unwrap();

    let entries = audit_reader.list_for_entity(tenant_id, summary.id).await.unwrap();
    assert_eq!(entries.len(), 2, "second revoke must not write a duplicate audit row");
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let pool = test_pool().await;
    let store = PostgresApiKeyStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    store.create(tenant_id, "mine", "alice").await.unwrap();
    store.create(Uuid::new_v4(), "not-mine", "bob").await.unwrap();

    let found = store.list(tenant_id).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].label, "mine");
}

#[tokio::test]
async fn ingestion_gateway_audit_log_rejects_update_and_delete_at_the_database_level() {
    let pool = test_pool().await;
    let store = PostgresApiKeyStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let (summary, _plaintext) =
        store.create(tenant_id, "immutability-check", "alice").await.unwrap();

    let update_err = sqlx::query(
        "UPDATE ingestion_gateway_audit_log SET actor = 'tampered' WHERE entity_id = $1",
    )
    .bind(summary.id)
    .execute(&pool)
    .await
    .expect_err("update should be rejected by the immutability trigger");
    assert!(update_err.to_string().contains("append-only"));

    let delete_err = sqlx::query("DELETE FROM ingestion_gateway_audit_log WHERE entity_id = $1")
        .bind(summary.id)
        .execute(&pool)
        .await
        .expect_err("delete should be rejected by the immutability trigger");
    assert!(delete_err.to_string().contains("append-only"));
}
