//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use action_executor::{ExecutionRepository, PostgresExecutionRepository};
use common::{ActionExecution, ActionExecutionStatus, ActionType};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "action_executor")
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
async fn insert_persists_an_action_execution_row() {
    let pool = test_pool().await;
    let repo = PostgresExecutionRepository::new(pool.clone());

    let execution = ActionExecution::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        ActionType::Webhook,
        serde_json::json!({"http_status": 200}),
    );

    repo.insert(&execution).await.expect("insert should succeed");

    let row: (Uuid, sqlx::types::Json<ActionExecutionStatus>) =
        sqlx::query_as("SELECT id, status FROM action_executions WHERE id = $1")
            .bind(execution.id)
            .fetch_one(&pool)
            .await
            .expect("row should exist after insert");
    assert_eq!(row.0, execution.id);
    assert_eq!(row.1 .0, ActionExecutionStatus::Pending);
}

#[tokio::test]
async fn retried_executions_are_separate_append_only_rows() {
    let pool = test_pool().await;
    let repo = PostgresExecutionRepository::new(pool.clone());

    let original = ActionExecution::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        ActionType::Email,
        serde_json::json!({}),
    );
    repo.insert(&original).await.unwrap();
    let retried = original.retry(serde_json::json!({"attempt": 2}));
    repo.insert(&retried).await.unwrap();

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM action_executions WHERE trigger_id = $1 AND event_id = $2",
    )
    .bind(original.trigger_id)
    .bind(original.event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count.0, 2, "retry must append a new row, not replace the original");
}
