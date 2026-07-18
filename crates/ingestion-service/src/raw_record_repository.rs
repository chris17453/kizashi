#[path = "raw_record_repository_test.rs"]
#[cfg(test)]
pub(crate) mod raw_record_repository_test;

use async_trait::async_trait;
use common::RawRecord;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Persists RawRecords to the hot store (spec §5.1). Ingestion Service's only write path to
/// Postgres — abstracted behind a trait so handler logic is unit-testable without a live DB,
/// per CLAUDE.md §2's requirement that unit tests not require the docker-compose stack.
#[async_trait]
pub trait RawRecordRepository: Send + Sync {
    async fn insert(&self, record: &RawRecord) -> Result<(), RepositoryError>;

    /// Sets `normalized_payload` for a previously-ingested record. This is the only write
    /// path onto `raw_records` from outside Ingestion Service — Normalization Service calls
    /// it over HTTP rather than touching Postgres directly (spec §2 principle 1, "API-mediated
    /// everything"). Returns `Ok(false)` if no record with that id exists, rather than an
    /// error, so callers can distinguish "not found" from a backend failure.
    async fn update_normalized_payload(
        &self,
        record_id: uuid::Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<bool, RepositoryError>;
}

pub struct PostgresRawRecordRepository {
    pool: sqlx::PgPool,
}

impl PostgresRawRecordRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RawRecordRepository for PostgresRawRecordRepository {
    async fn insert(&self, record: &RawRecord) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO raw_records
                (id, connector_id, source_type, ingested_at, occurred_at, raw_payload,
                 normalized_payload, tenant_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(record.id)
        .bind(&record.connector_id)
        .bind(sqlx::types::Json(record.source_type))
        .bind(record.ingested_at)
        .bind(record.occurred_at)
        .bind(&record.raw_payload)
        .bind(&record.normalized_payload)
        .bind(record.tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn update_normalized_payload(
        &self,
        record_id: uuid::Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query("UPDATE raw_records SET normalized_payload = $1 WHERE id = $2")
            .bind(normalized_payload)
            .bind(record_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}
