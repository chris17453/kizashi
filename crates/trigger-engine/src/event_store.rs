#[path = "event_store_test.rs"]
#[cfg(test)]
pub(crate) mod event_store_test;

use async_trait::async_trait;
use common::Event;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventStoreError {
    #[error("clickhouse unreachable: {0}")]
    Unreachable(String),
    #[error("clickhouse rejected the insert: HTTP {0}: {1}")]
    Rejected(u16, String),
    #[error("failed to serialize event: {0}")]
    Serialization(String),
}

/// Writes Events to the aggregate store (spec §5.2, ClickHouse). This is the durable record a
/// firing TriggerDefinition produces, read later by Query Gateway / Dashboard API.
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn insert_event(&self, event: &Event) -> Result<(), EventStoreError>;
}

#[derive(serde::Serialize)]
struct ClickHouseEventRow<'a> {
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    event_type: &'a str,
    source_connector_ids: &'a [String],
    entity_ref: &'a str,
    group_key: &'a str,
    payload: String,
    occurred_at: String,
    created_at: String,
    status: &'a str,
}

pub struct ClickHouseEventStore {
    client: reqwest::Client,
    base_url: String,
}

impl ClickHouseEventStore {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    pub async fn ensure_schema(&self) -> Result<(), EventStoreError> {
        let ddl = r#"
            CREATE TABLE IF NOT EXISTS events (
                id UUID,
                tenant_id UUID,
                event_type String,
                source_connector_ids Array(String),
                entity_ref String,
                group_key String,
                payload String,
                occurred_at DateTime64(3),
                created_at DateTime64(3),
                status String
            ) ENGINE = MergeTree() ORDER BY (tenant_id, occurred_at)
        "#;
        let response = self
            .client
            .post(&self.base_url)
            .body(ddl.to_string())
            .send()
            .await
            .map_err(|e| EventStoreError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EventStoreError::Rejected(status, body));
        }
        Ok(())
    }
}

fn status_str(status: common::EventStatus) -> &'static str {
    match status {
        common::EventStatus::New => "new",
        common::EventStatus::Triggered => "triggered",
        common::EventStatus::Actioned => "actioned",
        common::EventStatus::Dismissed => "dismissed",
    }
}

#[async_trait]
impl EventStore for ClickHouseEventStore {
    async fn insert_event(&self, event: &Event) -> Result<(), EventStoreError> {
        let row = ClickHouseEventRow {
            id: event.id,
            tenant_id: event.tenant_id,
            event_type: &event.event_type,
            source_connector_ids: &event.source_connector_ids,
            entity_ref: &event.entity_ref,
            group_key: &event.group_key,
            payload: serde_json::to_string(&event.payload)
                .map_err(|e| EventStoreError::Serialization(e.to_string()))?,
            occurred_at: event.occurred_at.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            created_at: event.created_at.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            status: status_str(event.status),
        };
        let body =
            serde_json::to_vec(&row).map_err(|e| EventStoreError::Serialization(e.to_string()))?;

        let response = self
            .client
            .post(&self.base_url)
            .query(&[("query", "INSERT INTO events FORMAT JSONEachRow")])
            .body(body)
            .send()
            .await
            .map_err(|e| EventStoreError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EventStoreError::Rejected(status, body));
        }
        Ok(())
    }
}
