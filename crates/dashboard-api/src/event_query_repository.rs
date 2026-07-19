#[path = "event_query_repository_test.rs"]
#[cfg(test)]
pub(crate) mod event_query_repository_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::Event;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("clickhouse unreachable: {0}")]
    Unreachable(String),
    #[error("clickhouse rejected the query: HTTP {0}: {1}")]
    Rejected(u16, String),
    #[error("failed to parse clickhouse response: {0}")]
    Parse(String),
}

/// One bucket of the events-over-time chart on the Events page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyEventCount {
    pub date: chrono::NaiveDate,
    pub count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    pub event_type: Option<String>,
    pub group_key: Option<String>,
    pub status: Option<common::EventStatus>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// Matches Events whose `record_ids` contains this `RawRecord` id — the record→event
    /// lineage lookup (ADR-0017) a record-journey view uses to find what a record contributed
    /// to.
    pub record_id: Option<Uuid>,
    pub limit: u32,
    pub offset: u32,
}

/// Reads Events from the aggregate store (spec §5.2, ClickHouse) for dashboards/reports/event
/// browsing (spec §6, service #9). Every query is tenant-scoped — the caller (Query Gateway)
/// has already resolved the caller's identity to a tenant_id, and every method here requires
/// one, so there is no code path that can accidentally read across tenants (spec §8).
#[async_trait]
pub trait EventQueryRepository: Send + Sync {
    async fn list_events(
        &self,
        tenant_id: Uuid,
        filter: &EventFilter,
    ) -> Result<Vec<Event>, QueryError>;
    async fn get_event(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Event>, QueryError>;

    /// Daily event counts within `[since, until]`, ascending by date, optionally scoped to one
    /// `event_type` — powers the Events page's over-time chart.
    async fn count_by_day(
        &self,
        tenant_id: Uuid,
        event_type: Option<&str>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<DailyEventCount>, QueryError>;
}

pub struct ClickHouseEventQueryRepository {
    client: reqwest::Client,
    base_url: String,
}

impl ClickHouseEventQueryRepository {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    async fn run_query(
        &self,
        query: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<ClickHouseEventRow>, QueryError> {
        // ClickHouse's HTTP interface requires an explicit Content-Length on POST requests;
        // reqwest omits it for a zero-length body, so a bodyless POST (query entirely in the
        // query string) gets rejected with 411 Length Required.
        let mut request = self
            .client
            .post(&self.base_url)
            .query(&[("query", query)])
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body(Vec::new());
        for (key, value) in params {
            request = request.query(&[(format!("param_{key}"), value.clone())]);
        }
        let response = request.send().await.map_err(|e| QueryError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(QueryError::Rejected(status, body));
        }

        let body = response.text().await.map_err(|e| QueryError::Unreachable(e.to_string()))?;
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|e| QueryError::Parse(e.to_string())))
            .collect()
    }
}

#[derive(serde::Deserialize)]
struct ClickHouseEventRow {
    id: Uuid,
    tenant_id: Uuid,
    event_type: String,
    source_connector_ids: Vec<String>,
    entity_ref: String,
    group_key: String,
    payload: String,
    occurred_at: String,
    created_at: String,
    status: String,
    #[serde(default)]
    record_ids: Vec<Uuid>,
}

impl TryFrom<ClickHouseEventRow> for Event {
    type Error = QueryError;

    fn try_from(row: ClickHouseEventRow) -> Result<Self, Self::Error> {
        let status = match row.status.as_str() {
            "new" => common::EventStatus::New,
            "triggered" => common::EventStatus::Triggered,
            "actioned" => common::EventStatus::Actioned,
            "dismissed" => common::EventStatus::Dismissed,
            other => return Err(QueryError::Parse(format!("unknown event status `{other}`"))),
        };
        Ok(Event {
            id: row.id,
            tenant_id: row.tenant_id,
            event_type: row.event_type,
            source_connector_ids: row.source_connector_ids,
            entity_ref: row.entity_ref,
            group_key: row.group_key,
            payload: serde_json::from_str(&row.payload)
                .map_err(|e| QueryError::Parse(e.to_string()))?,
            occurred_at: parse_clickhouse_datetime(&row.occurred_at)?,
            created_at: parse_clickhouse_datetime(&row.created_at)?,
            status,
            record_ids: row.record_ids,
        })
    }
}

fn parse_clickhouse_datetime(s: &str) -> Result<DateTime<Utc>, QueryError> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .map(|naive| naive.and_utc())
        .map_err(|e| QueryError::Parse(e.to_string()))
}

