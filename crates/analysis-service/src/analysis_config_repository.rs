#[path = "analysis_config_repository_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_config_repository_test;

use async_trait::async_trait;
use common::AnalysisConfig;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AnalysisConfigRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Analysis Service's own read-mostly copy of each tenant's AI prompt (ADR-0019), kept
/// current by upserting on every `analysis_config.changed` bus message — never written to
/// directly by any HTTP handler, since config-admin-service is the sole source of truth.
#[async_trait]
pub trait AnalysisConfigRepository: Send + Sync {
    async fn get(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError>;
    async fn upsert(&self, config: AnalysisConfig) -> Result<(), AnalysisConfigRepositoryError>;
}

pub struct PostgresAnalysisConfigRepository {
    pool: sqlx::PgPool,
}

impl PostgresAnalysisConfigRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type AnalysisConfigRow = (
    Uuid,
    String,
    chrono::DateTime<chrono::Utc>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn row_to_config(row: AnalysisConfigRow) -> AnalysisConfig {
    let (tenant_id, prompt, updated_at, provider, model, endpoint, api_key) = row;
    let provider = match provider.as_str() {
        "openai_compatible" => common::AnalysisProvider::OpenAiCompatible,
        _ => common::AnalysisProvider::AzureFoundry,
    };
    AnalysisConfig { tenant_id, prompt, provider, model, endpoint, api_key, updated_at }
}

fn provider_str(provider: common::AnalysisProvider) -> &'static str {
    match provider {
        common::AnalysisProvider::AzureFoundry => "azure_foundry",
        common::AnalysisProvider::OpenAiCompatible => "openai_compatible",
    }
}

#[async_trait]
impl AnalysisConfigRepository for PostgresAnalysisConfigRepository {
    async fn get(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError> {
        let row: Option<AnalysisConfigRow> = sqlx::query_as(
            "SELECT tenant_id, prompt, updated_at, provider, model, endpoint, api_key FROM analysis_configs WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_config))
    }

    async fn upsert(&self, config: AnalysisConfig) -> Result<(), AnalysisConfigRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO analysis_configs (tenant_id, prompt, updated_at, provider, model, endpoint, api_key)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (tenant_id) DO UPDATE SET
                prompt = EXCLUDED.prompt,
                updated_at = EXCLUDED.updated_at,
                provider = EXCLUDED.provider,
                model = EXCLUDED.model,
                endpoint = EXCLUDED.endpoint,
                api_key = EXCLUDED.api_key
            "#,
        )
        .bind(config.tenant_id)
        .bind(&config.prompt)
        .bind(config.updated_at)
        .bind(provider_str(config.provider))
        .bind(&config.model)
        .bind(&config.endpoint)
        .bind(&config.api_key)
        .execute(&self.pool)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
