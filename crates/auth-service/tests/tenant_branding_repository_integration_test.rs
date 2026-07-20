//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use auth_service::{
    AuditLogReader, PostgresTenantBrandingRepository, TenantBranding, TenantBrandingRepository,
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

async fn insert_tenant(pool: &sqlx::PgPool, id: Uuid, name: &str) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .expect("failed to insert tenant");
}

#[tokio::test]
async fn returns_none_for_a_tenant_with_no_branding_set() {
    let pool = test_pool().await;
    let repo = PostgresTenantBrandingRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let name = format!("branding-none-{tenant_id}");
    insert_tenant(&pool, tenant_id, &name).await;

    let found = repo.branding_for_name(&name).await.unwrap();
    assert_eq!(
        found,
        Some(TenantBranding { product_name: None, logo_url: None, accent_color: None })
    );
}

#[tokio::test]
async fn returns_none_for_an_unknown_tenant_name() {
    let pool = test_pool().await;
    let repo = PostgresTenantBrandingRepository::new(pool);
    assert_eq!(repo.branding_for_name("nonexistent-workspace-xyz").await.unwrap(), None);
}

#[tokio::test]
async fn update_then_lookup_round_trips_and_writes_an_audit_row_with_the_real_actor() {
    let pool = test_pool().await;
    let repo = PostgresTenantBrandingRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let name = format!("branding-roundtrip-{tenant_id}");
    insert_tenant(&pool, tenant_id, &name).await;

    let branding = TenantBranding {
        product_name: Some("Acme Signals".to_string()),
        logo_url: Some("https://acme.example.com/logo.png".to_string()),
        accent_color: Some("#ff6600".to_string()),
    };
    repo.update_branding(tenant_id, branding.clone(), "alice@acme.example.com").await.unwrap();

    let found = repo.branding_for_name(&name).await.unwrap();
    assert_eq!(found, Some(branding.clone()));
    let found_by_id = repo.branding_for_id(tenant_id).await.unwrap();
    assert_eq!(found_by_id, Some(branding));

    let audit_reader = auth_service::PostgresAuditLogReader::new(pool);
    let entries = audit_reader.list_for_entity(tenant_id, tenant_id).await.unwrap();
    let entry = entries.iter().find(|e| e.entity_type == "tenant_branding").unwrap();
    assert_eq!(entry.actor, "alice@acme.example.com");
    assert_ne!(entry.actor, tenant_id.to_string());
}
