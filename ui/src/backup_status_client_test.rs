use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryBackupStatusClient {
    pub runs: Mutex<Vec<BackupRun>>,
}

#[async_trait]
impl BackupStatusClient for InMemoryBackupStatusClient {
    async fn list_recent(
        &self,
        _role: common::Role,
    ) -> Result<Vec<BackupRun>, BackupStatusClientError> {
        Ok(self.runs.lock().unwrap().clone())
    }
}

pub struct FailingBackupStatusClient;

#[async_trait]
impl BackupStatusClient for FailingBackupStatusClient {
    async fn list_recent(
        &self,
        _role: common::Role,
    ) -> Result<Vec<BackupRun>, BackupStatusClientError> {
        Err(BackupStatusClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-role").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "started_at": "2026-07-20T00:00:00Z",
            "completed_at": "2026-07-20T00:01:00Z",
            "status": "success",
            "target": "postgres/2026-07-20.dump",
            "size_bytes": 4096,
            "error": null
        }]))
        .into_response()
    }
    let app = Router::new().route("/v1/backup/status", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_recent_runs_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpBackupStatusClient::new(reqwest::Client::new(), url);

    let runs = client.list_recent(common::Role::Admin).await.unwrap();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "success");
    assert_eq!(runs[0].size_bytes, Some(4096));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpBackupStatusClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_recent(common::Role::Admin).await.unwrap_err();
    assert!(matches!(err, BackupStatusClientError::Unreachable(_)));
}
