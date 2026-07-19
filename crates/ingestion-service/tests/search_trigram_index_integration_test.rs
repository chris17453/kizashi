//! Integration test against real Postgres (CLAUDE.md §2), proving the pg_trgm GIN index
//! (migration 0004) exists and that free-text search still returns correct results with it in
//! place — the index changes the scan strategy the planner *can* pick, not what a query
//! actually matches, so this is a behavior-preservation check as much as an existence check.
//! Requires DATABASE_URL.

use common::{RawRecord, SourceType};
use ingestion_service::{PostgresRawRecordRepository, RawRecordRepository, RecordSearchFilter};
use serde_json::json;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set to run search_trigram_index_integration_test");
    let pool = common::connect_with_schema(&database_url, "ingestion_service")
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
async fn pg_trgm_extension_and_indexes_exist_after_migration() {
    let pool = test_pool().await;

    let extension_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(extension_exists, "pg_trgm extension should be installed");

    let index_names: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE tablename = 'raw_records' AND indexname LIKE '%trgm%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(index_names.contains(&"idx_raw_records_payload_text_trgm".to_string()));
    assert!(index_names.contains(&"idx_raw_records_subject_trgm".to_string()));
    assert!(index_names.contains(&"idx_raw_records_from_trgm".to_string()));
}

#[tokio::test]
async fn free_text_search_still_finds_a_substring_match_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresRawRecordRepository::new(pool);
    let tenant_id = Uuid::new_v4();

    let mut record = RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        json!({"description": "the office printer is on fire, please send help"}),
    );
    record.id = Uuid::new_v4();
    repo.insert(&record).await.unwrap();

    let results = repo
        .search(
            tenant_id,
            &RecordSearchFilter {
                query: Some("printer".to_string()),
                limit: 10,
                offset: 0,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(results.iter().any(|r| r.id == record.id));

    let no_match = repo
        .search(
            tenant_id,
            &RecordSearchFilter {
                query: Some("nonexistent-term-xyz".to_string()),
                limit: 10,
                offset: 0,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(no_match.iter().all(|r| r.id != record.id));
}
