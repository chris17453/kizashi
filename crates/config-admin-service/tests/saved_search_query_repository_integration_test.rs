//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use common::SavedSearchQuery;
use config_admin_service::{PostgresSavedSearchQueryRepository, SavedSearchQueryRepository};
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

#[tokio::test]
async fn create_then_list_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresSavedSearchQueryRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let query =
        SavedSearchQuery::new(tenant_id, "urgent tickets", serde_json::json!({"q": "urgent"}));

    let created = repo.create(query.clone()).await.unwrap();
    assert_eq!(created, query);

    let listed = repo.list(tenant_id).await.unwrap();
    assert_eq!(listed, vec![query]);

    let other_tenant = repo.list(Uuid::new_v4()).await.unwrap();
    assert!(!other_tenant.iter().any(|q| q.id == created.id));
}

#[tokio::test]
async fn delete_removes_the_row_and_is_scoped_to_tenant() {
    let pool = test_pool().await;
    let repo = PostgresSavedSearchQueryRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let query = SavedSearchQuery::new(tenant_id, "flagged", serde_json::json!({}));
    repo.create(query.clone()).await.unwrap();

    let wrong_tenant_result = repo.delete(Uuid::new_v4(), query.id).await;
    assert!(wrong_tenant_result.is_err());
    assert_eq!(repo.list(tenant_id).await.unwrap(), vec![query.clone()]);

    repo.delete(tenant_id, query.id).await.unwrap();
    assert!(repo.list(tenant_id).await.unwrap().is_empty());
}
