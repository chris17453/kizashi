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
    pub has_more: Mutex<bool>,
    pub reprocessed: Mutex<usize>,
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

    async fn search_records(
        &self,
        _tenant_id: Uuid,
        _filter: &RecordSearchFilter,
    ) -> Result<SearchResult, IngestionStatsClientError> {
        Ok(SearchResult {
            records: self.records.lock().unwrap().clone(),
            has_more: *self.has_more.lock().unwrap(),
        })
    }

    async fn get_record(
        &self,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<RecordSummary>, IngestionStatsClientError> {
        Ok(self.records.lock().unwrap().iter().find(|r| r.id == id).cloned())
    }

    async fn reprocess(&self, _tenant_id: Uuid) -> Result<usize, IngestionStatsClientError> {
        Ok(*self.reprocessed.lock().unwrap())
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

    async fn search_records(
        &self,
        _tenant_id: Uuid,
        _filter: &RecordSearchFilter,
    ) -> Result<SearchResult, IngestionStatsClientError> {
        Err(IngestionStatsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn get_record(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<RecordSummary>, IngestionStatsClientError> {
        Err(IngestionStatsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn reprocess(&self, _tenant_id: Uuid) -> Result<usize, IngestionStatsClientError> {
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
    async fn search_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "records": [{
                "id": "11111111-1111-1111-1111-111111111111",
                "connector_id": "zendesk",
                "source_type": "ticket",
                "ingested_at": "2026-07-18T12:00:00Z",
                "raw_payload": {"subject": "printer on fire"},
                "normalized_payload": null
            }],
            "has_more": false
        }))
        .into_response()
    }
    async fn get_record_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "connector_id": "zendesk",
            "source_type": "ticket",
            "ingested_at": "2026-07-18T12:00:00Z",
            "raw_payload": {"subject": "printer on fire"},
            "normalized_payload": null
        }))
        .into_response()
    }
    async fn reprocess_handler() -> axum::response::Response {
        Json(serde_json::json!({"republished": 7})).into_response()
    }
    let app = Router::new()
        .route("/v1/records/stats", get(stats_handler))
        .route("/v1/records/by-connector", get(by_connector_handler))
        .route("/v1/records/search", get(search_handler))
        .route("/v1/records/reprocess", axum::routing::post(reprocess_handler))
        .route("/v1/records/:id", get(get_record_handler));
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
async fn http_client_searches_records_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIngestionStatsClient::new(reqwest::Client::new(), url);

    let filter = RecordSearchFilter { query: Some("printer".to_string()), ..Default::default() };
    let result = client.search_records(Uuid::new_v4(), &filter).await.unwrap();

    assert_eq!(result.records.len(), 1);
    assert_eq!(result.records[0].connector_id, "zendesk");
    assert!(!result.has_more);
}

#[tokio::test]
async fn http_client_sends_from_to_and_normalized_as_query_params() {
    async fn handler(Query(params): Query<HashMap<String, String>>) -> axum::response::Response {
        assert_eq!(params.get("from").map(String::as_str), Some("2026-07-01T00:00:00+00:00"));
        assert_eq!(params.get("to").map(String::as_str), Some("2026-07-20T00:00:00+00:00"));
        assert_eq!(params.get("normalized").map(String::as_str), Some("false"));
        Json(serde_json::json!({"records": [], "has_more": false})).into_response()
    }
    let app = Router::new().route("/v1/records/search", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = HttpIngestionStatsClient::new(reqwest::Client::new(), format!("http://{addr}"));

    let filter = RecordSearchFilter {
        from: Some("2026-07-01T00:00:00Z".parse().unwrap()),
        to: Some("2026-07-20T00:00:00Z".parse().unwrap()),
        normalized: Some(false),
        ..Default::default()
    };
    let result = client.search_records(Uuid::new_v4(), &filter).await.unwrap();

    assert!(result.records.is_empty());
}

#[tokio::test]
async fn http_client_gets_a_record_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIngestionStatsClient::new(reqwest::Client::new(), url);

    let record = client.get_record(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    assert!(record.is_some());
    assert_eq!(record.unwrap().connector_id, "zendesk");
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpIngestionStatsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.connector_stats(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, IngestionStatsClientError::Unreachable(_)));
}

#[tokio::test]
async fn http_client_reprocesses_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIngestionStatsClient::new(reqwest::Client::new(), url);

    let republished = client.reprocess(Uuid::new_v4()).await.unwrap();

    assert_eq!(republished, 7);
}
