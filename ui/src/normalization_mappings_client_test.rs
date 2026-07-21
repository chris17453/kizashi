use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use common::NormalizationMapping;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryNormalizationMappingsClient {
    pub mappings: Mutex<Vec<NormalizationMapping>>,
    pub created: Mutex<Vec<NormalizationMapping>>,
    pub deleted: Mutex<Vec<Uuid>>,
}

#[async_trait]
impl NormalizationMappingsClient for InMemoryNormalizationMappingsClient {
    async fn list_mappings(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingsClientError> {
        Ok(self.mappings.lock().unwrap().clone())
    }

    async fn create_mapping(
        &self,
        role: Role,
        _actor: &str,
        mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingsClientError> {
        if !role.at_least(Role::Operator) {
            return Err(NormalizationMappingsClientError::Rejected(403));
        }
        self.created.lock().unwrap().push(mapping.clone());
        Ok(mapping)
    }

    async fn delete_mapping(
        &self,
        role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), NormalizationMappingsClientError> {
        if !role.at_least(Role::Operator) {
            return Err(NormalizationMappingsClientError::Rejected(403));
        }
        self.deleted.lock().unwrap().push(id);
        Ok(())
    }
}

pub struct FailingNormalizationMappingsClient;

#[async_trait]
impl NormalizationMappingsClient for FailingNormalizationMappingsClient {
    async fn list_mappings(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingsClientError> {
        Err(NormalizationMappingsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn create_mapping(
        &self,
        _role: Role,
        _actor: &str,
        _mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingsClientError> {
        Err(NormalizationMappingsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete_mapping(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<(), NormalizationMappingsClientError> {
        Err(NormalizationMappingsClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn list_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        let mut field_map = BTreeMap::new();
        field_map.insert("text".to_string(), "$.description".to_string());
        Json(vec![NormalizationMapping {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            source_type: "ticket".to_string(),
            field_map,
            version: 1,
        }])
        .into_response()
    }
    async fn create_handler(
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        if headers.get("x-username").and_then(|v| v.to_str().ok()) != Some("alice") {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        (axum::http::StatusCode::CREATED, Json(body)).into_response()
    }
    async fn delete_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        axum::http::StatusCode::NO_CONTENT.into_response()
    }
    let app = Router::new()
        .route("/v1/normalization-mappings", get(list_handler).post(create_handler))
        .route("/v1/normalization-mappings/:id", axum::routing::delete(delete_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn sample_mapping() -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        source_type: "ticket".to_string(),
        field_map,
        version: 1,
    }
}

#[tokio::test]
async fn http_client_lists_mappings_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpNormalizationMappingsClient::new(reqwest::Client::new(), url);

    let mappings = client.list_mappings(Uuid::new_v4()).await.unwrap();
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].source_type, "ticket");
}

#[tokio::test]
async fn http_client_creates_a_mapping_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpNormalizationMappingsClient::new(reqwest::Client::new(), url);

    let created = client.create_mapping(Role::Operator, "alice", sample_mapping()).await.unwrap();
    assert_eq!(created.source_type, "ticket");
}

#[tokio::test]
async fn http_client_create_is_rejected_for_insufficient_role() {
    let url = spawn_stub_server().await;
    let client = HttpNormalizationMappingsClient::new(reqwest::Client::new(), url);

    let err = client.create_mapping(Role::Viewer, "alice", sample_mapping()).await.unwrap_err();
    assert!(matches!(err, NormalizationMappingsClientError::Rejected(403)));
}

#[tokio::test]
async fn http_client_create_is_rejected_when_actor_header_missing_expected_value() {
    let url = spawn_stub_server().await;
    let client = HttpNormalizationMappingsClient::new(reqwest::Client::new(), url);

    let err =
        client.create_mapping(Role::Operator, "someone-else", sample_mapping()).await.unwrap_err();
    assert!(matches!(err, NormalizationMappingsClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpNormalizationMappingsClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1".to_string(),
    );
    let err = client.list_mappings(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, NormalizationMappingsClientError::Unreachable(_)));
}

#[tokio::test]
async fn http_client_deletes_a_mapping_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpNormalizationMappingsClient::new(reqwest::Client::new(), url);

    client.delete_mapping(Role::Operator, "alice", Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn http_client_delete_is_rejected_for_insufficient_role() {
    let url = spawn_stub_server().await;
    let client = HttpNormalizationMappingsClient::new(reqwest::Client::new(), url);

    let err = client
        .delete_mapping(Role::Viewer, "alice", Uuid::new_v4(), Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(matches!(err, NormalizationMappingsClientError::Rejected(403)));
}
