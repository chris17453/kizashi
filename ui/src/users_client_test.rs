use super::*;
use axum::extract::Json as JsonExtractor;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryUsersClient {
    pub users: Mutex<Vec<UiUser>>,
}

#[async_trait]
impl UsersClient for InMemoryUsersClient {
    async fn list_users(
        &self,
        tenant_id: Uuid,
        _role: Role,
    ) -> Result<Vec<UiUser>, UsersClientError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn create_user(
        &self,
        tenant_id: Uuid,
        _role: Role,
        username: &str,
        _password: &str,
        new_user_role: Role,
        _actor: &str,
    ) -> Result<UiUser, UsersClientError> {
        let user = UiUser {
            id: Uuid::new_v4(),
            tenant_id,
            username: username.to_string(),
            role: new_user_role,
        };
        self.users.lock().unwrap().push(user.clone());
        Ok(user)
    }

    async fn update_user_role(
        &self,
        tenant_id: Uuid,
        _role: Role,
        id: Uuid,
        new_role: Role,
        _actor: &str,
    ) -> Result<UiUser, UsersClientError> {
        let mut users = self.users.lock().unwrap();
        let user = users.iter_mut().find(|u| u.id == id && u.tenant_id == tenant_id).ok_or(
            UsersClientError::Rejected { status: 404, message: "no such user".to_string() },
        )?;
        user.role = new_role;
        Ok(user.clone())
    }

    async fn delete_user(
        &self,
        tenant_id: Uuid,
        _role: Role,
        id: Uuid,
        _actor: &str,
    ) -> Result<(), UsersClientError> {
        self.users.lock().unwrap().retain(|u| !(u.id == id && u.tenant_id == tenant_id));
        Ok(())
    }

    async fn export_user_data(
        &self,
        tenant_id: Uuid,
        _role: Role,
        id: Uuid,
    ) -> Result<Vec<u8>, UsersClientError> {
        let users = self.users.lock().unwrap();
        let user = users.iter().find(|u| u.id == id && u.tenant_id == tenant_id).ok_or(
            UsersClientError::Rejected { status: 404, message: "no such user".to_string() },
        )?;
        Ok(serde_json::json!({
            "user": {"id": user.id, "username": user.username, "role": user.role},
            "audit_log": [],
            "login_attempts": []
        })
        .to_string()
        .into_bytes())
    }
}

pub struct FailingUsersClient;

#[async_trait]
impl UsersClient for FailingUsersClient {
    async fn list_users(
        &self,
        _tenant_id: Uuid,
        _role: Role,
    ) -> Result<Vec<UiUser>, UsersClientError> {
        Err(UsersClientError::Unreachable("simulated failure".to_string()))
    }

    async fn create_user(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _username: &str,
        _password: &str,
        _new_user_role: Role,
        _actor: &str,
    ) -> Result<UiUser, UsersClientError> {
        Err(UsersClientError::Unreachable("simulated failure".to_string()))
    }

