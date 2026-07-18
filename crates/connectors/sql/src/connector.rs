#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::connector::{Connector, ConnectorError};
use common::raw_record::{RawRecord, SourceType};
use sqlx::postgres::PgRow;
use sqlx::{Column, PgPool, Row};

/// Polls an arbitrary customer database with an operator-configured `SELECT` (ADR-0013) and
/// maps each row to a `RawRecord`, columns verbatim as `raw_payload`. Deliberately stateless —
/// incremental polling (a watermark/cursor) is the *query's* responsibility (e.g. a
/// `WHERE updated_at > now() - interval '1 hour'` baked into the configured query), not this
/// connector's, since each CronJob invocation is a fresh process with no persisted state to
/// track a cursor in (spec §3's one-shot poller model). Fabric's SQL analytics endpoint
/// (ADR-0003, ADR-0013) reuses this same row-mapping logic behind Entra token auth.
pub struct SqlConnector {
    connector_id: String,
    pool: PgPool,
    query: String,
}

impl SqlConnector {
    pub fn new(connector_id: impl Into<String>, pool: PgPool, query: impl Into<String>) -> Self {
        Self { connector_id: connector_id.into(), pool, query: query.into() }
    }
}

#[async_trait]
impl Connector for SqlConnector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::SqlRow
    }

    async fn poll(&self, tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        let rows = sqlx::query(&self.query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                RawRecord::new(
                    self.connector_id.clone(),
                    SourceType::SqlRow,
                    tenant_id,
                    row_to_json(&row),
                )
            })
            .collect())
    }
}

/// Best-effort dynamic column decoding — sqlx has no "decode as JSON regardless of column
/// type" primitive, so this tries the common scalar types in order and falls back to a string
/// decode, then finally `null` for anything it can't decode (e.g. a type this connector
/// doesn't know about yet). Good enough for the row shapes any reasonable operator-configured
/// query produces; not a claim of covering every Postgres type.
fn row_to_json(row: &PgRow) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name();
        let index = column.ordinal();
        let value = if let Ok(v) = row.try_get::<Option<i64>, _>(index) {
            v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null)
        } else if let Ok(v) = row.try_get::<Option<f64>, _>(index) {
            v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null)
        } else if let Ok(v) = row.try_get::<Option<bool>, _>(index) {
            v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null)
        } else if let Ok(v) = row.try_get::<Option<DateTime<Utc>>, _>(index) {
            v.map(|dt| serde_json::Value::String(dt.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null)
        } else if let Ok(v) = row.try_get::<Option<String>, _>(index) {
            v.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };
        object.insert(name.to_string(), value);
    }
    serde_json::Value::Object(object)
}
