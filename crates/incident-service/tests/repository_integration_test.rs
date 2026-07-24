//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.
//!
//! Exercises the transactional create/update/link/unlink + audit-log-write behavior that the
//! in-memory test double used for handler unit tests can't: `record_audit_entry` writing a
//! real row in the same Postgres transaction as the entity change, and the append-only
//! immutability trigger on `incident_audit_log`.

use common::{Incident, IncidentSeverity, IncidentStatus};
use incident_service::{
    AuditLogReader, ChangeType, IncidentRepository, IncidentRepositoryError,
    PostgresAuditLogReader, PostgresIncidentRepository,
};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "incident_service")
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

fn sample_incident(tenant_id: Uuid) -> Incident {
    Incident {
        id: Uuid::new_v4(),
        tenant_id,
        title: "integration-test-incident".to_string(),
        summary: String::new(),
        severity: IncidentSeverity::High,
        status: IncidentStatus::Open,
        assigned_to: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        resolved_at: None,
    }
}

#[tokio::test]
async fn create_incident_writes_a_created_audit_row_in_the_same_transaction() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);

    repo.create(incident.clone(), &[], "operator@example.com")
        .await
        .expect("create should succeed");

    // Postgres TIMESTAMPTZ is microsecond-precision; chrono::Utc::now() can carry nanosecond
    // precision, so compare fields individually rather than full-struct equality.
    let found = repo.get(tenant_id, incident.id).await.unwrap().expect("row should exist");
    assert_eq!(found.id, incident.id);
    assert_eq!(found.title, incident.title);
    assert_eq!(found.severity, incident.severity);
    assert_eq!(found.status, incident.status);

    let entries = audit_reader.list_for_entity(tenant_id, incident.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, ChangeType::Created);
    assert_eq!(entries[0].entity_type, "incident");
}

#[tokio::test]
async fn create_with_initial_events_links_them_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let event_id = Uuid::new_v4();

    repo.create(incident.clone(), &[event_id], "operator@example.com").await.unwrap();

    let linked = repo.list_linked_event_ids(incident.id).await.unwrap();
    assert_eq!(linked, vec![event_id]);
}

#[tokio::test]
async fn update_incident_writes_an_updated_audit_row_with_before_and_after() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    repo.create(incident.clone(), &[], "operator@example.com").await.unwrap();

    let mut updated = incident.clone();
    updated.status = IncidentStatus::Resolved;
    repo.update(updated.clone(), "operator@example.com").await.expect("update should succeed");

    let found = repo.get(tenant_id, incident.id).await.unwrap().expect("row should exist");
    assert_eq!(found.status, IncidentStatus::Resolved);

    let entries = audit_reader.list_for_entity(tenant_id, incident.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    let update_entry = entries.iter().find(|e| e.change_type == ChangeType::Updated).unwrap();
    assert!(update_entry.before.is_some());
}

#[tokio::test]
async fn update_of_unknown_incident_returns_not_found_without_leaving_a_partial_audit_row() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);

    let err = repo.update(incident.clone(), "operator@example.com").await.unwrap_err();
    assert!(matches!(err, IncidentRepositoryError::NotFound(_)));

    let entries = audit_reader.list_for_entity(tenant_id, incident.id).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn link_then_unlink_event_writes_audit_rows_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool.clone());
    let audit_reader = PostgresAuditLogReader::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    repo.create(incident.clone(), &[], "operator@example.com").await.unwrap();
    let event_id = Uuid::new_v4();

    repo.link_event(tenant_id, incident.id, event_id, "operator@example.com").await.unwrap();
    assert_eq!(repo.list_linked_event_ids(incident.id).await.unwrap(), vec![event_id]);

    repo.unlink_event(tenant_id, incident.id, event_id, "operator@example.com").await.unwrap();
    assert!(repo.list_linked_event_ids(incident.id).await.unwrap().is_empty());

    let entries = audit_reader.list_for_entity(tenant_id, incident.id).await.unwrap();
    // create + link + unlink
    assert_eq!(entries.len(), 3);
}

#[tokio::test]
async fn incident_audit_log_rejects_update_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    repo.create(incident.clone(), &[], "operator@example.com").await.unwrap();

    let err = sqlx::query("UPDATE incident_audit_log SET actor = 'tampered' WHERE entity_id = $1")
        .bind(incident.id)
        .execute(&pool)
        .await
        .expect_err("update should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}

#[tokio::test]
async fn incident_audit_log_rejects_delete_at_the_database_level() {
    let pool = test_pool().await;
    let repo = PostgresIncidentRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    repo.create(incident.clone(), &[], "operator@example.com").await.unwrap();

    let err = sqlx::query("DELETE FROM incident_audit_log WHERE entity_id = $1")
        .bind(incident.id)
        .execute(&pool)
        .await
        .expect_err("delete should be rejected by the immutability trigger");
    assert!(err.to_string().contains("append-only"));
}
