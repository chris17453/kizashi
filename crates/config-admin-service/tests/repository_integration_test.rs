//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.
//!
//! Exercises the transactional create/update + audit-log-write behavior that the in-memory
//! test doubles used for handler unit tests can't: `record_audit_entry` writing a real row in
//! the same Postgres transaction as the entity change.

use common::{AnalysisConfig, Sensor, TriggerCondition};
use config_admin_service::{
    AnalysisConfigRepository, AuditLogReader, ChangeType, NormalizationMappingRepository,
    PostgresAnalysisConfigRepository, PostgresAuditLogReader,
    PostgresNormalizationMappingRepository, PostgresSensorRepository,
    PostgresTriggerDefinitionRepository, SensorRepository, TriggerDefinitionRepository,
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

#[tokio::test]
async fn upsert_analysis_config_writes_created_then_updated_audit_rows_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAnalysisConfigRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());
    let tenant_id = Uuid::new_v4();

    repo.upsert(AnalysisConfig::new(tenant_id, "look for urgent tickets")).await.unwrap();
    let found = repo.get(tenant_id).await.unwrap();
    assert_eq!(found.map(|c| c.prompt), Some("look for urgent tickets".to_string()));

    repo.upsert(AnalysisConfig::new(tenant_id, "flag policy violations")).await.unwrap();
    let found = repo.get(tenant_id).await.unwrap();
    assert_eq!(found.map(|c| c.prompt), Some("flag policy violations".to_string()));

    let entries = audit_reader.list_for_entity(tenant_id, tenant_id).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[1].change_type, ChangeType::Updated);
    assert_eq!(entries[0].entity_type, "analysis_config");
}

// --- Tenant isolation (CLAUDE.md §5: "every query path must be tested for tenant isolation,
// not just implemented correctly by inspection") — every repository below is queried from
// tenant B's context for a row that actually belongs to tenant A, against real Postgres.

#[tokio::test]
async fn a_trigger_owned_by_one_tenant_is_invisible_to_get_from_a_different_tenant() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let trigger = sample_trigger(tenant_a);
    repo.create(trigger.clone()).await.unwrap();

    assert_eq!(repo.get(tenant_a, trigger.id).await.unwrap(), Some(trigger.clone()));
    assert_eq!(repo.get(tenant_b, trigger.id).await.unwrap(), None);
}

#[tokio::test]
async fn updating_a_trigger_owned_by_another_tenant_fails_as_not_found() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let trigger = sample_trigger(tenant_a);
    repo.create(trigger.clone()).await.unwrap();

    let mut cross_tenant_update = trigger.clone();
    cross_tenant_update.tenant_id = tenant_b;
    cross_tenant_update.enabled = false;
    let err = repo.update(cross_tenant_update).await.unwrap_err();
    assert!(matches!(err, config_admin_service::TriggerDefinitionRepositoryError::NotFound(_)));

    // the original, owned by tenant_a, must be unchanged
    assert_eq!(repo.get(tenant_a, trigger.id).await.unwrap(), Some(trigger));
}

#[tokio::test]
async fn listing_triggers_for_one_tenant_never_returns_another_tenants_rows() {
    let pool = test_pool().await;
    let repo = PostgresTriggerDefinitionRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    repo.create(sample_trigger(tenant_a)).await.unwrap();
    repo.create(sample_trigger(tenant_b)).await.unwrap();

    let tenant_a_triggers = repo.list(tenant_a, 100, 0).await.unwrap();
    assert_eq!(tenant_a_triggers.len(), 1);
    assert_eq!(tenant_a_triggers[0].tenant_id, tenant_a);
}

#[tokio::test]
async fn a_normalization_mapping_owned_by_one_tenant_is_invisible_to_get_from_a_different_tenant() {
    let pool = test_pool().await;
    let repo = PostgresNormalizationMappingRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let mapping = sample_mapping(tenant_a);
    repo.create(mapping.clone()).await.unwrap();

    assert_eq!(repo.get(tenant_a, mapping.id).await.unwrap(), Some(mapping.clone()));
    assert_eq!(repo.get(tenant_b, mapping.id).await.unwrap(), None);
}

#[tokio::test]
async fn listing_normalization_mappings_for_one_tenant_never_returns_another_tenants_rows() {
    let pool = test_pool().await;
    let repo = PostgresNormalizationMappingRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    repo.create(sample_mapping(tenant_a)).await.unwrap();
    repo.create(sample_mapping(tenant_b)).await.unwrap();

    let tenant_a_mappings = repo.list(tenant_a).await.unwrap();
    assert_eq!(tenant_a_mappings.len(), 1);
    assert_eq!(tenant_a_mappings[0].tenant_id, tenant_a);
}

fn sample_sensor(tenant_id: Uuid) -> Sensor {
    Sensor::new(tenant_id, "generic", "isolation-test-sensor", serde_json::json!({}))
}

#[tokio::test]
async fn a_sensor_owned_by_one_tenant_is_invisible_to_get_from_a_different_tenant() {
    let pool = test_pool().await;
    let repo = PostgresSensorRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let sensor = sample_sensor(tenant_a);
    repo.create(sensor.clone()).await.unwrap();

    assert_eq!(repo.get(tenant_a, sensor.id).await.unwrap(), Some(sensor.clone()));
    assert_eq!(repo.get(tenant_b, sensor.id).await.unwrap(), None);
}

#[tokio::test]
async fn deleting_a_sensor_owned_by_another_tenant_fails_and_leaves_it_intact() {
    let pool = test_pool().await;
    let repo = PostgresSensorRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let sensor = sample_sensor(tenant_a);
    repo.create(sensor.clone()).await.unwrap();

    let err = repo.delete(tenant_b, sensor.id).await.unwrap_err();
    assert!(matches!(err, config_admin_service::SensorRepositoryError::NotFound(_)));
    assert_eq!(repo.get(tenant_a, sensor.id).await.unwrap(), Some(sensor));
}

#[tokio::test]
async fn find_by_name_never_crosses_tenant_boundaries_even_with_a_matching_name() {
    let pool = test_pool().await;
    let repo = PostgresSensorRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    // Both tenants register a sensor with the identical name - a realistic collision since
    // connector_id/name is operator-chosen, not globally unique.
    let sensor_a = sample_sensor(tenant_a);
    let mut sensor_b = sample_sensor(tenant_b);
    sensor_b.name = sensor_a.name.clone();
    repo.create(sensor_a.clone()).await.unwrap();
    repo.create(sensor_b.clone()).await.unwrap();

    let found = repo.find_by_name(tenant_a, &sensor_a.name).await.unwrap();
    assert_eq!(found.map(|a| a.id), Some(sensor_a.id));
}

#[tokio::test]
async fn an_analysis_config_owned_by_one_tenant_is_invisible_to_get_from_a_different_tenant() {
    let pool = test_pool().await;
    let repo = PostgresAnalysisConfigRepository::new(pool.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    repo.upsert(AnalysisConfig::new(tenant_a, "tenant a's prompt")).await.unwrap();

    let found_a = repo.get(tenant_a).await.unwrap();
    assert_eq!(found_a.map(|c| c.prompt), Some("tenant a's prompt".to_string()));
    assert_eq!(repo.get(tenant_b).await.unwrap(), None);
}
