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

/// Per-connector aggregate derived from `raw_records` — this is how an Agent's live status
/// (last run, volume) is computed. There is no separate "agent run" bookkeeping table; a
/// connector's own ingested records are already the ground truth for whether/when it ran, so
/// this reads that data rather than requiring connectors to self-report a heartbeat.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConnectorStats {
    pub connector_id: String,
    pub record_count: i64,
    pub last_ingested_at: chrono::DateTime<chrono::Utc>,
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

    /// Lists up to `limit` records for `tenant_id` with `ingested_at` before `cutoff`, oldest
    /// first — the read side of Retention/Archival Service's sweep (spec §6, service #12).
    /// Retention Service never touches this table directly (spec §2 principle 1); this is the
    /// HTTP-mediated read path, mirroring how `update_normalized_payload` is Normalization
    /// Service's write path. Tenant-scoped like every other query path (spec §8, CLAUDE.md §5).
    async fn list_older_than(
        &self,
        tenant_id: uuid::Uuid,
        cutoff: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError>;

    /// Hard-deletes a record after it has been durably archived (spec §9 disposal), scoped to
    /// `tenant_id` so one tenant's sweep can never delete another tenant's record. Returns
    /// `Ok(false)` if no matching record exists, rather than an error, matching
    /// `update_normalized_payload`'s not-found convention.
    async fn delete(
        &self,
        tenant_id: uuid::Uuid,
        record_id: uuid::Uuid,
    ) -> Result<bool, RepositoryError>;

    /// Aggregates `raw_records` by `connector_id` for `tenant_id` — powers the Agent status
    /// view (record count and last-ingested time per registered agent).
    async fn stats_by_connector(
        &self,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<ConnectorStats>, RepositoryError>;

    /// Lists up to `limit` records for one `connector_id`/`tenant_id`, newest first — the
    /// per-agent data drill-down view.
    async fn list_by_connector(
        &self,
        tenant_id: uuid::Uuid,
        connector_id: &str,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError>;

    /// Fetches a single record, tenant-scoped — the Data Viewer's detail view.
    async fn get_by_id(
        &self,
        tenant_id: uuid::Uuid,
        record_id: uuid::Uuid,
    ) -> Result<Option<RawRecord>, RepositoryError>;

    /// The Data Viewer's search: every filter is optional and AND-ed together — connector,
    /// source type, an ingested-at range, and a substring match against the raw payload's
    /// text representation. Deliberately a plain `ILIKE` scan rather than a dedicated search
    /// index (Elasticsearch, pg_trgm, tsvector): v1 scope is "find records that mention X,"
    /// not sub-second full-text relevance ranking over a large corpus.
    async fn search(
        &self,
        tenant_id: uuid::Uuid,
        filter: &RecordSearchFilter,
    ) -> Result<Vec<RawRecord>, RepositoryError>;
}

#[derive(Debug, Clone, Default)]
pub struct RecordSearchFilter {
    pub connector_id: Option<String>,
    pub source_type: Option<common::SourceType>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub query: Option<String>,
    pub limit: i64,
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

    async fn list_older_than(
        &self,
        tenant_id: uuid::Uuid,
        cutoff: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        let rows: Vec<RawRecordRow> = sqlx::query_as(
            r#"
            SELECT id, connector_id, source_type, ingested_at, occurred_at, raw_payload,
                   normalized_payload, tenant_id
            FROM raw_records
            WHERE tenant_id = $1 AND ingested_at < $2
            ORDER BY ingested_at ASC
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(cutoff)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }

    async fn delete(
        &self,
        tenant_id: uuid::Uuid,
        record_id: uuid::Uuid,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query("DELETE FROM raw_records WHERE id = $1 AND tenant_id = $2")
            .bind(record_id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn stats_by_connector(
        &self,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<ConnectorStats>, RepositoryError> {
        let rows: Vec<(String, i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            r#"
            SELECT connector_id, COUNT(*), MAX(ingested_at)
            FROM raw_records
            WHERE tenant_id = $1
            GROUP BY connector_id
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(connector_id, record_count, last_ingested_at)| ConnectorStats {
                connector_id,
                record_count,
                last_ingested_at,
            })
            .collect())
    }

    async fn list_by_connector(
        &self,
        tenant_id: uuid::Uuid,
        connector_id: &str,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        let rows: Vec<RawRecordRow> = sqlx::query_as(
            r#"
            SELECT id, connector_id, source_type, ingested_at, occurred_at, raw_payload,
                   normalized_payload, tenant_id
            FROM raw_records
            WHERE tenant_id = $1 AND connector_id = $2
            ORDER BY ingested_at DESC
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(connector_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }

    async fn get_by_id(
        &self,
        tenant_id: uuid::Uuid,
        record_id: uuid::Uuid,
    ) -> Result<Option<RawRecord>, RepositoryError> {
        let row: Option<RawRecordRow> = sqlx::query_as(
            r#"
            SELECT id, connector_id, source_type, ingested_at, occurred_at, raw_payload,
                   normalized_payload, tenant_id
            FROM raw_records
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(record_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_record))
    }

    async fn search(
        &self,
        tenant_id: uuid::Uuid,
        filter: &RecordSearchFilter,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        let rows: Vec<RawRecordRow> = sqlx::query_as(
            r#"
            SELECT id, connector_id, source_type, ingested_at, occurred_at, raw_payload,
                   normalized_payload, tenant_id
            FROM raw_records
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR connector_id = $2)
              AND ($3::jsonb IS NULL OR source_type = $3)
              AND ($4::timestamptz IS NULL OR ingested_at >= $4)
              AND ($5::timestamptz IS NULL OR ingested_at <= $5)
              AND ($6::text IS NULL OR raw_payload::text ILIKE '%' || $6 || '%')
            ORDER BY ingested_at DESC
            LIMIT $7
            "#,
        )
        .bind(tenant_id)
        .bind(&filter.connector_id)
        .bind(filter.source_type.map(sqlx::types::Json))
        .bind(filter.from)
        .bind(filter.to)
        .bind(&filter.query)
        .bind(filter.limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }
}

type RawRecordRow = (
    uuid::Uuid,
    String,
    sqlx::types::Json<common::SourceType>,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
    serde_json::Value,
    Option<serde_json::Value>,
    uuid::Uuid,
);

fn row_to_record(row: RawRecordRow) -> RawRecord {
    let (
        id,
        connector_id,
        source_type,
        ingested_at,
        occurred_at,
        raw_payload,
        normalized_payload,
        tenant_id,
    ) = row;
    RawRecord {
        id,
        connector_id,
        source_type: source_type.0,
        ingested_at,
        occurred_at,
        raw_payload,
        normalized_payload,
        tenant_id,
    }
}
