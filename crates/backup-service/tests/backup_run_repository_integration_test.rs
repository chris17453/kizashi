//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use backup_service::{BackupRunRepository, BackupRunStatus, PostgresBackupRunRepository};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "backup_service")
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
async fn start_then_complete_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresBackupRunRepository::new(pool);
    let id = Uuid::new_v4();

    repo.start(id, chrono::Utc::now(), "postgres/test.dump").await.unwrap();
    repo.complete(id, chrono::Utc::now(), 2048).await.unwrap();

    let runs = repo.list_recent(50).await.unwrap();
    let found = runs.iter().find(|r| r.id == id).expect("run should be present");
    assert_eq!(found.status, BackupRunStatus::Success);
    assert_eq!(found.size_bytes, Some(2048));
}

#[tokio::test]
async fn start_then_fail_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresBackupRunRepository::new(pool);
    let id = Uuid::new_v4();

    repo.start(id, chrono::Utc::now(), "postgres/test.dump").await.unwrap();
    repo.fail(id, chrono::Utc::now(), "pg_dump exited with status 1").await.unwrap();

    let runs = repo.list_recent(50).await.unwrap();
    let found = runs.iter().find(|r| r.id == id).expect("run should be present");
    assert_eq!(found.status, BackupRunStatus::Failed);
    assert_eq!(found.error.as_deref(), Some("pg_dump exited with status 1"));
}
