use super::*;

/// `connect_lazy` never opens a socket, so metadata-only assertions don't need a live Postgres
/// instance — real query execution is covered by `tests/sql_connector_integration_test.rs`
/// against a real database (sqlx has no in-memory double to substitute here; querying an
/// actual database *is* this connector's logic, per ADR-0013).
fn lazy_connector(connector_id: &str, query: &str) -> SqlConnector {
    let pool = PgPool::connect_lazy("postgres://kizashi:kizashi@localhost:5432/kizashi").unwrap();
    SqlConnector::new(connector_id, pool, query)
}

#[tokio::test]
async fn reports_its_own_connector_id() {
    let c = lazy_connector("sql", "SELECT 1");
    assert_eq!(c.connector_id(), "sql");
}

#[tokio::test]
async fn reports_sql_row_as_its_source_type() {
    let c = lazy_connector("sql", "SELECT 1");
    assert_eq!(c.source_type(), SourceType::SqlRow);
}