#[async_trait]
impl EventQueryRepository for ClickHouseEventQueryRepository {
    async fn list_events(
        &self,
        tenant_id: Uuid,
        filter: &EventFilter,
    ) -> Result<Vec<Event>, QueryError> {
        let mut conditions = vec!["tenant_id = {tenant_id:UUID}".to_string()];
        let mut params = vec![("tenant_id".to_string(), tenant_id.to_string())];

        if let Some(event_type) = &filter.event_type {
            conditions.push("event_type = {event_type:String}".to_string());
            params.push(("event_type".to_string(), event_type.clone()));
        }
        if let Some(group_key) = &filter.group_key {
            conditions.push("group_key = {group_key:String}".to_string());
            params.push(("group_key".to_string(), group_key.clone()));
        }
        if let Some(status) = filter.status {
            conditions.push("status = {status:String}".to_string());
            params.push(("status".to_string(), status_str(status).to_string()));
        }
        if let Some(since) = filter.since {
            conditions.push("occurred_at >= {since:DateTime64}".to_string());
            params.push(("since".to_string(), since.format("%Y-%m-%d %H:%M:%S%.3f").to_string()));
        }
        if let Some(until) = filter.until {
            conditions.push("occurred_at <= {until:DateTime64}".to_string());
            params.push(("until".to_string(), until.format("%Y-%m-%d %H:%M:%S%.3f").to_string()));
        }
        if let Some(record_id) = filter.record_id {
            conditions.push("has(record_ids, {record_id:UUID})".to_string());
            params.push(("record_id".to_string(), record_id.to_string()));
        }

        let limit = filter.limit.clamp(1, 1000);
        let query = format!(
            "SELECT id, tenant_id, event_type, source_connector_ids, entity_ref, group_key, payload, occurred_at, created_at, status, record_ids FROM events WHERE {} ORDER BY occurred_at DESC LIMIT {} OFFSET {} FORMAT JSONEachRow",
            conditions.join(" AND "),
            limit,
            filter.offset
        );
        let params_ref: Vec<(&str, String)> =
            params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let rows = self.run_query(&query, &params_ref).await?;
        rows.into_iter().map(Event::try_from).collect()
    }

    async fn get_event(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Event>, QueryError> {
        let query = "SELECT id, tenant_id, event_type, source_connector_ids, entity_ref, group_key, payload, occurred_at, created_at, status, record_ids FROM events WHERE tenant_id = {tenant_id:UUID} AND id = {id:UUID} LIMIT 1 FORMAT JSONEachRow";
        let rows = self
            .run_query(query, &[("tenant_id", tenant_id.to_string()), ("id", id.to_string())])
            .await?;
        rows.into_iter().next().map(Event::try_from).transpose()
    }

    async fn count_by_day(
        &self,
        tenant_id: Uuid,
        event_type: Option<&str>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<DailyEventCount>, QueryError> {
        let mut conditions = vec![
            "tenant_id = {tenant_id:UUID}".to_string(),
            "occurred_at >= {since:DateTime64}".to_string(),
            "occurred_at <= {until:DateTime64}".to_string(),
        ];
        let mut params = vec![
            ("tenant_id".to_string(), tenant_id.to_string()),
            ("since".to_string(), since.format("%Y-%m-%d %H:%M:%S%.3f").to_string()),
            ("until".to_string(), until.format("%Y-%m-%d %H:%M:%S%.3f").to_string()),
        ];
        if let Some(event_type) = event_type {
            conditions.push("event_type = {event_type:String}".to_string());
            params.push(("event_type".to_string(), event_type.to_string()));
        }

        let query = format!(
            "SELECT toDate(occurred_at) AS d, count() AS c FROM events WHERE {} GROUP BY d ORDER BY d FORMAT JSONEachRow",
            conditions.join(" AND "),
        );
        let params_ref: Vec<(&str, String)> =
            params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

        let request = self
            .client
            .post(&self.base_url)
            .query(&[("query", &query)])
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body(Vec::new());
        let request = params_ref
            .iter()
            .fold(request, |req, (k, v)| req.query(&[(format!("param_{k}"), v.clone())]));
        let response = request.send().await.map_err(|e| QueryError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(QueryError::Rejected(status, body));
        }
        let body = response.text().await.map_err(|e| QueryError::Unreachable(e.to_string()))?;

        // ClickHouse's JSONEachRow format serializes UInt64 (what count() returns) as a
        // quoted JSON string, not a number — JS can't represent a full 64-bit int precisely,
        // so ClickHouse always quotes it regardless of client. `c` must deserialize from a
        // string, not u64 directly, or every row here fails to parse (caught live: this threw
        // `invalid type: string "2", expected u64` against the real deployed stack).
        #[derive(serde::Deserialize)]
        struct Row {
            d: String,
            c: String,
        }
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let row: Row =
                    serde_json::from_str(line).map_err(|e| QueryError::Parse(e.to_string()))?;
                let date = chrono::NaiveDate::parse_from_str(&row.d, "%Y-%m-%d")
                    .map_err(|e| QueryError::Parse(e.to_string()))?;
                let count: u64 = row
                    .c
                    .parse()
                    .map_err(|e: std::num::ParseIntError| QueryError::Parse(e.to_string()))?;
                Ok(DailyEventCount { date, count })
            })
            .collect()
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
