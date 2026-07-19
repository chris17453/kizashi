use super::*;
use axum::extract::Json as JsonExtractor;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryRetentionPoliciesClient {
    pub policies: Mutex<Vec<RetentionPolicy>>,
}

#[async_trait]
impl RetentionPoliciesClient for InMemoryRetentionPoliciesClient {
    async fn list_policies(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPoliciesClientError> {
        Ok(self.policies.lock().unwrap().clone())
    }

    async fn create_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        if !role.at_least(Role::Operator) {
            return Err(RetentionPoliciesClientError::Rejected(403));
        }
        self.policies.lock().unwrap().push(policy.clone());
        Ok(policy)
    }

    async fn update_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        if !role.at_least(Role::Operator) {
            return Err(RetentionPoliciesClientError::Rejected(403));
        }
        let mut policies = self.policies.lock().unwrap();
        match policies.iter_mut().find(|p| p.id == policy.id) {
            Some(existing) => {
                *existing = policy.clone();
                Ok(policy)
            }
            None => Err(RetentionPoliciesClientError::Rejected(404)),
        }
    }

    async fn delete_policy(
        &self,
        role: Role,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), RetentionPoliciesClientError> {
        if !role.at_least(Role::Operator) {
            return Err(RetentionPoliciesClientError::Rejected(403));
        }
        let mut policies = self.policies.lock().unwrap();
        let before_len = policies.len();
        policies.retain(|p| p.id != id);
        if policies.len() == before_len {
            return Err(RetentionPoliciesClientError::Rejected(404));
        }
        Ok(())
    }
}

pub struct FailingRetentionPoliciesClient;

#[async_trait]
impl RetentionPoliciesClient for FailingRetentionPoliciesClient {
    async fn list_policies(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Unreachable("simulated failure".to_string()))
    }

    async fn create_policy(
        &self,
        _role: Role,
        _policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Unreachable("simulated failure".to_string()))
    }

    async fn update_policy(
        &self,
        _role: Role,
        _policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete_policy(
        &self,
        _role: Role,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<(), RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Unreachable("simulated failure".to_string()))
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
            "data_class": "raw",
            "ttl_days": 90,
            "enabled": true
        }]))
        .into_response()
    }
    async fn create_handler(
        JsonExtractor(policy): JsonExtractor<RetentionPolicy>,
    ) -> axum::response::Response {
        (axum::http::StatusCode::CREATED, Json(policy)).into_response()
    }
    async fn update_handler(
        JsonExtractor(policy): JsonExtractor<RetentionPolicy>,
    ) -> axum::response::Response {
        Json(policy).into_response()
    }
    async fn delete_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::NO_CONTENT
    }
    let app = Router::new()
        .route("/v1/retention-policies", get(list_handler).post(create_handler))
        .route(
            "/v1/retention-policies/:id",
            axum::routing::put(update_handler).delete(delete_handler),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_policies_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpRetentionPoliciesClient::new(reqwest::Client::new(), url);

    let policies = client.list_policies(Uuid::new_v4()).await.unwrap();

    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].data_class, DataClass::Raw);
    assert_eq!(policies[0].ttl_days, 90);
}

#[tokio::test]
async fn http_client_creates_a_policy_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpRetentionPoliciesClient::new(reqwest::Client::new(), url);
    let tenant_id = Uuid::new_v4();
    let policy = RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days: 30,
        enabled: true,
    };

    let created = client.create_policy(Role::Operator, policy.clone()).await.unwrap();

    assert_eq!(created.tenant_id, tenant_id);
    assert_eq!(created.ttl_days, 30);
}

#[tokio::test]
async fn http_client_updates_a_policy_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpRetentionPoliciesClient::new(reqwest::Client::new(), url);
    let mut policy = RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        data_class: DataClass::Raw,
        ttl_days: 30,
        enabled: true,
    };
    policy.enabled = false;

    let updated = client.update_policy(Role::Operator, policy).await.unwrap();
    assert!(!updated.enabled);
}

#[tokio::test]
async fn http_client_deletes_a_policy_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpRetentionPoliciesClient::new(reqwest::Client::new(), url);

    client.delete_policy(Role::Operator, Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpRetentionPoliciesClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_policies(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, RetentionPoliciesClientError::Unreachable(_)));
}
