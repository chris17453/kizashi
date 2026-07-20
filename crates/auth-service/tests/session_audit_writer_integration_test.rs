//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use auth_service::{
    AuditLogReader, PostgresAuditLogReader, PostgresSessionAuditWriter, SessionAuditWriter,
};
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

#[tokio::test]
async fn record_revocation_writes_an_audit_row_readable_via_the_shared_reader() {
    let pool = test_pool().await;
    let writer = PostgresSessionAuditWriter::new(pool.clone());
    let reader = PostgresAuditLogReader::new(pool);
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    writer
        .record_revocation(tenant_id, "admin@example.com", session_id, "bob")
        .await
        .expect("record_revocation should succeed");

    let entries = reader.list_for_entity(tenant_id, session_id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entity_type, "session");
    assert_eq!(entries[0].actor, "admin@example.com");
    assert_eq!(entries[0].after["revoked_username"], "bob");
}

#[tokio::test]
async fn session_audit_rows_are_immutable_at_the_database_level() {
    let pool = test_pool().await;
    let writer = PostgresSessionAuditWriter::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    writer.record_revocation(tenant_id, "admin@example.com", session_id, "bob").await.unwrap();

    let result = sqlx::query("DELETE FROM auth_audit_log WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await;

    assert!(result.is_err(), "auth_audit_log must reject DELETE at the database level");
}
