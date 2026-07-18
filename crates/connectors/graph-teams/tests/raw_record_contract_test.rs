use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use common::connector::Connector;
use common::raw_record::RawRecord;
use connector_graph_teams::GraphTeamsConnector;

async fn spawn_stub_token_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(
            serde_json::json!({"access_token": "stub-token", "token_type": "bearer", "expires_in": 3600}),
        )
    }
    let app = Router::new().route("/token", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/token")
}

async fn spawn_stub_graph_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"value": [{"id": "1"}, {"id": "2"}]}))
    }
    let app = Router::new().route("/teams/:team_id/channels/:channel_id/messages", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn poll_returns_records_conforming_to_raw_record_schema() {
    let token_url = spawn_stub_token_server().await;
    let graph_url = spawn_stub_graph_server().await;
    let connector = GraphTeamsConnector::new(
        "graph-teams",
        reqwest::Client::new(),
        graph_url,
        token_url,
        "client-id",
        "client-secret",
        "team-1",
        "channel-1",
    );
    let tenant_id = uuid::Uuid::new_v4();

    let records: Vec<RawRecord> = connector.poll(tenant_id).await.expect("poll should not error");

    assert!(!records.is_empty());
    for r in records {
        assert_eq!(r.tenant_id, tenant_id);
        assert_eq!(r.connector_id, connector.connector_id());
    }
}
