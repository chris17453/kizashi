//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use ingestion_gateway::{hash_api_key, ApiKeyStore, PostgresApiKeyStore};
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
