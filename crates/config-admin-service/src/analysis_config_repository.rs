#[path = "analysis_config_repository_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_config_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::AnalysisConfig;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AnalysisConfigRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Upsert-only CRUD for AnalysisConfig (ADR-0019), config-admin's own Postgres schema — one
/// row per tenant, so "write" is always "replace the tenant's prompt", not create-vs-update
/// as a caller-visible distinction. Every write still logs an audit_log row in the same
/// transaction (CLAUDE.md §5), tagged Created or Updated based on whether a row already
/// existed.
#[async_trait]
pub trait AnalysisConfigRepository: Send + Sync {
    async fn upsert(
        &self,
        config: AnalysisConfig,
    ) -> Result<AnalysisConfig, AnalysisConfigRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError>;
}

pub struct PostgresAnalysisConfigRepository {
    pool: sqlx::PgPool,
}

impl PostgresAnalysisConfigRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type AnalysisConfigRow = (Uuid, String, chrono::DateTime<chrono::Utc>);

fn row_to_config(row: AnalysisConfigRow) -> AnalysisConfig {
    let (tenant_id, prompt, updated_at) = row;
    AnalysisConfig { tenant_id, prompt, updated_at }
}

#[async_trait]
impl AnalysisConfigRepository for PostgresAnalysisConfigRepository {
    async fn upsert(
        &self,
        config: AnalysisConfig,
    ) -> Result<AnalysisConfig, AnalysisConfigRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;

        let existing: Option<AnalysisConfigRow> = sqlx::query_as(
            "SELECT tenant_id, prompt, updated_at FROM analysis_configs WHERE tenant_id = $1",
        )
        .bind(config.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        let before = existing.map(row_to_config);

        sqlx::query(
            r#"
            INSERT INTO analysis_configs (tenant_id, prompt, updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (tenant_id) DO UPDATE SET prompt = EXCLUDED.prompt, updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(config.tenant_id)
        .bind(&config.prompt)
        .bind(config.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: config.tenant_id,
                entity_type: "analysis_config".to_string(),
                entity_id: config.tenant_id,
                change_type: if before.is_some() {
                    ChangeType::Updated
                } else {
                    ChangeType::Created
                },
                actor: config.tenant_id.to_string(),
                before: before.map(|b| serde_json::to_value(b).unwrap_or_default()),
                after: serde_json::to_value(&config).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        Ok(config)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError> {
        let row: Option<AnalysisConfigRow> = sqlx::query_as(
            "SELECT tenant_id, prompt, updated_at FROM analysis_configs WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_config))
    }
}
