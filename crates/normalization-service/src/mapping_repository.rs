#[path = "mapping_repository_test.rs"]
#[cfg(test)]
pub(crate) mod mapping_repository_test;

use async_trait::async_trait;
use common::NormalizationMapping;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MappingRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Looks up the active `NormalizationMapping` for a tenant's source type (spec §5.6). v1 owns
/// this data directly in Normalization Service's own Postgres schema rather than depending on
/// Config/Admin Service (not yet built) — see docs/features.md entry for this crate. When
/// Config/Admin Service ships, it becomes the authority and this repository's Postgres
/// implementation is swapped for a client of that service's API, without touching callers of
/// this trait.
#[async_trait]
pub trait MappingRepository: Send + Sync {
    async fn active_mapping(
        &self,
        tenant_id: Uuid,
        source_type: &str,
    ) -> Result<Option<NormalizationMapping>, MappingRepositoryError>;

    /// Mirrors a `NormalizationMapping` from `config-admin-service`'s `mapping.changed` bus
    /// message into this service's own local table — the sync mechanism ADR-0010 originally
    /// called for and ADR-0018 built for triggers, extended here to mappings.
    async fn upsert(&self, mapping: NormalizationMapping) -> Result<(), MappingRepositoryError>;

    /// Removes a mapping by id (ADR-0110): the write side of syncing a `MappingChangeEvent::
    /// Deleted` message, same shape as `TriggerRepository::delete` in trigger-engine.
    async fn delete(&self, id: Uuid) -> Result<(), MappingRepositoryError>;
}

pub struct PostgresMappingRepository {
    pool: sqlx::PgPool,
}

impl PostgresMappingRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MappingRepository for PostgresMappingRepository {
    async fn active_mapping(
        &self,
        tenant_id: Uuid,
        source_type: &str,
    ) -> Result<Option<NormalizationMapping>, MappingRepositoryError> {
        let row: Option<(
            Uuid,
            Uuid,
            String,
            sqlx::types::Json<std::collections::BTreeMap<String, String>>,
            i32,
        )> = sqlx::query_as(
            r#"
                SELECT id, tenant_id, source_type, field_map, version
                FROM normalization_mappings
                WHERE tenant_id = $1 AND source_type = $2
                ORDER BY version DESC
                LIMIT 1
                "#,
        )
        .bind(tenant_id)
        .bind(source_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MappingRepositoryError::Backend(e.to_string()))?;

        Ok(row.map(|(id, tenant_id, source_type, field_map, version)| NormalizationMapping {
            id,
            tenant_id,
            source_type,
            field_map: field_map.0,
            version,
        }))
    }

    async fn upsert(&self, mapping: NormalizationMapping) -> Result<(), MappingRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO normalization_mappings (id, tenant_id, source_type, field_map, version)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                source_type = EXCLUDED.source_type,
                field_map = EXCLUDED.field_map,
                version = EXCLUDED.version
            "#,
        )
        .bind(mapping.id)
        .bind(mapping.tenant_id)
        .bind(&mapping.source_type)
        .bind(sqlx::types::Json(&mapping.field_map))
        .bind(mapping.version)
        .execute(&self.pool)
        .await
        .map_err(|e| MappingRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), MappingRepositoryError> {
        sqlx::query("DELETE FROM normalization_mappings WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MappingRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
