use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
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
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<BackupRun>, BackupStatusClientError> {
        let mut runs = self.runs.lock().unwrap().clone();
        if let Some(before) = before {
            runs.retain(|r| r.started_at < before);
        }
        Ok(runs)
    }
}

pub struct FailingBackupStatusClient;

#[async_trait]
impl BackupStatusClient for FailingBackupStatusClient {
    async fn list_recent(
        &self,
        _role: common::Role,
        _before: Option<DateTime<Utc>>,
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
    async fn trigger_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "status": "success",
            "size_bytes": 4096,
            "error": null
        }))
        .into_response()
    }
    let app = Router::new()
        .route("/v1/backup/status", get(handler))
        .route("/v1/backup/run", post(trigger_handler));
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

    let runs = client.list_recent(common::Role::Admin, None).await.unwrap();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "success");
    assert_eq!(runs[0].size_bytes, Some(4096));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpBackupStatusClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_recent(common::Role::Admin, None).await.unwrap_err();
    assert!(matches!(err, BackupStatusClientError::Unreachable(_)));
}

#[tokio::test]
async fn http_client_triggers_a_backup_and_decodes_the_outcome() {
    let url = spawn_stub_server().await;
    let client = HttpBackupStatusClient::new(reqwest::Client::new(), url);

    let result = client.trigger_backup().await.unwrap();

    assert_eq!(result.status, "success");
    assert_eq!(result.size_bytes, Some(4096));
}

#[tokio::test]
async fn http_client_sends_the_before_cursor_as_a_query_param() {
    async fn handler(
        axum::extract::Query(params): axum::extract::Query<
            std::collections::HashMap<String, String>,
        >,
    ) -> axum::response::Response {
        assert!(params.contains_key("before"));
        Json(Vec::<serde_json::Value>::new()).into_response()
    }
    let app = Router::new().route("/v1/backup/status", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = HttpBackupStatusClient::new(reqwest::Client::new(), format!("http://{addr}"));

    let runs = client.list_recent(common::Role::Admin, Some(Utc::now())).await.unwrap();

    assert!(runs.is_empty());
}
