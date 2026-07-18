//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use query_gateway::{hash_token, PostgresTokenStore, TokenStore};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "query_gateway")
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
async fn resolves_a_stored_token_to_its_tenant_and_rejects_unknown_or_revoked_tokens() {
    let pool = test_pool().await;
    let store = PostgresTokenStore::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let token = format!("test-token-{}", Uuid::new_v4());
    let token_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO query_api_tokens (id, tenant_id, token_hash, label, created_at) VALUES ($1, $2, $3, $4, now())",
    )
    .bind(token_id)
    .bind(tenant_id)
    .bind(hash_token(&token))
    .bind("integration-test-token")
    .execute(&pool)
    .await
    .expect("failed to insert token");

    let resolved = store.tenant_for_token(&token).await.unwrap();
    assert_eq!(resolved, Some(tenant_id));

    let unknown = store.tenant_for_token("never-issued-token").await.unwrap();
    assert_eq!(unknown, None);

    sqlx::query("UPDATE query_api_tokens SET revoked_at = now() WHERE id = $1")
        .bind(token_id)
        .execute(&pool)
        .await
        .expect("failed to revoke token");

    let revoked = store.tenant_for_token(&token).await.unwrap();
    assert_eq!(revoked, None, "a revoked token must no longer resolve to its tenant");
}
