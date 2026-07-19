use super::*;
use axum::extract::Json as JsonExtractor;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySavedSearchQueriesClient {
    pub queries: Mutex<Vec<SavedSearchQuery>>,
}

#[async_trait]
impl SavedSearchQueriesClient for InMemorySavedSearchQueriesClient {
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueriesClientError> {
        Ok(self
            .queries
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        name: &str,
        filter: serde_json::Value,
    ) -> Result<SavedSearchQuery, SavedSearchQueriesClientError> {
        let query = SavedSearchQuery::new(tenant_id, name, filter);
        self.queries.lock().unwrap().push(query.clone());
        Ok(query)
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), SavedSearchQueriesClientError> {
        self.queries.lock().unwrap().retain(|q| !(q.id == id && q.tenant_id == tenant_id));
        Ok(())
    }
}

pub struct FailingSavedSearchQueriesClient;

#[async_trait]
impl SavedSearchQueriesClient for FailingSavedSearchQueriesClient {
    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Unreachable("simulated failure".to_string()))
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _name: &str,
        _filter: serde_json::Value,
    ) -> Result<SavedSearchQuery, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<(), SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn list_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "name": "urgent tickets",
            "filter": {"q": "urgent"}
        }]))
        .into_response()
    }
    async fn create_handler(
        JsonExtractor(query): JsonExtractor<SavedSearchQuery>,
    ) -> axum::response::Response {
        (axum::http::StatusCode::CREATED, Json(query)).into_response()
    }
    async fn delete_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::NO_CONTENT
    }
    let app = Router::new()
        .route("/v1/saved-search-queries", get(list_handler).post(create_handler))
        .route("/v1/saved-search-queries/:id", axum::routing::delete(delete_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_saved_queries_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSavedSearchQueriesClient::new(reqwest::Client::new(), url);

    let queries = client.list(Uuid::new_v4()).await.unwrap();

    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].name, "urgent tickets");
}

#[tokio::test]
async fn http_client_creates_a_saved_query_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSavedSearchQueriesClient::new(reqwest::Client::new(), url);
    let tenant_id = Uuid::new_v4();

    let query = client.create(tenant_id, "my search", serde_json::json!({"q": "x"})).await.unwrap();

    assert_eq!(query.tenant_id, tenant_id);
    assert_eq!(query.name, "my search");
}

#[tokio::test]
async fn http_client_deletes_a_saved_query_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSavedSearchQueriesClient::new(reqwest::Client::new(), url);

    client.delete(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpSavedSearchQueriesClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, SavedSearchQueriesClientError::Unreachable(_)));
}
