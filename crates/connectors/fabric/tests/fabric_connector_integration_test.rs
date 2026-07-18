//! Integration test against a real TDS server (CLAUDE.md §2) — a plain SQL Server container
//! standing in for Fabric's SQL analytics endpoint, since both speak TDS and this connector's
//! Entra-token login path is exercised identically either way (ADR-0013). Requires
//! FABRIC_TEST_HOST/FABRIC_TEST_PORT pointing at a real TDS server.
//!
//! What this genuinely proves: the real TCP connect, real TDS handshake, and real
//! `AuthMethod::aad_token` login attempt all execute against a live server, and a rejected
//! login is correctly classified as `ConnectorError::AuthFailed` — a plain (non-Fabric) SQL
//! Server always rejects an AAD token login, so this cannot prove the happy-path query
//! actually returns rows against real Fabric data; that requires a real Fabric tenant, the
//! same inherent limitation ADR-0009 already documents for OIDC's browser hop.

use axum::response::Json;
use axum::routing::post;
use axum::Router;
use common::connector::{Connector, ConnectorError};
use connector_fabric::FabricConnector;

async fn spawn_stub_token_server() -> String {
    async fn handler() -> Json<serde_json::Value> {
        Json(
            serde_json::json!({"access_token": "fake-token", "token_type": "bearer", "expires_in": 3600}),
        )
    }
    let app = Router::new().route("/token", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/token")
}

fn test_host_port() -> (String, u16) {
    let host =
        std::env::var("FABRIC_TEST_HOST").expect("FABRIC_TEST_HOST must be set to run this test");
    let port: u16 = std::env::var("FABRIC_TEST_PORT")
        .expect("FABRIC_TEST_PORT must be set to run this test")
        .parse()
        .expect("FABRIC_TEST_PORT must be a port number");
    (host, port)
}

#[tokio::test]
async fn an_aad_token_rejected_by_a_real_tds_server_is_reported_as_auth_failed() {
    let (host, port) = test_host_port();
    let token_url = spawn_stub_token_server().await;

    let connector = FabricConnector::new(
        "fabric",
        host,
        port,
        "master",
        token_url,
        "client-id",
        "client-secret",
        "SELECT 1 AS id",
        true,
    );

    let err = connector.poll(uuid::Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, ConnectorError::AuthFailed(_)));
}

#[tokio::test]
async fn unreachable_tds_server_is_reported_as_source_unavailable() {
    let token_url = spawn_stub_token_server().await;

    let connector = FabricConnector::new(
        "fabric",
        "127.0.0.1",
        1, // nothing listens on port 1
        "master",
        token_url,
        "client-id",
        "client-secret",
        "SELECT 1",
        true,
    );

    let err = connector.poll(uuid::Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, ConnectorError::SourceUnavailable(_)));
}

#[tokio::test]
async fn unreachable_token_endpoint_is_reported_as_auth_failed() {
    let (host, port) = test_host_port();

    let connector = FabricConnector::new(
        "fabric",
        host,
        port,
        "master",
        "http://127.0.0.1:1",
        "client-id",
        "client-secret",
        "SELECT 1",
        true,
    );

    let err = connector.poll(uuid::Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, ConnectorError::AuthFailed(_)));
}
