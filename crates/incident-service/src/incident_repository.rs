#[path = "incident_repository_test.rs"]
#[cfg(test)]
pub(crate) mod incident_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::{Incident, IncidentNote, IncidentSeverity, IncidentStatus};
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum IncidentRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no incident with id {0}")]
    NotFound(Uuid),
}

/// CRUD + Event-linking for Incident, incident-service's own Postgres schema (ADR-0111). Every
/// create/update/link/unlink writes one audit_log row in the same transaction as the entity
/// change, same shape as config-admin-service's repositories.
#[async_trait]
pub trait IncidentRepository: Send + Sync {
    /// Creates an incident and, in the same transaction, links any `initial_event_ids` —
    /// covers the "create incident from selected Events" bulk action in one atomic call.
    async fn create(
        &self,
        incident: Incident,
        initial_event_ids: &[Uuid],
        actor: &str,
    ) -> Result<Incident, IncidentRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Incident>, IncidentRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<Incident>, IncidentRepositoryError>;
    async fn update(
        &self,
        incident: Incident,
        actor: &str,
    ) -> Result<Incident, IncidentRepositoryError>;
    async fn link_event(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
        actor: &str,
    ) -> Result<(), IncidentRepositoryError>;
    async fn unlink_event(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
        actor: &str,
    ) -> Result<(), IncidentRepositoryError>;
    async fn list_linked_event_ids(
        &self,
        incident_id: Uuid,
    ) -> Result<Vec<Uuid>, IncidentRepositoryError>;
    async fn list_notes(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
    ) -> Result<Vec<IncidentNote>, IncidentRepositoryError>;
    async fn add_note(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        author: &str,
        body: &str,
    ) -> Result<IncidentNote, IncidentRepositoryError>;
}

pub struct PostgresIncidentRepository {
    pool: sqlx::PgPool,
}

impl PostgresIncidentRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type IncidentRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
);

fn row_to_incident(row: IncidentRow) -> Incident {
    let (
        id,
        tenant_id,
        title,
        summary,
        severity,
        status,
        assigned_to,
        created_at,
        updated_at,
        resolved_at,
    ) = row;
    Incident {
        id,
        tenant_id,
        title,
        summary,
        severity: IncidentSeverity::from_str(&severity).unwrap_or(IncidentSeverity::Low),
        status: IncidentStatus::from_str(&status).unwrap_or(IncidentStatus::Open),
        assigned_to,
        created_at,
        updated_at,
        resolved_at,
    }
}

const INCIDENT_COLUMNS: &str =
    "id, tenant_id, title, summary, severity, status, assigned_to, created_at, updated_at, resolved_at";

