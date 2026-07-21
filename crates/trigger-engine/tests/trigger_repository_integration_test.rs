//! Integration test against real Postgres (CLAUDE.md §2), proving ADR-0018's `upsert` — the
//! write side of syncing Trigger Engine's own trigger definitions from config-admin-service's
//! `trigger.changed` messages — actually inserts and then replaces a row. Requires DATABASE_URL.

use common::{CorrelatedCondition, ThresholdDirection, TriggerCondition, TriggerDefinition};
use trigger_engine::{PostgresTriggerRepository, TriggerRepository};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "trigger_engine")
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

fn sample_trigger(id: Uuid, tenant_id: Uuid) -> TriggerDefinition {
    TriggerDefinition {
        id,
        tenant_id,
        name: "upsert-integration-test".to_string(),
        event_type_match: "priority_score".to_string(),
        condition: TriggerCondition::ThresholdOverWindow {
            field: "priority_score".to_string(),
            threshold: 5.0,
            direction: ThresholdDirection::Above,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn upsert_inserts_a_new_trigger_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresTriggerRepository::new(pool);
    let trigger = sample_trigger(Uuid::new_v4(), Uuid::new_v4());

    repo.upsert(trigger.clone()).await.unwrap();

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn upsert_replaces_an_existing_trigger_by_id_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresTriggerRepository::new(pool);
    let trigger = sample_trigger(Uuid::new_v4(), Uuid::new_v4());
    repo.upsert(trigger.clone()).await.unwrap();

    let mut updated = trigger.clone();
    updated.name = "renamed-via-upsert".to_string();
    updated.enabled = false;
    repo.upsert(updated.clone()).await.unwrap();

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert_eq!(found, Some(updated));
}

#[tokio::test]
async fn delete_removes_a_trigger_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresTriggerRepository::new(pool);
    let trigger = sample_trigger(Uuid::new_v4(), Uuid::new_v4());
    repo.upsert(trigger.clone()).await.unwrap();

    repo.delete(trigger.id).await.unwrap();

    let found = repo.get_by_id(trigger.id).await.unwrap();
    assert!(found.is_none());
}

fn correlated_trigger(id: Uuid, tenant_id: Uuid) -> TriggerDefinition {
    TriggerDefinition {
        id,
        tenant_id,
        name: "email-and-chat-integration-test".to_string(),
        event_type_match: "sentiment_drop_email".to_string(),
        condition: TriggerCondition::CorrelatedOverWindow {
            conditions: vec![
                CorrelatedCondition {
                    event_type: "sentiment_drop_email".to_string(),
                    min_count: 1,
                },
                CorrelatedCondition { event_type: "unresolved_chat".to_string(), min_count: 1 },
            ],
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn a_correlated_trigger_is_found_by_any_of_its_listed_event_types_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresTriggerRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let trigger = correlated_trigger(Uuid::new_v4(), tenant_id);
    repo.upsert(trigger.clone()).await.unwrap();

    let by_email = repo.active_triggers_for(tenant_id, "sentiment_drop_email").await.unwrap();
    assert_eq!(by_email, vec![trigger.clone()]);

    let by_chat = repo.active_triggers_for(tenant_id, "unresolved_chat").await.unwrap();
    assert_eq!(by_chat, vec![trigger]);

    let unrelated = repo.active_triggers_for(tenant_id, "totally_unrelated").await.unwrap();
    assert!(unrelated.is_empty());
}

#[tokio::test]
async fn a_correlated_trigger_does_not_leak_across_tenants_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresTriggerRepository::new(pool);
    let trigger = correlated_trigger(Uuid::new_v4(), Uuid::new_v4());
    repo.upsert(trigger).await.unwrap();

    let found = repo.active_triggers_for(Uuid::new_v4(), "unresolved_chat").await.unwrap();
    assert!(found.is_empty());
}
