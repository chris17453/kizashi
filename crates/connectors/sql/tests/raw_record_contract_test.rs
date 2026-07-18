//! Integration test against real Postgres (CLAUDE.md §2) standing in for an arbitrary source
//! database — this connector's whole job is running a real SQL query, so there's no in-memory
//! double to substitute (ADR-0013). Requires DATABASE_URL.

use common::connector::Connector;
use common::raw_record::RawRecord;
use connector_sql::SqlConnector;
use sqlx::PgPool;

async fn test_pool() -> PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    // A single-connection pool so the CREATE TEMP TABLE below stays visible to the SELECT the
    // connector runs — temp tables are connection-scoped, and a multi-connection pool could
    // otherwise route the two statements to different backend connections.
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("failed to connect to postgres")
}

#[tokio::test]
async fn poll_returns_records_conforming_to_raw_record_schema() {
    let pool = test_pool().await;
    sqlx::query("CREATE TEMP TABLE contract_test_rows (id BIGINT, label TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO contract_test_rows (id, label) VALUES (1, 'a'), (2, 'b')")
        .execute(&pool)
        .await
        .unwrap();

    let connector =
        SqlConnector::new("sql", pool, "SELECT id, label FROM contract_test_rows ORDER BY id");
    let tenant_id = uuid::Uuid::new_v4();

    let records: Vec<RawRecord> = connector.poll(tenant_id).await.expect("poll should not error");

    assert_eq!(records.len(), 2);
    for r in &records {
        assert_eq!(r.tenant_id, tenant_id);
        assert_eq!(r.connector_id, connector.connector_id());
    }
    assert_eq!(records[0].raw_payload["id"], 1);
    assert_eq!(records[0].raw_payload["label"], "a");
}
