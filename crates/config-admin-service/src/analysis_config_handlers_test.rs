use super::*;
use crate::analysis_config_publisher::analysis_config_publisher_test::{
    FailingAnalysisConfigPublisher, InMemoryAnalysisConfigPublisher,
};
use crate::analysis_config_repository::analysis_config_repository_test::{
    FailingAnalysisConfigRepository, InMemoryAnalysisConfigRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AnalysisConfigState) -> Router {
    Router::new()
        .route("/v1/analysis-config", get(get_analysis_config).put(put_analysis_config))
        .with_state(state)
}

fn default_state() -> AnalysisConfigState {
    AnalysisConfigState {
        repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        publisher: Arc::new(InMemoryAnalysisConfigPublisher::default()),
    }
}

async fn send(
    app: Router,
    method: &str,
    tenant_header: Option<Uuid>,
    role_header: Option<&str>,
    body: Option<serde_json::Value>,
) -> axum::response::Response {
    let mut req = Request::builder().method(method).uri("/v1/analysis-config");
    if let Some(t) = tenant_header {
        req = req.header("x-tenant-id", t.to_string());
    }
    if let Some(r) = role_header {
        req = req.header("x-role", r);
    }
    let body = match body {
        Some(b) => {
            req = req.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&b).unwrap())
        }
        None => Body::empty(),
    };
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn get_returns_none_when_no_config_exists() {
    let state = default_state();
    let response = send(router(state), "GET", Some(Uuid::new_v4()), None, None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, serde_json::json!(null));
}

#[tokio::test]
async fn get_requires_tenant_header() {
    let response = send(router(default_state()), "GET", None, None, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_creates_config_and_returns_it() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(state),
        "PUT",
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({"prompt": "look for urgent tickets"})),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: AnalysisConfig = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(config.tenant_id, tenant_id);
    assert_eq!(config.prompt, "look for urgent tickets");
}

#[tokio::test]
async fn put_then_get_round_trips() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();
    send(
        router(state.clone()),
        "PUT",
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({"prompt": "flag policy violations"})),
    )
    .await;

    let response = send(router(state), "GET", Some(tenant_id), None, None).await;
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: AnalysisConfig = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(config.prompt, "flag policy violations");
}

#[tokio::test]
async fn put_accepts_and_round_trips_provider_model_endpoint_and_api_key() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(state),
        "PUT",
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({
            "prompt": "flag urgent issues",
            "provider": "openai_compatible",
            "model": "qwen3:8b",
            "endpoint": "http://localhost:11434/v1",
            "api_key": "unused-for-ollama",
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: AnalysisConfig = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(config.provider, common::AnalysisProvider::OpenAiCompatible);
    assert_eq!(config.model, Some("qwen3:8b".to_string()));
    assert_eq!(config.endpoint, Some("http://localhost:11434/v1".to_string()));
    assert_eq!(config.api_key, Some("unused-for-ollama".to_string()));
}

#[tokio::test]
async fn put_without_provider_fields_defaults_to_azure_foundry() {
    let state = default_state();
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(state),
        "PUT",
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({"prompt": "look for urgent tickets"})),
    )
    .await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: AnalysisConfig = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(config.provider, common::AnalysisProvider::AzureFoundry);
    assert!(config.model.is_none());
}

#[tokio::test]
async fn put_rejects_a_viewer_role() {
    let response = send(
        router(default_state()),
        "PUT",
        Some(Uuid::new_v4()),
        Some("viewer"),
        Some(serde_json::json!({"prompt": "x"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn put_requires_role_header() {
    let response = send(
        router(default_state()),
        "PUT",
        Some(Uuid::new_v4()),
        None,
        Some(serde_json::json!({"prompt": "x"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_publishes_analysis_config_changed() {
    let publisher = Arc::new(InMemoryAnalysisConfigPublisher::default());
    let state = AnalysisConfigState {
        repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        publisher: publisher.clone(),
    };
    let tenant_id = Uuid::new_v4();
    send(
        router(state),
        "PUT",
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({"prompt": "look for urgent tickets"})),
    )
    .await;

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].tenant_id, tenant_id);
}

#[tokio::test]
async fn put_succeeds_even_when_publish_fails() {
    let state = AnalysisConfigState {
        repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        publisher: Arc::new(FailingAnalysisConfigPublisher),
    };
    let response = send(
        router(state),
        "PUT",
        Some(Uuid::new_v4()),
        Some("operator"),
        Some(serde_json::json!({"prompt": "x"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_returns_500_on_backend_failure() {
    let state = AnalysisConfigState {
        repository: Arc::new(FailingAnalysisConfigRepository),
        publisher: Arc::new(InMemoryAnalysisConfigPublisher::default()),
    };
    let response = send(router(state), "GET", Some(Uuid::new_v4()), None, None).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
