//! Integration test against real Postgres (CLAUDE.md §2), proving the local `agents` mirror
//! table (ADR-0020) actually upserts, lists only enabled sensors, tracks `last_polled_at`, and
//! deletes correctly. Requires DATABASE_URL.

use agent_scheduler::{PostgresSensorRepository, SensorRepository};
use common::Sensor;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "agent_scheduler")
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

fn sample_sensor(enabled: bool) -> Sensor {
    Sensor {
        enabled,
        ..Sensor::new(
            Uuid::new_v4(),
            "zendesk",
            "integration-test-sensor",
            serde_json::json!({"poll_interval_seconds": 60}),
        )
    }
}

#[tokio::test]
async fn upsert_then_list_enabled_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresSensorRepository::new(pool);
    let sensor = sample_sensor(true);

    repo.upsert(sensor.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert!(enabled.iter().any(|a| a.sensor.id == sensor.id && a.last_polled_at.is_none()));

    repo.delete(sensor.id).await.unwrap();
}

#[tokio::test]
async fn list_enabled_excludes_disabled_sensors_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresSensorRepository::new(pool);
    let sensor = sample_sensor(false);

    repo.upsert(sensor.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert!(!enabled.iter().any(|a| a.sensor.id == sensor.id));

    repo.delete(sensor.id).await.unwrap();
}

#[tokio::test]
async fn mark_polled_and_delete_work_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresSensorRepository::new(pool);
    let sensor = sample_sensor(true);
    repo.upsert(sensor.clone()).await.unwrap();

    let now = chrono::Utc::now();
    repo.mark_polled(sensor.id, now, Some("42".to_string())).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    let found = enabled.iter().find(|a| a.sensor.id == sensor.id).unwrap();
    assert!(found.last_polled_at.is_some());
    assert_eq!(found.last_checkpoint, Some("42".to_string()));

    repo.delete(sensor.id).await.unwrap();
    let enabled_after_delete = repo.list_enabled().await.unwrap();
    assert!(!enabled_after_delete.iter().any(|a| a.sensor.id == sensor.id));
}
