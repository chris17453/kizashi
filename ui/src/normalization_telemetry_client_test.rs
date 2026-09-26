use super::*;
use axum::{routing::get, Json, Router};
use std::future::IntoFuture;
use std::net::SocketAddr;

#[tokio::test]
async fn http_client_reads_a_tenant_scoped_dedup_summary() {
    let app = Router::new().route(
        "/v1/dedup/summary",
        get(|| async {
            Json(DedupSummary {
                fingerprint_count: 3,
                active_duplicate_count: 2,
                suppressed_count: 7,
            })
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());

    let client =
        HttpNormalizationTelemetryClient::new(reqwest::Client::new(), format!("http://{address}"));
    let summary = client.dedup_summary(Uuid::new_v4()).await.unwrap();
    assert_eq!(summary.suppressed_count, 7);
}
