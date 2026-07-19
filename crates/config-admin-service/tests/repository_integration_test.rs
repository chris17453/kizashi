//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.
//!
//! Exercises the transactional create/update + audit-log-write behavior that the in-memory
//! test doubles used for handler unit tests can't: `record_audit_entry` writing a real row in
//! the same Postgres transaction as the entity change.

use common::TriggerCondition;
use config_admin_service::{
    AuditLogReader, ChangeType, NormalizationMappingRepository, PostgresAuditLogReader,
    PostgresNormalizationMappingRepository, PostgresTriggerDefinitionRepository,
    TriggerDefinitionRepository,
};
use std::collections::BTreeMap;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "config_admin_service")
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

fn sample_trigger(tenant_id: Uuid) -> common::TriggerDefinition {
    common::TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "high-volume-negative".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 3 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn create_trigger_writes_a_created_audit_row_in_the_same_transaction() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);

    repo.create(trigger.clone()).await.expect("create should succeed");

    let found = repo.get(tenant_id, trigger.id).await.unwrap();
    assert_eq!(found, Some(trigger.clone()));

    let entries = audit_reader.list_for_entity(tenant_id, trigger.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[0].entity_type, "trigger_definition");
    assert!(entries[0].before.is_none());
}

#[tokio::test]
async fn update_trigger_writes_an_updated_audit_row_with_before_and_after() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    repo.create(trigger.clone()).await.unwrap();

    let mut updated = trigger.clone();
    updated.enabled = false;
    repo.update(updated.clone()).await.expect("update should succeed");

    let found = repo.get(tenant_id, trigger.id).await.unwrap();
    assert_eq!(found, Some(updated));

    let entries = audit_reader.list_for_entity(tenant_id, trigger.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    let update_entry = entries.iter().find(|e| e.change_type == ChangeType::Updated).unwrap();
    assert!(update_entry.before.is_some());
}

#[tokio::test]
async fn a_failed_update_does_not_leave_a_partial_audit_row() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);

    let err = repo.update(trigger.clone()).await.unwrap_err();
    assert!(matches!(err, config_admin_service::TriggerDefinitionRepositoryError::NotFound(_)));

    let entries = audit_reader.list_for_entity(tenant_id, trigger.id).await.unwrap();
    assert!(entries.is_empty());
}

fn sample_mapping(tenant_id: Uuid) -> common::NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    common::NormalizationMapping::new(tenant_id, "ticket", field_map)
}

#[tokio::test]
async fn config_audit_log_rejects_update_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    repo.create(trigger.clone()).await.unwrap();

    let err = sqlx::query("UPDATE config_audit_log SET actor = 'tampered' WHERE entity_id = $1")
        .bind(trigger.id)
        .execute(&pool)
        .await
        .expect_err("update should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}

#[tokio::test]
async fn config_audit_log_rejects_delete_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    repo.create(trigger.clone()).await.unwrap();

    let err = sqlx::query("DELETE FROM config_audit_log WHERE entity_id = $1")
        .bind(trigger.id)
        .execute(&pool)
        .await
        .expect_err("delete should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}

#[tokio::test]
async fn create_mapping_writes_a_created_audit_row_in_the_same_transaction() {
    let pool = test_pool().await;
    let repo = PostgresNormalizationMappingRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);

    repo.create(mapping.clone()).await.expect("create should succeed");

    let found = repo.get(tenant_id, mapping.id).await.unwrap();
    assert_eq!(found, Some(mapping.clone()));

    let entries = audit_reader.list_for_entity(tenant_id, mapping.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[0].entity_type, "normalization_mapping");
}
