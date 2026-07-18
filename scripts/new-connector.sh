#!/usr/bin/env bash
# Scaffold a new connector crate under crates/connectors/<name>, implementing the shared
# Connector trait from `common`, with a contract test against the RawRecord schema.
#
# Usage: scripts/new-connector.sh <connector-name>
set -euo pipefail

NAME="${1:?usage: new-connector.sh <connector-name>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATE_DIR="$ROOT/crates/connectors/$NAME"

if [ -d "$CRATE_DIR" ]; then
  echo "error: $CRATE_DIR already exists" >&2
  exit 1
fi

mkdir -p "$CRATE_DIR/src" "$CRATE_DIR/tests"
MOD_NAME="$(echo "$NAME" | tr '-' '_')"

cat > "$CRATE_DIR/Cargo.toml" <<EOF
[package]
name = "connector-$NAME"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
common = { path = "../../common" }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
EOF

cat > "$CRATE_DIR/src/lib.rs" <<EOF
mod connector;

pub use connector::${MOD_NAME^}Connector;
EOF

cat > "$CRATE_DIR/src/connector.rs" <<EOF
#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_trait::async_trait;
use common::connector::Connector;
use common::raw_record::{RawRecord, SourceType};

pub struct ${MOD_NAME^}Connector {
    pub connector_id: String,
}

impl ${MOD_NAME^}Connector {
    pub fn new(connector_id: impl Into<String>) -> Self {
        Self { connector_id: connector_id.into() }
    }
}

#[async_trait]
impl Connector for ${MOD_NAME^}Connector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::Generic
    }

    async fn poll(&self, _tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, common::connector::ConnectorError> {
        // TODO: implement source-specific polling logic.
        Ok(Vec::new())
    }
}
EOF

cat > "$CRATE_DIR/src/connector_test.rs" <<EOF
use super::*;

#[test]
fn reports_its_own_connector_id() {
    let c = ${MOD_NAME^}Connector::new("$NAME");
    assert_eq!(c.connector_id(), "$NAME");
}
EOF

cat > "$CRATE_DIR/tests/raw_record_contract_test.rs" <<EOF
use common::raw_record::RawRecord;
use connector_$MOD_NAME::${MOD_NAME^}Connector;
use common::connector::Connector;

#[tokio::test]
async fn poll_returns_records_conforming_to_raw_record_schema() {
    let connector = ${MOD_NAME^}Connector::new("$NAME");
    let tenant_id = uuid::Uuid::new_v4();
    let records: Vec<RawRecord> = connector.poll(tenant_id).await.expect("poll should not error");
    for r in records {
        assert_eq!(r.tenant_id, tenant_id);
        assert_eq!(r.connector_id, connector.connector_id());
    }
}
EOF

echo "Scaffolded connector crate: crates/connectors/$NAME"
echo "Add \"crates/connectors/$NAME\" to the workspace members list in the root Cargo.toml."
