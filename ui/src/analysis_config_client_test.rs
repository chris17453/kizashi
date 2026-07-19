use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAnalysisConfigClient {
    pub configs: Mutex<std::collections::HashMap<Uuid, AnalysisConfigView>>,
}

#[async_trait]
impl AnalysisConfigClient for InMemoryAnalysisConfigClient {
    async fn get_analysis_config(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfigView>, AnalysisConfigClientError> {
        Ok(self.configs.lock().unwrap().get(&tenant_id).cloned())
    }

    async fn put_analysis_config(
        &self,
        tenant_id: Uuid,
        role: Role,
        input: AnalysisConfigInput<'_>,
    ) -> Result<AnalysisConfigView, AnalysisConfigClientError> {
        if !role.at_least(Role::Operator) {
            return Err(AnalysisConfigClientError::Rejected(403));
        }
        let view = AnalysisConfigView {
            prompt: input.prompt.to_string(),
            provider: input.provider,
            model: input.model.map(str::to_string),
            endpoint: input.endpoint.map(str::to_string),
            api_key: input.api_key.map(str::to_string),
        };
        self.configs.lock().unwrap().insert(tenant_id, view.clone());
        Ok(view)
    }
}

pub struct FailingAnalysisConfigClient;

#[async_trait]
impl AnalysisConfigClient for FailingAnalysisConfigClient {
    async fn get_analysis_config(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfigView>, AnalysisConfigClientError> {
        Err(AnalysisConfigClientError::Unreachable("simulated failure".to_string()))
    }

    async fn put_analysis_config(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _input: AnalysisConfigInput<'_>,
    ) -> Result<AnalysisConfigView, AnalysisConfigClientError> {
        Err(AnalysisConfigClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn get_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({
            "tenant_id": "11111111-1111-1111-1111-111111111111",
            "prompt": "look for urgent tickets",
            "provider": "azure_foundry",
            "model": null,
            "endpoint": null,
            "api_key": null,
            "updated_at": "2026-07-19T00:00:00Z"
        }))
        .into_response()
    }
    async fn put_handler(
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        Json(serde_json::json!({
            "tenant_id": "11111111-1111-1111-1111-111111111111",
            "prompt": body["prompt"],
            "provider": body.get("provider").cloned().unwrap_or(serde_json::json!("azure_foundry")),
            "model": body.get("model").cloned().unwrap_or(serde_json::Value::Null),
            "endpoint": body.get("endpoint").cloned().unwrap_or(serde_json::Value::Null),
            "api_key": body.get("api_key").cloned().unwrap_or(serde_json::Value::Null),
            "updated_at": "2026-07-19T00:00:00Z"
        }))
        .into_response()
    }
    let app = Router::new().route("/v1/analysis-config", get(get_handler).put(put_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_gets_the_configured_prompt_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAnalysisConfigClient::new(reqwest::Client::new(), url);

    let config = client.get_analysis_config(Uuid::new_v4()).await.unwrap().expect("should exist");
    assert_eq!(config.prompt, "look for urgent tickets");
}

#[tokio::test]
async fn http_client_puts_a_new_prompt_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAnalysisConfigClient::new(reqwest::Client::new(), url);

    let config = client
        .put_analysis_config(
            Uuid::new_v4(),
            Role::Operator,
            AnalysisConfigInput {
                prompt: "flag policy violations",
                provider: AnalysisProvider::AzureFoundry,
                model: None,
                endpoint: None,
                api_key: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(config.prompt, "flag policy violations");
}

#[tokio::test]
async fn http_client_puts_an_openai_compatible_config_and_round_trips_the_new_fields() {
    let url = spawn_stub_server().await;
    let client = HttpAnalysisConfigClient::new(reqwest::Client::new(), url);

    let config = client
        .put_analysis_config(
            Uuid::new_v4(),
            Role::Operator,
            AnalysisConfigInput {
                prompt: "flag urgent issues",
                provider: AnalysisProvider::OpenAiCompatible,
                model: Some("qwen3:8b"),
                endpoint: Some("http://localhost:11434/v1"),
                api_key: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(config.provider, AnalysisProvider::OpenAiCompatible);
    assert_eq!(config.model, Some("qwen3:8b".to_string()));
    assert_eq!(config.endpoint, Some("http://localhost:11434/v1".to_string()));
}

#[tokio::test]
async fn http_client_put_is_rejected_for_insufficient_role() {
    let url = spawn_stub_server().await;
    let client = HttpAnalysisConfigClient::new(reqwest::Client::new(), url);

    let err = client
        .put_analysis_config(
            Uuid::new_v4(),
            Role::Viewer,
            AnalysisConfigInput {
                prompt: "x",
                provider: AnalysisProvider::AzureFoundry,
                model: None,
                endpoint: None,
                api_key: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AnalysisConfigClientError::Rejected(403)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpAnalysisConfigClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.get_analysis_config(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, AnalysisConfigClientError::Unreachable(_)));
}
