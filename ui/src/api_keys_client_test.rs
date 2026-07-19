use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::{delete, get};
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryApiKeysClient {
    pub keys: Mutex<Vec<ApiKeySummary>>,
}

#[async_trait]
impl ApiKeysClient for InMemoryApiKeysClient {
    async fn list_api_keys(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ApiKeySummary>, ApiKeysClientError> {
        Ok(self.keys.lock().unwrap().clone())
    }

    async fn create_api_key(
        &self,
        _tenant_id: Uuid,
        label: &str,
    ) -> Result<String, ApiKeysClientError> {
        self.keys.lock().unwrap().push(ApiKeySummary {
            id: Uuid::new_v4(),
            label: label.to_string(),
            created_at: Utc::now(),
            revoked_at: None,
        });
        Ok(format!("kzsh_{}", Uuid::new_v4().simple()))
    }

    async fn revoke_api_key(&self, _tenant_id: Uuid, id: Uuid) -> Result<(), ApiKeysClientError> {
        if let Some(key) = self.keys.lock().unwrap().iter_mut().find(|k| k.id == id) {
            key.revoked_at = Some(Utc::now());
        }
        Ok(())
    }
}

pub struct FailingApiKeysClient;

#[async_trait]
impl ApiKeysClient for FailingApiKeysClient {
    async fn list_api_keys(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ApiKeySummary>, ApiKeysClientError> {
        Err(ApiKeysClientError::Unreachable("simulated failure".to_string()))
    }

    async fn create_api_key(
        &self,
        _tenant_id: Uuid,
        _label: &str,
    ) -> Result<String, ApiKeysClientError> {
        Err(ApiKeysClientError::Unreachable("simulated failure".to_string()))
    }

    async fn revoke_api_key(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), ApiKeysClientError> {
        Err(ApiKeysClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn list_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "label": "ci-agent",
            "created_at": "2026-01-01T00:00:00Z",
            "revoked_at": null
        }]))
        .into_response()
    }
    async fn create_handler() -> axum::response::Response {
        (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!({
                "id": "11111111-1111-1111-1111-111111111111",
                "label": "ci-agent",
                "api_key": "kzsh_test-plaintext-key"
            })),
        )
            .into_response()
    }
    async fn revoke_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::NO_CONTENT
    }
    let app = Router::new()
        .route("/v1/api-keys", get(list_handler).post(create_handler))
        .route("/v1/api-keys/:id", delete(revoke_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_api_keys_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpApiKeysClient::new(reqwest::Client::new(), url);

    let keys = client.list_api_keys(Uuid::new_v4()).await.unwrap();

    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].label, "ci-agent");
}

#[tokio::test]
async fn http_client_creates_an_api_key_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpApiKeysClient::new(reqwest::Client::new(), url);

    let plaintext = client.create_api_key(Uuid::new_v4(), "ci-agent").await.unwrap();

    assert_eq!(plaintext, "kzsh_test-plaintext-key");
}

#[tokio::test]
async fn http_client_revokes_an_api_key_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpApiKeysClient::new(reqwest::Client::new(), url);

    client.revoke_api_key(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpApiKeysClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_api_keys(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, ApiKeysClientError::Unreachable(_)));
}
