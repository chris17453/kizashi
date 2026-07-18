use super::*;
use axum::extract::{Path, Query};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::{delete, get};
use axum::Router;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryRawRecordClient {
    pub records: Mutex<Vec<RawRecord>>,
    pub deleted: Mutex<Vec<Uuid>>,
    pub reimported: Mutex<Vec<RawRecord>>,
}

#[async_trait]
impl RawRecordClient for InMemoryRawRecordClient {
    async fn list_older_than(
        &self,
        tenant_id: Uuid,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RawRecordClientError> {
        let mut found: Vec<RawRecord> = self
            .records
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.tenant_id == tenant_id && r.ingested_at < cutoff)
            .cloned()
            .collect();
        found.sort_by_key(|r| r.ingested_at);
        found.truncate(limit as usize);
        Ok(found)
    }

    async fn delete(&self, tenant_id: Uuid, record_id: Uuid) -> Result<(), RawRecordClientError> {
        self.records.lock().unwrap().retain(|r| !(r.id == record_id && r.tenant_id == tenant_id));
        self.deleted.lock().unwrap().push(record_id);
        Ok(())
    }

    async fn reimport(&self, record: &RawRecord) -> Result<(), RawRecordClientError> {
        self.reimported.lock().unwrap().push(record.clone());
        Ok(())
    }
}

pub struct FailingRawRecordClient;

#[async_trait]
impl RawRecordClient for FailingRawRecordClient {
    async fn list_older_than(
        &self,
        _tenant_id: Uuid,
        _cutoff: DateTime<Utc>,
        _limit: i64,
    ) -> Result<Vec<RawRecord>, RawRecordClientError> {
        Err(RawRecordClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete(&self, _tenant_id: Uuid, _record_id: Uuid) -> Result<(), RawRecordClientError> {
        Err(RawRecordClientError::Unreachable("simulated failure".to_string()))
    }

    async fn reimport(&self, _record: &RawRecord) -> Result<(), RawRecordClientError> {
        Err(RawRecordClientError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_record(tenant_id: Uuid) -> RawRecord {
    RawRecord::new("zendesk", common::SourceType::Ticket, tenant_id, serde_json::json!({"a": 1}))
}

#[tokio::test]
async fn in_memory_client_lists_deletes_and_reimports() {
    let client = InMemoryRawRecordClient::default();
    let tenant_id = Uuid::new_v4();
    let record = sample_record(tenant_id);
    client.records.lock().unwrap().push(record.clone());

    let found = client.list_older_than(tenant_id, Utc::now(), 10).await.unwrap();
    assert_eq!(found, vec![record.clone()]);

    client.delete(tenant_id, record.id).await.unwrap();
    assert!(client.records.lock().unwrap().is_empty());
    assert_eq!(*client.deleted.lock().unwrap(), vec![record.id]);

    client.reimport(&record).await.unwrap();
    assert_eq!(*client.reimported.lock().unwrap(), vec![record]);
}

#[tokio::test]
async fn list_older_than_is_scoped_to_tenant() {
    let client = InMemoryRawRecordClient::default();
    let tenant_id = Uuid::new_v4();
    client.records.lock().unwrap().push(sample_record(tenant_id));
    client.records.lock().unwrap().push(sample_record(Uuid::new_v4()));

    let found = client.list_older_than(tenant_id, Utc::now(), 10).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].tenant_id, tenant_id);
}

#[tokio::test]
async fn failing_client_returns_unreachable_error() {
    let client = FailingRawRecordClient;
    assert!(client.list_older_than(Uuid::new_v4(), Utc::now(), 10).await.is_err());
    assert!(client.delete(Uuid::new_v4(), Uuid::new_v4()).await.is_err());
    assert!(client.reimport(&sample_record(Uuid::new_v4())).await.is_err());
}

async fn spawn_stub_server() -> (String, std::sync::Arc<Mutex<Vec<RawRecord>>>) {
    let tenant_id = Uuid::new_v4();
    let store = std::sync::Arc::new(Mutex::new(vec![sample_record(tenant_id)]));

    async fn list_handler(
        axum::extract::State(store): axum::extract::State<std::sync::Arc<Mutex<Vec<RawRecord>>>>,
        headers: HeaderMap,
        Query(_params): Query<HashMap<String, String>>,
    ) -> Json<Vec<RawRecord>> {
        let tenant_id: Uuid =
            headers.get("x-tenant-id").and_then(|v| v.to_str().ok()).unwrap().parse().unwrap();
        Json(store.lock().unwrap().iter().filter(|r| r.tenant_id == tenant_id).cloned().collect())
    }
    async fn delete_handler(
        axum::extract::State(store): axum::extract::State<std::sync::Arc<Mutex<Vec<RawRecord>>>>,
        Path(id): Path<Uuid>,
    ) -> axum::http::StatusCode {
        store.lock().unwrap().retain(|r| r.id != id);
        axum::http::StatusCode::NO_CONTENT
    }
    async fn post_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::CREATED
    }

    let app = Router::new()
        .route("/v1/records", get(list_handler).post(post_handler))
        .route("/v1/records/:id", delete(delete_handler))
        .with_state(store.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), store)
}

async fn spawn_erroring_server() -> String {
    async fn error_handler() -> axum::response::Response {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
    let app = Router::new()
        .route("/v1/records", get(error_handler).post(error_handler))
        .route("/v1/records/:id", delete(error_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_deletes_and_reimports_against_a_real_server() {
    let (url, store) = spawn_stub_server().await;
    let client = HttpRawRecordClient::new(reqwest::Client::new(), url);
    let tenant_id = store.lock().unwrap()[0].tenant_id;

    let found = client.list_older_than(tenant_id, Utc::now(), 10).await.unwrap();
    assert_eq!(found.len(), 1);
    let record_id = found[0].id;

    client.delete(tenant_id, record_id).await.unwrap();
    assert!(store.lock().unwrap().is_empty());

    client.reimport(&found[0]).await.unwrap();
}

#[tokio::test]
async fn http_client_returns_rejected_when_server_errors() {
    let url = spawn_erroring_server().await;
    let client = HttpRawRecordClient::new(reqwest::Client::new(), url);

    assert!(matches!(
        client.list_older_than(Uuid::new_v4(), Utc::now(), 10).await.unwrap_err(),
        RawRecordClientError::Rejected(500)
    ));
    assert!(matches!(
        client.delete(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err(),
        RawRecordClientError::Rejected(500)
    ));
    assert!(matches!(
        client.reimport(&sample_record(Uuid::new_v4())).await.unwrap_err(),
        RawRecordClientError::Rejected(500)
    ));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpRawRecordClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    assert!(matches!(
        client.list_older_than(Uuid::new_v4(), Utc::now(), 10).await.unwrap_err(),
        RawRecordClientError::Unreachable(_)
    ));
}
