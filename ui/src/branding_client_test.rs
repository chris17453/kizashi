use super::*;
use axum::extract::{Json as JsonExtractor, Path, State};
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryBrandingClient {
    pub branding: Mutex<Option<Branding>>,
    pub put_calls: Mutex<Vec<(Uuid, Role, String, Branding)>>,
}

#[async_trait]
impl BrandingClient for InMemoryBrandingClient {
    async fn get_branding(&self, _tenant_name: &str) -> Result<Branding, BrandingClientError> {
        self.branding.lock().unwrap().clone().ok_or(BrandingClientError::UnknownWorkspace)
    }

    async fn get_branding_by_id(&self, _tenant_id: Uuid) -> Result<Branding, BrandingClientError> {
        self.branding.lock().unwrap().clone().ok_or(BrandingClientError::UnknownWorkspace)
    }

    async fn put_branding(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        branding: Branding,
    ) -> Result<(), BrandingClientError> {
        self.put_calls.lock().unwrap().push((tenant_id, role, actor.to_string(), branding));
        Ok(())
    }
}

async fn spawn_stub_server(branding: Option<Branding>) -> String {
    async fn get_handler(
        State(branding): State<Option<Branding>>,
        Path(_name): Path<String>,
    ) -> axum::response::Response {
        match branding {
            Some(b) => Json(b).into_response(),
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }
    async fn put_handler(
        Path(_id): Path<Uuid>,
        JsonExtractor(_body): JsonExtractor<Branding>,
    ) -> axum::response::Response {
        axum::http::StatusCode::OK.into_response()
    }
    let app = Router::new()
        .route("/v1/tenants/:name/branding", get(get_handler).put(put_handler))
        .with_state(branding);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_get_branding_returns_the_stored_values() {
    let branding = Branding {
        product_name: Some("Acme Signals".to_string()),
        logo_url: Some("https://acme.example.com/logo.png".to_string()),
        accent_color: Some("#ff6600".to_string()),
    };
    let url = spawn_stub_server(Some(branding.clone())).await;
    let client = HttpBrandingClient::new(reqwest::Client::new(), url);

    let result = client.get_branding("acme").await.unwrap();
    assert_eq!(result, branding);
}

#[tokio::test]
async fn http_client_get_branding_returns_unknown_workspace_on_404() {
    let url = spawn_stub_server(None).await;
    let client = HttpBrandingClient::new(reqwest::Client::new(), url);

    let err = client.get_branding("nonexistent").await.unwrap_err();
    assert!(matches!(err, BrandingClientError::UnknownWorkspace));
}

#[tokio::test]
async fn http_client_put_branding_succeeds() {
    let url = spawn_stub_server(None).await;
    let client = HttpBrandingClient::new(reqwest::Client::new(), url);

    let result = client
        .put_branding(
            Uuid::new_v4(),
            Role::Admin,
            "alice",
            Branding { product_name: Some("Acme".to_string()), logo_url: None, accent_color: None },
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpBrandingClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.get_branding("acme").await.unwrap_err();
    assert!(matches!(err, BrandingClientError::Unreachable(_)));
}
