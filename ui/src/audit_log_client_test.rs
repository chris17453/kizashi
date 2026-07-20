use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAuditLogClient {
    pub entries: Mutex<HashMap<Uuid, Vec<AuditLogEntry>>>,
    pub recent: Mutex<Vec<AuditLogEntry>>,
}

#[async_trait]
impl AuditLogClient for InMemoryAuditLogClient {
    async fn list_for_entity(
        &self,
        _tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        Ok(self.entries.lock().unwrap().get(&entity_id).cloned().unwrap_or_default())
    }

    async fn list_recent(
        &self,
        _tenant_id: Uuid,
        limit: u32,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        let recent = self.recent.lock().unwrap();
        Ok(recent
            .iter()
            .filter(|e| before.is_none_or(|b| e.changed_at < b))
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

pub struct FailingAuditLogClient;

#[async_trait]
impl AuditLogClient for FailingAuditLogClient {
    async fn list_for_entity(
        &self,
        _tenant_id: Uuid,
        _entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        Err(AuditLogClientError::Unreachable("simulated failure".to_string()))
    }

    async fn list_recent(
        &self,
        _tenant_id: Uuid,
        _limit: u32,
        _before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        Err(AuditLogClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "entity_type": "trigger_definition",
            "entity_id": "22222222-2222-2222-2222-222222222222",
            "change_type": "created",
            "actor": "33333333-3333-3333-3333-333333333333",
            "before": null,
            "after": {"name": "high-volume"},
            "changed_at": "2026-07-19T00:00:00Z"
        }]))
        .into_response()
    }
    let app = Router::new().route("/v1/audit-log/:entity_id", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_entries_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAuditLogClient::new(reqwest::Client::new(), url);

    let entries = client.list_for_entity(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change_type, "created");
    assert_eq!(entries[0].entity_type, "trigger_definition");
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpAuditLogClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_for_entity(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, AuditLogClientError::Unreachable(_)));
}

async fn spawn_stub_recent_server() -> String {
    async fn handler(
        headers: HeaderMap,
        axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "entity_type": "trigger_definition",
            "entity_id": "22222222-2222-2222-2222-222222222222",
            "change_type": "created",
            "actor": "alice",
            "before": null,
            "after": {"name": "high-volume", "limit_param": params.get("limit")},
            "changed_at": "2026-07-19T00:00:00Z"
        }]))
        .into_response()
    }
    let app = Router::new().route("/v1/audit-log", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_recent_entries_against_a_real_server() {
    let url = spawn_stub_recent_server().await;
    let client = HttpAuditLogClient::new(reqwest::Client::new(), url);

    let entries = client.list_recent(Uuid::new_v4(), 50, None).await.unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].actor, "alice");
}

#[tokio::test]
async fn http_client_list_recent_sends_the_limit_and_before_query_params() {
    let url = spawn_stub_recent_server().await;
    let client = HttpAuditLogClient::new(reqwest::Client::new(), url);

    let entries = client
        .list_recent(Uuid::new_v4(), 25, Some("2026-07-18T00:00:00Z".parse().unwrap()))
        .await
        .unwrap();

    assert_eq!(entries[0].after["limit_param"], "25");
}

#[tokio::test]
async fn http_client_list_recent_returns_unreachable_when_server_is_down() {
    let client = HttpAuditLogClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_recent(Uuid::new_v4(), 50, None).await.unwrap_err();
    assert!(matches!(err, AuditLogClientError::Unreachable(_)));
}
