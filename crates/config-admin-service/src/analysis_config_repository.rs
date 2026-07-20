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

/// Upsert-only CRUD for AnalysisConfig (ADR-0019, extended by ADR-0031's provider/model/
/// endpoint/api_key fields), config-admin's own Postgres schema — one row per tenant, so
/// "write" is always "replace the tenant's config", not create-vs-update as a caller-visible
/// distinction. Every write still logs an audit_log row in the same transaction (CLAUDE.md
/// §5), tagged Created or Updated based on whether a row already existed — with `api_key`
/// redacted before it's written to that row (see `redact_for_audit`), since the audit log is
/// readable through the Console UI's audit viewer and a tenant's AI provider credential has no
/// business living there in plaintext, unlike the rest of this config.
#[async_trait]
pub trait AnalysisConfigRepository: Send + Sync {
    async fn upsert(
        &self,
        config: AnalysisConfig,
        actor: &str,
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

/// A tenant's `api_key` never appears in the audit log — everything else about their AI
/// analysis config does.
fn redact_for_audit(config: &AnalysisConfig) -> serde_json::Value {
    let mut value = serde_json::to_value(config).unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        if obj.get("api_key").and_then(|v| v.as_str()).is_some() {
            obj.insert("api_key".to_string(), serde_json::Value::String("<redacted>".to_string()));
        }
    }
    value
}

#[async_trait]
impl AnalysisConfigRepository for PostgresAnalysisConfigRepository {
    async fn upsert(
        &self,
        config: AnalysisConfig,
        actor: &str,
    ) -> Result<AnalysisConfig, AnalysisConfigRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;

        let existing: Option<AnalysisConfigRow> = sqlx::query_as(
            "SELECT tenant_id, prompt, updated_at, provider, model, endpoint, api_key FROM analysis_configs WHERE tenant_id = $1",
        )
        .bind(config.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        let before = existing.map(row_to_config);

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
                actor: actor.to_string(),
                before: before.as_ref().map(redact_for_audit),
                after: redact_for_audit(&config),
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
            "SELECT tenant_id, prompt, updated_at, provider, model, endpoint, api_key FROM analysis_configs WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AnalysisConfigRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_config))
    }
}
