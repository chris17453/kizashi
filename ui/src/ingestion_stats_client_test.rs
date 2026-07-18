use super::*;
use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryIngestionStatsClient {
    pub stats: Mutex<Vec<ConnectorStatSummary>>,
    pub records: Mutex<Vec<RecordSummary>>,
}

#[async_trait]
impl IngestionStatsClient for InMemoryIngestionStatsClient {
    async fn connector_stats(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ConnectorStatSummary>, IngestionStatsClientError> {
        Ok(self.stats.lock().unwrap().clone())
    }

    async fn records_by_connector(
        &self,
        _tenant_id: Uuid,
        _connector_id: &str,
    ) -> Result<Vec<RecordSummary>, IngestionStatsClientError> {
        Ok(self.records.lock().unwrap().clone())
    }
}

pub struct FailingIngestionStatsClient;

#[async_trait]
impl IngestionStatsClient for FailingIngestionStatsClient {
    async fn connector_stats(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ConnectorStatSummary>, IngestionStatsClientError> {
        Err(IngestionStatsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn records_by_connector(
        &self,
        _tenant_id: Uuid,
        _connector_id: &str,
    ) -> Result<Vec<RecordSummary>, IngestionStatsClientError> {
        Err(IngestionStatsClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn stats_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "connector_id": "zendesk",
            "record_count": 42,
            "last_ingested_at": "2026-07-18T12:00:00Z"
        }]))
        .into_response()
    }
    async fn by_connector_handler(
        Query(params): Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "source_type": "ticket",
            "ingested_at": "2026-07-18T12:00:00Z",
            "normalized_payload": null,
            "connector_id": params.get("connector_id")
        }]))
        .into_response()
    }
    let app = Router::new()
        .route("/v1/records/stats", get(stats_handler))
        .route("/v1/records/by-connector", get(by_connector_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_reads_connector_stats_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIngestionStatsClient::new(reqwest::Client::new(), url);

    let stats = client.connector_stats(Uuid::new_v4()).await.unwrap();

    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].connector_id, "zendesk");
    assert_eq!(stats[0].record_count, 42);
}

#[tokio::test]
async fn http_client_reads_records_by_connector_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIngestionStatsClient::new(reqwest::Client::new(), url);

    let records = client.records_by_connector(Uuid::new_v4(), "zendesk").await.unwrap();

    assert_eq!(records.len(), 1);
    assert!(!records[0].is_normalized());
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpIngestionStatsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.connector_stats(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, IngestionStatsClientError::Unreachable(_)));
}
