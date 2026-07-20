//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use auth_service::{
    hash_password, AuditLogReader, LocalUserRepository, PostgresLocalUserRepository,
};
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

fn sample_user(tenant_id: Uuid) -> auth_service::LocalUser {
    auth_service::LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: format!("user-{}", Uuid::new_v4()),
        password_hash: hash_password("correct-horse-battery-staple").unwrap(),
        role: Role::Operator,
    }
}

#[tokio::test]
async fn create_then_list_returns_the_user_scoped_to_tenant() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);

    let created = repo.create(user.clone(), "test-actor").await.unwrap();
    assert_eq!(created, user);

    let listed = repo.list(tenant_id).await.unwrap();
    assert_eq!(listed, vec![user]);

    let other_tenant = repo.list(Uuid::new_v4()).await.unwrap();
    assert!(!other_tenant.iter().any(|u| u.id == created.id));
}

#[tokio::test]
async fn create_writes_an_audit_row_with_the_real_actor_not_the_tenant_id() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());
    let audit_reader = auth_service::PostgresAuditLogReader::new(pool);
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);

    repo.create(user.clone(), "alice-the-admin").await.unwrap();

    let entries = audit_reader.list_for_entity(tenant_id, user.id).await.unwrap();
    assert_eq!(entries.len(), 1, "expected exactly the created row");
    assert_eq!(entries[0].actor, "alice-the-admin");
    assert_ne!(
        entries[0].actor,
        tenant_id.to_string(),
        "actor must be the real actor, not the tenant_id"
    );
}

#[tokio::test]
async fn update_role_persists_against_real_postgres_and_writes_an_audit_row() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());
    let audit_reader = auth_service::PostgresAuditLogReader::new(pool);
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
    repo.create(user.clone(), "test-actor").await.unwrap();

    let updated = repo.update_role(tenant_id, user.id, Role::Admin, "test-actor").await.unwrap();
    assert_eq!(updated.role, Role::Admin);

    let found = repo.find_by_username(tenant_id, &user.username).await.unwrap().unwrap();
    assert_eq!(found.role, Role::Admin);

    let entries = audit_reader.list_for_entity(tenant_id, user.id).await.unwrap();
    assert_eq!(entries.len(), 2, "expected a created row and an updated row");
    assert_eq!(entries[1].actor, "test-actor");
}

#[tokio::test]
async fn delete_removes_the_user_and_is_scoped_to_tenant() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
    repo.create(user.clone(), "test-actor").await.unwrap();

    let wrong_tenant_result = repo.delete(Uuid::new_v4(), user.id, "test-actor").await;
    assert!(matches!(
        wrong_tenant_result,
        Err(auth_service::LocalUserRepositoryError::NotFound(_))
    ));
    assert!(repo.find_by_username(tenant_id, &user.username).await.unwrap().is_some());

    repo.delete(tenant_id, user.id, "test-actor").await.unwrap();
    assert!(repo.find_by_username(tenant_id, &user.username).await.unwrap().is_none());
}

#[tokio::test]
async fn auth_audit_log_rejects_delete_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
    repo.create(user.clone(), "test-actor").await.unwrap();

    let result = sqlx::query("DELETE FROM auth_audit_log WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await;
    assert!(result.is_err(), "auth_audit_log must reject DELETE at the database level");
}
