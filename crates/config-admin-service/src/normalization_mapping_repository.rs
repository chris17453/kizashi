#[path = "normalization_mapping_repository_test.rs"]
#[cfg(test)]
pub(crate) mod normalization_mapping_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::NormalizationMapping;
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum NormalizationMappingRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no normalization mapping with id {0}")]
    NotFound(Uuid),
}

/// CRUD for NormalizationMapping, config-admin's own Postgres schema (ADR-0010). Every
/// create/update writes one audit_log row in the same transaction as the entity change.
/// Creating a mapping for a (tenant, source_type) that already has one produces a new version
/// row rather than overwriting — matching NormalizationMapping's own versioning design.
#[async_trait]
pub trait NormalizationMappingRepository: Send + Sync {
    async fn create(
        &self,
        mapping: NormalizationMapping,
        actor: &str,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError>;
    async fn update(
        &self,
        mapping: NormalizationMapping,
        actor: &str,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<NormalizationMapping>, NormalizationMappingRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingRepositoryError>;
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), NormalizationMappingRepositoryError>;
}

pub struct PostgresNormalizationMappingRepository {
    pool: sqlx::PgPool,
}

impl PostgresNormalizationMappingRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type MappingRow = (
    Uuid,
    Uuid,
    String,
    sqlx::types::Json<BTreeMap<String, String>>,
    i32,
    sqlx::types::Json<Vec<String>>,
    Option<i64>,
);

fn row_to_mapping(row: MappingRow) -> NormalizationMapping {
    let (id, tenant_id, source_type, field_map, version, dedup_fields, dedup_window_seconds) = row;
    NormalizationMapping {
        id,
        tenant_id,
        source_type,
        field_map: field_map.0,
        version,
        dedup_fields: dedup_fields.0,
        dedup_window_seconds,
    }
}

const MAPPING_COLUMNS: &str =
    "id, tenant_id, source_type, field_map, version, dedup_fields, dedup_window_seconds";

#[async_trait]
impl NormalizationMappingRepository for PostgresNormalizationMappingRepository {
    async fn create(
        &self,
        mapping: NormalizationMapping,
        actor: &str,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            "INSERT INTO normalization_mappings (id, tenant_id, source_type, field_map, version, dedup_fields, dedup_window_seconds) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(mapping.id)
        .bind(mapping.tenant_id)
        .bind(&mapping.source_type)
        .bind(sqlx::types::Json(&mapping.field_map))
        .bind(mapping.version)
        .bind(sqlx::types::Json(&mapping.dedup_fields))
        .bind(mapping.dedup_window_seconds)
        .execute(&mut *tx)
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: mapping.tenant_id,
                entity_type: "normalization_mapping".to_string(),
                entity_id: mapping.id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::to_value(&mapping).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;
        Ok(mapping)
    }

    async fn update(
        &self,
        mapping: NormalizationMapping,
        actor: &str,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        let existing: Option<MappingRow> = sqlx::query_as(&format!(
            "SELECT {MAPPING_COLUMNS} FROM normalization_mappings WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(mapping.id)
        .bind(mapping.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(NormalizationMappingRepositoryError::NotFound(mapping.id));
        };
        let before = row_to_mapping(existing);

        sqlx::query(
            "UPDATE normalization_mappings SET source_type = $1, field_map = $2, version = $3, dedup_fields = $4, dedup_window_seconds = $5 WHERE id = $6 AND tenant_id = $7",
        )
        .bind(&mapping.source_type)
        .bind(sqlx::types::Json(&mapping.field_map))
        .bind(mapping.version)
        .bind(sqlx::types::Json(&mapping.dedup_fields))
        .bind(mapping.dedup_window_seconds)
        .bind(mapping.id)
        .bind(mapping.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: mapping.tenant_id,
                entity_type: "normalization_mapping".to_string(),
                entity_id: mapping.id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&mapping).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;
        Ok(mapping)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<NormalizationMapping>, NormalizationMappingRepositoryError> {
        let row: Option<MappingRow> = sqlx::query_as(&format!(
            "SELECT {MAPPING_COLUMNS} FROM normalization_mappings WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_mapping))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingRepositoryError> {
        let rows: Vec<MappingRow> = sqlx::query_as(&format!(
            "SELECT {MAPPING_COLUMNS} FROM normalization_mappings WHERE tenant_id = $1 ORDER BY source_type, version"
        ))
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_mapping).collect())
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), NormalizationMappingRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        let existing: Option<MappingRow> = sqlx::query_as(&format!(
            "SELECT {MAPPING_COLUMNS} FROM normalization_mappings WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(NormalizationMappingRepositoryError::NotFound(id));
        };
        let before = row_to_mapping(existing);

        sqlx::query("DELETE FROM normalization_mappings WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "normalization_mapping".to_string(),
                entity_id: id,
                change_type: ChangeType::Deleted,
                actor: actor.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::Value::Null,
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| NormalizationMappingRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
