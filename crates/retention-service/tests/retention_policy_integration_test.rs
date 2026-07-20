//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use retention_service::{
    AuditLogReader, ChangeType, DataClass, PostgresAuditLogReader,
    PostgresRetentionPolicyRepository, RetentionPolicy, RetentionPolicyRepository,
    RetentionPolicyRepositoryError,
};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "retention_service")
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

fn sample_policy(tenant_id: Uuid) -> RetentionPolicy {
    RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days: 90,
        enabled: true,
    }
}

#[tokio::test]
async fn create_policy_writes_a_created_audit_row_in_the_same_transaction() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);

    repo.create(policy.clone(), "alice@example.com").await.expect("create should succeed");

    let found = repo.get(tenant_id, policy.id).await.unwrap();
    assert_eq!(found, Some(policy.clone()));

    let entries = audit_reader.list_for_entity(tenant_id, policy.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[0].entity_type, "retention_policy");
    // The audit actor must be the real user who performed the action, not the tenant id —
    // tenant_id is already its own column on every audit row, so reusing it as `actor` makes
    // the audit trail useless for "who did this" (CLAUDE.md §5).
    assert_eq!(entries[0].actor, "alice@example.com");
    assert_ne!(entries[0].actor, tenant_id.to_string());
}

#[tokio::test]
async fn retention_audit_log_rejects_update_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    repo.create(policy.clone(), "alice@example.com").await.unwrap();

    let err = sqlx::query("UPDATE retention_audit_log SET actor = 'tampered' WHERE entity_id = $1")
        .bind(policy.id)
        .execute(&pool)
        .await
        .expect_err("update should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}

#[tokio::test]
async fn retention_audit_log_rejects_delete_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    repo.create(policy.clone(), "alice@example.com").await.unwrap();

    let err = sqlx::query("DELETE FROM retention_audit_log WHERE entity_id = $1")
        .bind(policy.id)
        .execute(&pool)
        .await
        .expect_err("delete should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}

#[tokio::test]
async fn update_policy_writes_an_updated_audit_row_with_before_and_after() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    repo.create(policy.clone(), "alice@example.com").await.unwrap();

    let mut updated = policy.clone();
    updated.enabled = false;
    repo.update(updated.clone(), "bob@example.com").await.expect("update should succeed");

    let found = repo.get(tenant_id, policy.id).await.unwrap();
    assert_eq!(found, Some(updated));

    let entries = audit_reader.list_for_entity(tenant_id, policy.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    let update_entry = entries.iter().find(|e| e.change_type == ChangeType::Updated).unwrap();
    assert!(update_entry.before.is_some());
    assert_eq!(update_entry.actor, "bob@example.com");
}

#[tokio::test]
async fn a_failed_update_does_not_leave_a_partial_audit_row() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);

    let err = repo.update(policy.clone(), "alice@example.com").await.unwrap_err();
    assert!(matches!(err, RetentionPolicyRepositoryError::NotFound(_)));

    let entries = audit_reader.list_for_entity(tenant_id, policy.id).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn list_all_enabled_only_returns_enabled_policies() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let enabled = sample_policy(tenant_id);
    let mut disabled = sample_policy(tenant_id);
    disabled.data_class = DataClass::Event;
    disabled.enabled = false;
    repo.create(enabled.clone(), "alice@example.com").await.unwrap();
    repo.create(disabled, "alice@example.com").await.unwrap();

    let found = repo.list_all_enabled().await.unwrap();
    assert!(found.iter().any(|p| p.id == enabled.id));
    assert!(found.iter().all(|p| p.enabled));
}

#[tokio::test]
async fn delete_policy_writes_a_deleted_audit_row_and_removes_the_row() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    repo.create(policy.clone(), "alice@example.com").await.unwrap();

    repo.delete(tenant_id, policy.id, "carol@example.com").await.expect("delete should succeed");

    assert_eq!(repo.get(tenant_id, policy.id).await.unwrap(), None);

    let entries = audit_reader.list_for_entity(tenant_id, policy.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    let delete_entry = entries.iter().find(|e| e.change_type == ChangeType::Deleted).unwrap();
    assert!(delete_entry.before.is_some());
    assert_eq!(delete_entry.actor, "carol@example.com");
}

#[tokio::test]
async fn delete_of_unknown_policy_against_real_postgres_returns_not_found() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());

    let err = repo.delete(Uuid::new_v4(), Uuid::new_v4(), "alice@example.com").await.unwrap_err();
    assert!(matches!(err, RetentionPolicyRepositoryError::NotFound(_)));
}
