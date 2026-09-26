use super::*;
use axum::{body::Body, http::Request, Router};
use std::sync::Mutex;
use tower::ServiceExt;

const SECRET: &str = "brief-secret";

struct FakeGenerator {
    tenant_id: Mutex<Option<Uuid>>,
}

#[async_trait]
impl IncidentBriefGenerator for FakeGenerator {
    async fn generate(
        &self,
        tenant_id: Uuid,
        _evidence: serde_json::Value,
    ) -> Result<String, String> {
        *self.tenant_id.lock().unwrap() = Some(tenant_id);
        Ok("Impact is bounded; investigate the failing connector next.".to_string())
    }
}

fn app(generator: Arc<dyn IncidentBriefGenerator>) -> Router {
    build_router(BriefState { generator, internal_secret: SECRET.to_string() })
}

#[tokio::test]
async fn brief_requires_internal_secret() {
    let response = app(Arc::new(FakeGenerator { tenant_id: Mutex::new(None) }))
        .oneshot(
            Request::post("/v1/incident-brief")
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .header("content-type", "application/json")
                .body(Body::from(r#"{"evidence":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn brief_is_tenant_scoped_and_returns_summary() {
    let tenant_id = Uuid::new_v4();
    let generator = Arc::new(FakeGenerator { tenant_id: Mutex::new(None) });
    let response = app(generator.clone())
        .oneshot(
            Request::post("/v1/incident-brief")
                .header("x-internal-secret", SECRET)
                .header("x-tenant-id", tenant_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(r#"{"evidence":{"events":2}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(*generator.tenant_id.lock().unwrap(), Some(tenant_id));
}
