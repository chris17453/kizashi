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
}
