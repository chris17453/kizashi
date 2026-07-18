use axum::response::Json;
use axum::routing::get;
use axum::Router;
use common::connector::Connector;
use common::raw_record::RawRecord;
use connector_zendesk::ZendeskConnector;

async fn spawn_stub_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"tickets": [{"id": 1}, {"id": 2}], "end_time": 0, "count": 2}))
    }
    let app = Router::new().route("/api/v2/incremental/tickets.json", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn poll_returns_records_conforming_to_raw_record_schema() {
    let base_url = spawn_stub_server().await;
    let connector = ZendeskConnector::new(
        "zendesk",
        reqwest::Client::new(),
        base_url,
        "agent@example.com",
        "api-token",
        0,
    );
    let tenant_id = uuid::Uuid::new_v4();

    let records: Vec<RawRecord> = connector.poll(tenant_id).await.expect("poll should not error");

    assert!(!records.is_empty());
    for r in records {
        assert_eq!(r.tenant_id, tenant_id);
        assert_eq!(r.connector_id, connector.connector_id());
    }
}
