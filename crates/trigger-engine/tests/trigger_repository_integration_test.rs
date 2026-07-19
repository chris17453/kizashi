//! Integration test against real Postgres (CLAUDE.md §2), proving ADR-0018's `upsert` — the
//! write side of syncing Trigger Engine's own trigger definitions from config-admin-service's
//! `trigger.changed` messages — actually inserts and then replaces a row. Requires DATABASE_URL.

use common::{ThresholdDirection, TriggerCondition, TriggerDefinition};
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
