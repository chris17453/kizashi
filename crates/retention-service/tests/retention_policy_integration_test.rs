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

    repo.create(policy.clone()).await.expect("create should succeed");

    let found = repo.get(tenant_id, policy.id).await.unwrap();
    assert_eq!(found, Some(policy.clone()));

    let entries = audit_reader.list_for_entity(tenant_id, policy.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[0].entity_type, "retention_policy");
}

#[tokio::test]
async fn update_policy_writes_an_updated_audit_row_with_before_and_after() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    repo.create(policy.clone()).await.unwrap();

    let mut updated = policy.clone();
    updated.enabled = false;
    repo.update(updated.clone()).await.expect("update should succeed");

    let found = repo.get(tenant_id, policy.id).await.unwrap();
    assert_eq!(found, Some(updated));

    let entries = audit_reader.list_for_entity(tenant_id, policy.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    let update_entry = entries.iter().find(|e| e.change_type == ChangeType::Updated).unwrap();
    assert!(update_entry.before.is_some());
}

#[tokio::test]
async fn a_failed_update_does_not_leave_a_partial_audit_row() {
    let pool = test_pool().await;
    let repo = PostgresRetentionPolicyRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);

    let err = repo.update(policy.clone()).await.unwrap_err();
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
    repo.create(enabled.clone()).await.unwrap();
    repo.create(disabled).await.unwrap();

    let found = repo.list_all_enabled().await.unwrap();
    assert!(found.iter().any(|p| p.id == enabled.id));
    assert!(found.iter().all(|p| p.enabled));
}
