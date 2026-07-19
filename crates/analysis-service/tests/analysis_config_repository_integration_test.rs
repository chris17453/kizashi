//! Integration test against real Postgres (CLAUDE.md §2), proving ADR-0019's local
//! `analysis_configs` table (analysis-service's first-ever Postgres schema) actually upserts
//! and reads back a tenant's AI prompt. Requires DATABASE_URL.

use analysis_service::{AnalysisConfigRepository, PostgresAnalysisConfigRepository};
use common::AnalysisConfig;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "analysis_service")
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
async fn upsert_then_get_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAnalysisConfigRepository::new(pool);
    let config = AnalysisConfig::new(Uuid::new_v4(), "look for urgent tickets");

    repo.upsert(config.clone()).await.unwrap();

    // Postgres TIMESTAMPTZ is microsecond-precision; chrono::Utc::now() can carry nanosecond
    // precision, so compare fields individually rather than full-struct equality.
    let found = repo.get(config.tenant_id).await.unwrap().expect("row should exist");
    assert_eq!(found.tenant_id, config.tenant_id);
    assert_eq!(found.prompt, config.prompt);
}

#[tokio::test]
async fn upsert_replaces_the_existing_row_for_the_same_tenant_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAnalysisConfigRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    repo.upsert(AnalysisConfig::new(tenant_id, "first prompt")).await.unwrap();

    let updated = AnalysisConfig::new(tenant_id, "second prompt");
    repo.upsert(updated.clone()).await.unwrap();

    let found = repo.get(tenant_id).await.unwrap().expect("row should exist");
    assert_eq!(found.prompt, "second prompt");
}

#[tokio::test]
async fn get_returns_none_for_a_tenant_with_no_row_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAnalysisConfigRepository::new(pool);
    let found = repo.get(Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
}
