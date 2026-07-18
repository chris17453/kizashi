#[path = "signal_repository_test.rs"]
#[cfg(test)]
pub(crate) mod signal_repository_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SignalRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedSignal {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub record_id: Uuid,
    pub event_type: String,
    pub group_key: String,
    pub entity_ref: String,
    pub numeric_value: Option<f64>,
    pub source_connector_id: String,
    pub occurred_at: DateTime<Utc>,
}

/// Durable, window-queryable log of every classified signal Trigger Engine has seen (ADR-0006).
/// This is what `TriggerCondition::CountOverWindow`/`ThresholdOverWindow` are evaluated
/// against — count and numeric values for a (tenant, event_type, group_key) within a rolling
/// window.
#[async_trait]
pub trait SignalRepository: Send + Sync {
    async fn record_signal(&self, signal: &AnalyzedSignal) -> Result<(), SignalRepositoryError>;

    /// Returns (count of matching signals, their numeric values where present) for the given
    /// tenant/event_type/group_key within the last `window_seconds`.
    async fn window_stats(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        group_key: &str,
        window_seconds: i64,
    ) -> Result<(u32, Vec<f64>), SignalRepositoryError>;
}

pub struct PostgresSignalRepository {
    pool: sqlx::PgPool,
}

impl PostgresSignalRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SignalRepository for PostgresSignalRepository {
    async fn record_signal(&self, signal: &AnalyzedSignal) -> Result<(), SignalRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO analyzed_signals
                (id, tenant_id, record_id, event_type, group_key, entity_ref, numeric_value,
                 source_connector_id, occurred_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(signal.id)
        .bind(signal.tenant_id)
        .bind(signal.record_id)
        .bind(&signal.event_type)
        .bind(&signal.group_key)
        .bind(&signal.entity_ref)
        .bind(signal.numeric_value)
        .bind(&signal.source_connector_id)
        .bind(signal.occurred_at)
        .execute(&self.pool)
        .await
        .map_err(|e| SignalRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn window_stats(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        group_key: &str,
        window_seconds: i64,
    ) -> Result<(u32, Vec<f64>), SignalRepositoryError> {
        let rows: Vec<(Option<f64>,)> = sqlx::query_as(
            r#"
            SELECT numeric_value
            FROM analyzed_signals
            WHERE tenant_id = $1
              AND event_type = $2
              AND group_key = $3
              AND occurred_at >= now() - make_interval(secs => $4)
            "#,
        )
        .bind(tenant_id)
        .bind(event_type)
        .bind(group_key)
        .bind(window_seconds as f64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SignalRepositoryError::Backend(e.to_string()))?;

        let count = rows.len() as u32;
        let values = rows.into_iter().filter_map(|(v,)| v).collect();
        Ok((count, values))
    }
}
