use super::*;
use crate::fingerprint_repository::fingerprint_repository_test::InMemoryFingerprintRepository;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

#[tokio::test]
async fn summary_requires_internal_secret_and_tenant_scope() {
    let tenant_id = Uuid::new_v4();
    let repository = Arc::new(InMemoryFingerprintRepository::default());
    repository.check_and_record(tenant_id, "abc", Uuid::new_v4(), None).await.unwrap();
    let app = build_router(DedupState {
        fingerprint_repository: repository,
        internal_secret: "secret".to_string(),
    });

    let unauthorized = app
        .clone()
        .oneshot(Request::builder().uri("/v1/dedup/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/dedup/summary")
                .header("x-internal-secret", "secret")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["fingerprint_count"], 1);
    assert_eq!(json["suppressed_count"], 0);
}
