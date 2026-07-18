#!/usr/bin/env bash
# Scaffold a new service crate under crates/ with the standard layout: src/, tests/,
# workspace-consistent Cargo.toml, a smoke test that compiles, and a healthcheck endpoint stub.
#
# Usage: scripts/new-service.sh <service-name>
set -euo pipefail

NAME="${1:?usage: new-service.sh <service-name>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATE_DIR="$ROOT/crates/$NAME"

if [ -d "$CRATE_DIR" ]; then
  echo "error: $CRATE_DIR already exists" >&2
  exit 1
fi

mkdir -p "$CRATE_DIR/src" "$CRATE_DIR/tests"

cat > "$CRATE_DIR/Cargo.toml" <<EOF
[package]
name = "$NAME"
version = "0.1.0"
edition = "2021"
license = "MIT"
publish = false

[dependencies]
common = { path = "../common" }
axum = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

[dev-dependencies]
reqwest = { workspace = true }
EOF

cat > "$CRATE_DIR/src/main.rs" <<EOF
use $( echo "$NAME" | tr '-' '_' )::build_router;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "$NAME listening");
    axum::serve(listener, build_router()).await.expect("server error");
}
EOF

cat > "$CRATE_DIR/src/lib.rs" <<EOF
mod health;

pub use health::build_router;
EOF

cat > "$CRATE_DIR/src/health.rs" <<'EOF'
#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

use axum::{routing::get, Router};

pub fn build_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz() -> &'static str {
    "ok"
}
EOF

cat > "$CRATE_DIR/src/health_test.rs" <<'EOF'
use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_200() {
    let app = build_router();
    let response = app
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
EOF

cat > "$CRATE_DIR/tests/smoke_test.rs" <<EOF
#[test]
fn crate_compiles() {
    assert!(true, "smoke test: $NAME crate builds and links");
}
EOF

echo "Scaffolded crate: crates/$NAME"
echo "Add \"crates/$NAME\" to the workspace members list in the root Cargo.toml."