#[async_trait]
impl IncidentRepository for PostgresIncidentRepository {
    async fn create(
        &self,
        incident: Incident,
        initial_event_ids: &[Uuid],
        actor: &str,
    ) -> Result<Incident, IncidentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO incidents
                (id, tenant_id, title, summary, severity, status, assigned_to, created_at, updated_at, resolved_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(incident.id)
        .bind(incident.tenant_id)
        .bind(&incident.title)
        .bind(&incident.summary)
        .bind(incident.severity.to_string())
        .bind(incident.status.to_string())
        .bind(&incident.assigned_to)
        .bind(incident.created_at)
        .bind(incident.updated_at)
        .bind(incident.resolved_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        for event_id in initial_event_ids {
            sqlx::query(
                "INSERT INTO incident_events (incident_id, event_id, linked_at) VALUES ($1, $2, $3)",
            )
            .bind(incident.id)
            .bind(event_id)
            .bind(chrono::Utc::now())
            .execute(&mut *tx)
            .await
            .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        }

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: incident.tenant_id,
                entity_type: "incident".to_string(),
                entity_id: incident.id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::to_value(&incident).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(incident)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Incident>, IncidentRepositoryError> {
        let row: Option<IncidentRow> = sqlx::query_as(&format!(
            "SELECT {INCIDENT_COLUMNS} FROM incidents WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_incident))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<Incident>, IncidentRepositoryError> {
        let rows: Vec<IncidentRow> = if let Some(status) = status_filter {
            sqlx::query_as(&format!(
                "SELECT {INCIDENT_COLUMNS} FROM incidents WHERE tenant_id = $1 AND status = $2 ORDER BY created_at DESC"
            ))
            .bind(tenant_id)
            .bind(status.to_string())
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as(&format!(
                "SELECT {INCIDENT_COLUMNS} FROM incidents WHERE tenant_id = $1 ORDER BY created_at DESC"
            ))
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_incident).collect())
    }

    async fn update(
        &self,
        incident: Incident,
        actor: &str,
    ) -> Result<Incident, IncidentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        let existing: Option<IncidentRow> = sqlx::query_as(&format!(
            "SELECT {INCIDENT_COLUMNS} FROM incidents WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(incident.id)
        .bind(incident.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(IncidentRepositoryError::NotFound(incident.id));
        };
        let before = row_to_incident(existing);

        sqlx::query(
            r#"
            UPDATE incidents
            SET title = $1, summary = $2, severity = $3, status = $4, assigned_to = $5, updated_at = $6, resolved_at = $7
            WHERE id = $8 AND tenant_id = $9
            "#,
        )
        .bind(&incident.title)
        .bind(&incident.summary)
        .bind(incident.severity.to_string())
        .bind(incident.status.to_string())
        .bind(&incident.assigned_to)
        .bind(incident.updated_at)
        .bind(incident.resolved_at)
        .bind(incident.id)
        .bind(incident.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: incident.tenant_id,
                entity_type: "incident".to_string(),
                entity_id: incident.id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&incident).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(incident)
    }

    async fn link_event(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
        actor: &str,
    ) -> Result<(), IncidentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM incidents WHERE id = $1 AND tenant_id = $2")
                .bind(incident_id)
                .bind(tenant_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        if exists.is_none() {
            return Err(IncidentRepositoryError::NotFound(incident_id));
        }

        sqlx::query(
            "INSERT INTO incident_events (incident_id, event_id, linked_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(incident_id)
        .bind(event_id)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "incident_event".to_string(),
                entity_id: incident_id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::json!({ "event_id": event_id }),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn unlink_event(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
        actor: &str,
    ) -> Result<(), IncidentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM incidents WHERE id = $1 AND tenant_id = $2")
                .bind(incident_id)
                .bind(tenant_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        if exists.is_none() {
            return Err(IncidentRepositoryError::NotFound(incident_id));
        }

        sqlx::query("DELETE FROM incident_events WHERE incident_id = $1 AND event_id = $2")
            .bind(incident_id)
            .bind(event_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "incident_event".to_string(),
                entity_id: incident_id,
                change_type: ChangeType::Deleted,
                actor: actor.to_string(),
                before: Some(serde_json::json!({ "event_id": event_id })),
                after: serde_json::Value::Null,
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_linked_event_ids(
        &self,
        incident_id: Uuid,
    ) -> Result<Vec<Uuid>, IncidentRepositoryError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT event_id FROM incident_events WHERE incident_id = $1 ORDER BY linked_at ASC",
        )
        .bind(incident_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    async fn list_notes(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
    ) -> Result<Vec<IncidentNote>, IncidentRepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, incident_id, author, body, created_at FROM incident_notes WHERE tenant_id = $1 AND incident_id = $2 ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(incident_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))
    }

    async fn add_note(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        author: &str,
        body: &str,
    ) -> Result<IncidentNote, IncidentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM incidents WHERE id = $1 AND tenant_id = $2")
                .bind(incident_id)
                .bind(tenant_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        if exists.is_none() {
            return Err(IncidentRepositoryError::NotFound(incident_id));
        }
        let note = IncidentNote {
            id: Uuid::new_v4(),
            tenant_id,
            incident_id,
            author: author.to_string(),
            body: body.to_string(),
            created_at: chrono::Utc::now(),
        };
        sqlx::query("INSERT INTO incident_notes (id, tenant_id, incident_id, author, body, created_at) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(note.id).bind(note.tenant_id).bind(note.incident_id).bind(&note.author).bind(&note.body).bind(note.created_at)
            .execute(&mut *tx).await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "incident_note".to_string(),
                entity_id: incident_id,
                change_type: ChangeType::Created,
                actor: author.to_string(),
                before: None,
                after: serde_json::to_value(&note).unwrap_or_default(),
                changed_at: note.created_at,
            },
        )
        .await
        .map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| IncidentRepositoryError::Backend(e.to_string()))?;
        Ok(note)
    }
}
