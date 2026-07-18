use axum::response::Json;
use axum::routing::get;
use axum::Router;
use common::connector::Connector;
use common::raw_record::RawRecord;
use connector_generic::GenericConnector;

async fn spawn_stub_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!([{"subject": "one"}, {"subject": "two"}]))
    }
    let app = Router::new().route("/items", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/items")
}

#[tokio::test]
async fn poll_returns_records_conforming_to_raw_record_schema() {
    let url = spawn_stub_server().await;
    let connector = GenericConnector::new("generic", reqwest::Client::new(), url, None);
    let tenant_id = uuid::Uuid::new_v4();

    let records: Vec<RawRecord> = connector.poll(tenant_id).await.expect("poll should not error");

    assert!(!records.is_empty());
    for r in records {
        assert_eq!(r.tenant_id, tenant_id);
        assert_eq!(r.connector_id, connector.connector_id());
    }
}
