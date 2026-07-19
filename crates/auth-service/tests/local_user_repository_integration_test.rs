//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use auth_service::{hash_password, LocalUserRepository, PostgresLocalUserRepository};
use common::Role;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "auth_service")
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
async fn finds_a_stored_user_and_scopes_by_tenant() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let password_hash = hash_password("correct-horse-battery-staple").unwrap();

    sqlx::query(
        "INSERT INTO local_users (id, tenant_id, username, password_hash, role) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind("alice")
    .bind(&password_hash)
    .bind("operator")
    .execute(&pool)
    .await
    .expect("failed to insert local user");

    let found = repo.find_by_username(tenant_id, "alice").await.unwrap().unwrap();
    assert_eq!(found.password_hash, password_hash);
    assert_eq!(found.role, Role::Operator);

    let wrong_tenant = repo.find_by_username(Uuid::new_v4(), "alice").await.unwrap();
    assert!(wrong_tenant.is_none());
}
