//! Integration test against real ClickHouse (CLAUDE.md §2). Requires CLICKHOUSE_URL.
//!
//! `dashboard-api` never writes to the `events` table itself (that's trigger-engine's job,
//! ADR-0006) — the schema here mirrors `trigger_engine::ClickHouseEventStore::ensure_schema`
//! exactly, and this test writes a row directly via ClickHouse's HTTP insert interface to
//! stand in for what trigger-engine would have written, then proves the read side
//! (`ClickHouseEventQueryRepository`) parses it back correctly, including `record_ids` — the
//! record→event lineage field.

use chrono::Utc;
use dashboard_api::{ClickHouseEventQueryRepository, EventFilter, EventQueryRepository};
use uuid::Uuid;

async fn ensure_schema(client: &reqwest::Client, base_url: &str) {
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
            status String,
            record_ids Array(UUID)
        ) ENGINE = MergeTree() ORDER BY (tenant_id, occurred_at)
    "#;
    let response = client
        .post(base_url)
        .body(ddl.to_string())
        .send()
        .await
        .expect("failed to reach clickhouse");
    assert!(response.status().is_success(), "failed to ensure schema: {:?}", response.text().await);
}

async fn insert_event_row(
    client: &reqwest::Client,
    base_url: &str,
    id: Uuid,
    tenant_id: Uuid,
    record_ids: &[Uuid],
) {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let row = serde_json::json!({
        "id": id,
        "tenant_id": tenant_id,
        "event_type": "sentiment",
        "source_connector_ids": ["zendesk"],
        "entity_ref": "cust-integration-test",
        "group_key": "cust-integration-test",
        "payload": serde_json::to_string(&serde_json::json!({"value": -0.9})).unwrap(),
        "occurred_at": now,
        "created_at": now,
        "status": "new",
        "record_ids": record_ids,
    });
    let response = client
        .post(base_url)
        .query(&[("query", "INSERT INTO events FORMAT JSONEachRow")])
        .body(serde_json::to_vec(&row).unwrap())
        .send()
        .await
        .expect("failed to reach clickhouse");
    assert!(response.status().is_success(), "insert failed: {:?}", response.text().await);
}

#[tokio::test]
async fn list_and_get_events_round_trip_against_real_clickhouse_including_record_ids() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let base_url = format!("{clickhouse_url}/");
    let client = reqwest::Client::new();
    ensure_schema(&client, &base_url).await;

    let tenant_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let record_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    insert_event_row(&client, &base_url, event_id, tenant_id, &record_ids).await;

    let repo = ClickHouseEventQueryRepository::new(client, base_url);

    let found = repo
        .get_event(tenant_id, event_id)
        .await
        .expect("get_event failed")
        .expect("event not found");
    assert_eq!(found.id, event_id);
    assert_eq!(found.tenant_id, tenant_id);
    let mut found_record_ids = found.record_ids.clone();
    found_record_ids.sort();
    let mut expected = record_ids.clone();
    expected.sort();
    assert_eq!(found_record_ids, expected, "record_ids must round-trip through ClickHouse");

    let listed = repo
        .list_events(tenant_id, &EventFilter { limit: 10, ..Default::default() })
        .await
        .expect("list_events failed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, event_id);
}

#[tokio::test]
async fn list_events_filters_by_record_id_against_real_clickhouse() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let base_url = format!("{clickhouse_url}/");
    let client = reqwest::Client::new();
    ensure_schema(&client, &base_url).await;

    let tenant_id = Uuid::new_v4();
    let target_record_id = Uuid::new_v4();
    let matching_event = Uuid::new_v4();
    let other_event = Uuid::new_v4();
    insert_event_row(&client, &base_url, matching_event, tenant_id, &[target_record_id]).await;
    insert_event_row(&client, &base_url, other_event, tenant_id, &[Uuid::new_v4()]).await;

    let repo = ClickHouseEventQueryRepository::new(client, base_url);
    let found = repo
        .list_events(
            tenant_id,
            &EventFilter { record_id: Some(target_record_id), limit: 10, ..Default::default() },
        )
        .await
        .expect("list_events failed");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, matching_event);
}

#[tokio::test]
async fn get_event_returns_none_for_an_unknown_id_against_real_clickhouse() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let base_url = format!("{clickhouse_url}/");
    let client = reqwest::Client::new();
    ensure_schema(&client, &base_url).await;

    let repo = ClickHouseEventQueryRepository::new(client, base_url);
    let found = repo.get_event(Uuid::new_v4(), Uuid::new_v4()).await.expect("get_event failed");
    assert!(found.is_none());
}
