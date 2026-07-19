//! Integration test against real Postgres (CLAUDE.md §2), proving egress-gateway's schema
//! (ADR-0021) actually persists the allowlist and audit log, and that the audit log's
//! immutability trigger really rejects UPDATE/DELETE. Requires DATABASE_URL.

use egress_gateway::{
    AllowlistRepository, AuditLogEntry, AuditLogRepository, PostgresAllowlistRepository,
    PostgresAuditLogRepository,
};

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "egress_gateway")
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
async fn allowlist_set_then_get_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAllowlistRepository::new(pool);
    let tenant_id = uuid::Uuid::new_v4().to_string();

    repo.set_domains(&tenant_id, vec!["zendesk.com".to_string(), "example.com".to_string()])
        .await
        .unwrap();

    let domains = repo.get_domains(&tenant_id).await.unwrap();
    assert_eq!(domains, vec!["zendesk.com".to_string(), "example.com".to_string()]);
}

#[tokio::test]
async fn allowlist_get_returns_empty_for_an_unconfigured_tenant_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAllowlistRepository::new(pool);
    let domains = repo.get_domains(&uuid::Uuid::new_v4().to_string()).await.unwrap();
    assert!(domains.is_empty());
}

#[tokio::test]
async fn allowlist_set_replaces_the_existing_list_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAllowlistRepository::new(pool);
    let tenant_id = uuid::Uuid::new_v4().to_string();

    repo.set_domains(&tenant_id, vec!["first.com".to_string()]).await.unwrap();
    repo.set_domains(&tenant_id, vec!["second.com".to_string()]).await.unwrap();

    let domains = repo.get_domains(&tenant_id).await.unwrap();
    assert_eq!(domains, vec!["second.com".to_string()]);
}

#[tokio::test]
async fn audit_log_record_persists_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAuditLogRepository::new(pool.clone());
    let tenant_id = uuid::Uuid::new_v4().to_string();

    repo.record(AuditLogEntry {
        tenant_id: tenant_id.clone(),
        connector_id: "zendesk-connector".to_string(),
        destination_host: "api.zendesk.com".to_string(),
        destination_port: 443,
        allowed: true,
        occurred_at: chrono::Utc::now(),
    })
    .await
    .unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM egress_audit_log WHERE tenant_id = $1")
            .bind(&tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn egress_audit_log_rejects_update_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresAuditLogRepository::new(pool.clone());
    let tenant_id = uuid::Uuid::new_v4().to_string();
    repo.record(AuditLogEntry {
        tenant_id: tenant_id.clone(),
        connector_id: "zendesk-connector".to_string(),
        destination_host: "api.zendesk.com".to_string(),
        destination_port: 443,
        allowed: true,
        occurred_at: chrono::Utc::now(),
    })
    .await
    .unwrap();

    let err = sqlx::query("UPDATE egress_audit_log SET allowed = false WHERE tenant_id = $1")
        .bind(&tenant_id)
        .execute(&pool)
        .await
        .expect_err("update should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}

#[tokio::test]
async fn egress_audit_log_rejects_delete_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresAuditLogRepository::new(pool.clone());
    let tenant_id = uuid::Uuid::new_v4().to_string();
    repo.record(AuditLogEntry {
        tenant_id: tenant_id.clone(),
        connector_id: "zendesk-connector".to_string(),
        destination_host: "api.zendesk.com".to_string(),
        destination_port: 443,
        allowed: true,
        occurred_at: chrono::Utc::now(),
    })
    .await
    .unwrap();

    let err = sqlx::query("DELETE FROM egress_audit_log WHERE tenant_id = $1")
        .bind(&tenant_id)
        .execute(&pool)
        .await
        .expect_err("delete should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}
