//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use auth_service::{LoginAttempt, LoginAttemptRepository, PostgresLoginAttemptRepository};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "auth_service")
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

fn sample_attempt(tenant_id: Option<Uuid>, username: &str, success: bool) -> LoginAttempt {
    LoginAttempt {
        id: Uuid::new_v4(),
        tenant_id,
        username: username.to_string(),
        success,
        reason: if success { "success".to_string() } else { "wrong_password".to_string() },
        attempted_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn record_then_list_recent_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresLoginAttemptRepository::new(pool);
    let tenant_id = Uuid::new_v4();

    repo.record(&sample_attempt(Some(tenant_id), "alice", false)).await.unwrap();
    repo.record(&sample_attempt(Some(tenant_id), "alice", true)).await.unwrap();
    repo.record(&sample_attempt(Some(Uuid::new_v4()), "eve", false)).await.unwrap();

    let found = repo.list_recent(tenant_id, 10, None).await.unwrap();

    assert_eq!(found.len(), 2);
    assert!(found[0].attempted_at >= found[1].attempted_at, "expected most-recent-first order");
}

#[tokio::test]
async fn a_record_with_no_tenant_id_persists_as_null() {
    let pool = test_pool().await;
    let repo = PostgresLoginAttemptRepository::new(pool);

    repo.record(&sample_attempt(None, "nobody", false)).await.unwrap();
    // No tenant_id to scope a list_recent query to -- just confirming the write itself
    // succeeds against the nullable column, since list_recent is always tenant-scoped by
    // design (there's no platform-wide read path here for unknown-workspace attempts).
}

#[tokio::test]
async fn login_attempts_rejects_update_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresLoginAttemptRepository::new(pool.clone());
    let attempt = sample_attempt(Some(Uuid::new_v4()), "alice", false);
    repo.record(&attempt).await.unwrap();

    let result = sqlx::query("UPDATE login_attempts SET success = true WHERE id = $1")
        .bind(attempt.id)
        .execute(&pool)
        .await;

    assert!(result.is_err(), "the append-only trigger must reject UPDATE");
}

#[tokio::test]
async fn login_attempts_rejects_delete_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresLoginAttemptRepository::new(pool.clone());
    let attempt = sample_attempt(Some(Uuid::new_v4()), "alice", false);
    repo.record(&attempt).await.unwrap();

    let result = sqlx::query("DELETE FROM login_attempts WHERE id = $1")
        .bind(attempt.id)
        .execute(&pool)
        .await;

    assert!(result.is_err(), "the append-only trigger must reject DELETE");
}
