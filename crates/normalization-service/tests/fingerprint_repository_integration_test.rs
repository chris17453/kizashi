//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use normalization_service::{DedupOutcome, FingerprintRepository, PostgresFingerprintRepository};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "normalization_service")
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
async fn first_sighting_is_new_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresFingerprintRepository::new(pool);

    let outcome = repo
        .check_and_record(Uuid::new_v4(), "integration-test-fp", Uuid::new_v4(), Some(3600))
        .await
        .unwrap();
    assert_eq!(outcome, DedupOutcome::New);
}

#[tokio::test]
async fn a_second_sighting_within_the_window_is_a_duplicate_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresFingerprintRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let fingerprint = format!("fp-{}", Uuid::new_v4());

    repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), Some(3600)).await.unwrap();
    let second =
        repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), Some(3600)).await.unwrap();

    assert_eq!(second, DedupOutcome::Duplicate);
}

#[tokio::test]
async fn a_second_sighting_outside_the_window_is_new_again_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresFingerprintRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let fingerprint = format!("fp-{}", Uuid::new_v4());

    // Window of 0 seconds: the very next check is already outside the window.
    repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), Some(0)).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let second =
        repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), Some(0)).await.unwrap();

    assert_eq!(second, DedupOutcome::New);
}

#[tokio::test]
async fn occurrence_count_increments_on_each_duplicate_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresFingerprintRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let fingerprint = format!("fp-{}", Uuid::new_v4());

    repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), None).await.unwrap();
    repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), None).await.unwrap();
    repo.check_and_record(tenant_id, &fingerprint, Uuid::new_v4(), None).await.unwrap();

    let (count,): (i64,) = sqlx::query_as(
        "SELECT occurrence_count FROM record_fingerprints WHERE tenant_id = $1 AND fingerprint = $2",
    )
    .bind(tenant_id)
    .bind(&fingerprint)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 3);
}
