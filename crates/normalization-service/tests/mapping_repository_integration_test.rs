//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use normalization_service::{MappingRepository, PostgresMappingRepository};
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
async fn returns_the_highest_version_mapping_for_a_tenant_and_source_type() {
    let pool = test_pool().await;
    let repo = PostgresMappingRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO normalization_mappings (id, tenant_id, source_type, field_map, version) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind("ticket")
    .bind(serde_json::json!({"text": "$.description"}))
    .bind(1)
    .execute(&pool)
    .await
    .expect("failed to insert v1");

    sqlx::query(
        "INSERT INTO normalization_mappings (id, tenant_id, source_type, field_map, version) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind("ticket")
    .bind(serde_json::json!({"text": "$.description", "entity_ref": "$.requester_id"}))
    .bind(2)
    .execute(&pool)
    .await
    .expect("failed to insert v2");

    let active =
        repo.active_mapping(tenant_id, "ticket").await.unwrap().expect("mapping should exist");
    assert_eq!(active.version, 2);
    assert!(active.field_map.contains_key("entity_ref"));

    let missing = repo.active_mapping(tenant_id, "message").await.unwrap();
    assert!(missing.is_none());
}