    async fn update_user_role(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _id: Uuid,
        _new_role: Role,
        _actor: &str,
    ) -> Result<UiUser, UsersClientError> {
        Err(UsersClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete_user(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _id: Uuid,
        _actor: &str,
    ) -> Result<(), UsersClientError> {
        Err(UsersClientError::Unreachable("simulated failure".to_string()))
    }

    async fn export_user_data(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _id: Uuid,
    ) -> Result<Vec<u8>, UsersClientError> {
        Err(UsersClientError::Unreachable("simulated failure".to_string()))
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
            "username": "alice",
            "role": "operator"
        }]))
        .into_response()
    }
    async fn create_handler(
        headers: HeaderMap,
        JsonExtractor(body): JsonExtractor<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-username").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!({
                "id": "11111111-1111-1111-1111-111111111111",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "username": body["username"],
                "role": body["role"]
            })),
        )
            .into_response()
    }
    async fn update_handler(
        JsonExtractor(body): JsonExtractor<serde_json::Value>,
    ) -> axum::response::Response {
        Json(serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "username": "alice",
            "role": body["role"]
        }))
        .into_response()
    }
    async fn delete_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::NO_CONTENT
    }
    async fn export_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "user": {"id": "11111111-1111-1111-1111-111111111111", "username": "alice"},
            "audit_log": [],
            "login_attempts": []
        }))
        .into_response()
    }
    let app = Router::new()
        .route("/v1/users", get(list_handler).post(create_handler))
        .route("/v1/users/:id", axum::routing::put(update_handler).delete(delete_handler))
        .route("/v1/users/:id/data-subject-export", get(export_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_users_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpUsersClient::new(reqwest::Client::new(), url);

    let users = client.list_users(Uuid::new_v4(), Role::Admin).await.unwrap();

    assert_eq!(users.len(), 1);
    assert_eq!(users[0].username, "alice");
}

#[tokio::test]
async fn http_client_creates_a_user_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpUsersClient::new(reqwest::Client::new(), url);

    let user = client
        .create_user(Uuid::new_v4(), Role::Admin, "bob", "a-real-password", Role::Operator, "alice")
        .await
        .unwrap();

    assert_eq!(user.username, "bob");
    assert_eq!(user.role, Role::Operator);
}

#[tokio::test]
async fn http_client_create_user_surfaces_the_backends_error_message() {
    async fn handler() -> axum::response::Response {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "password must be at least 12 characters"})),
        )
            .into_response()
    }
    let app = Router::new().route("/v1/users", axum::routing::post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = HttpUsersClient::new(reqwest::Client::new(), format!("http://{addr}"));

    let err = client
        .create_user(Uuid::new_v4(), Role::Admin, "bob", "short", Role::Operator, "alice")
        .await
        .unwrap_err();

    match err {
        UsersClientError::Rejected { status, message } => {
            assert_eq!(status, 400);
            assert_eq!(message, "password must be at least 12 characters");
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

#[tokio::test]
async fn http_client_updates_a_users_role_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpUsersClient::new(reqwest::Client::new(), url);

    let user = client
        .update_user_role(Uuid::new_v4(), Role::Admin, Uuid::new_v4(), Role::Admin, "alice")
        .await
        .unwrap();

    assert_eq!(user.role, Role::Admin);
}

#[tokio::test]
async fn http_client_deletes_a_user_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpUsersClient::new(reqwest::Client::new(), url);

    client.delete_user(Uuid::new_v4(), Role::Admin, Uuid::new_v4(), "alice").await.unwrap();
}

#[tokio::test]
async fn http_client_exports_a_users_data_subject_record_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpUsersClient::new(reqwest::Client::new(), url);

    let bytes = client.export_user_data(Uuid::new_v4(), Role::Admin, Uuid::new_v4()).await.unwrap();

    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["user"]["username"], "alice");
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpUsersClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_users(Uuid::new_v4(), Role::Admin).await.unwrap_err();
    assert!(matches!(err, UsersClientError::Unreachable(_)));
}

#[tokio::test]
async fn http_client_sends_x_username_header_on_create_user() {
    use std::sync::Arc;

    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_for_handler = captured.clone();

    async fn create_handler(
        axum::extract::State(captured): axum::extract::State<Arc<Mutex<Option<String>>>>,
        headers: HeaderMap,
        JsonExtractor(body): JsonExtractor<serde_json::Value>,
    ) -> axum::response::Response {
        *captured.lock().unwrap() =
            headers.get("x-username").and_then(|v| v.to_str().ok()).map(str::to_string);
        (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!({
                "id": "11111111-1111-1111-1111-111111111111",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "username": body["username"],
                "role": body["role"]
            })),
        )
            .into_response()
    }

    let app = Router::new()
        .route("/v1/users", axum::routing::post(create_handler))
        .with_state(captured_for_handler);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let url = format!("http://{addr}");

    let client = HttpUsersClient::new(reqwest::Client::new(), url);
    client
        .create_user(Uuid::new_v4(), Role::Admin, "bob", "a-real-password", Role::Operator, "carol")
        .await
        .unwrap();

    assert_eq!(captured.lock().unwrap().as_deref(), Some("carol"));
}
