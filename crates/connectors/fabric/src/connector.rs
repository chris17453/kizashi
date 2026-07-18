#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::connector::{Connector, ConnectorError};
use common::raw_record::{RawRecord, SourceType};
use connector_runtime::fetch_access_token;
use tiberius::{AuthMethod, Client, Config, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

const FABRIC_SQL_SCOPE: &str = "https://analysis.windows.net/powerbi/api/.default";

/// Polls Fabric's SQL analytics endpoint (ADR-0003, ADR-0013's "Fabric ships SQL endpoint
/// only" decision) — the same operator-configured `SELECT` and stateless-cursor design as the
/// `sql` connector, just over TDS with an Entra AAD access token in place of a
/// username/password (`tiberius::AuthMethod::aad_token`). `trust_server_certificate` exists
/// only for testing against a local TLS-self-signed TDS server standing in for Fabric — real
/// Fabric endpoints always present a valid certificate, so production config should never set
/// it.
pub struct FabricConnector {
    connector_id: String,
    host: String,
    port: u16,
    database: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    query: String,
    trust_server_certificate: bool,
}

impl FabricConnector {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connector_id: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        query: impl Into<String>,
        trust_server_certificate: bool,
    ) -> Self {
        Self {
            connector_id: connector_id.into(),
            host: host.into(),
            port,
            database: database.into(),
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            query: query.into(),
            trust_server_certificate,
        }
    }
}

#[async_trait]
impl Connector for FabricConnector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::FabricRecord
    }

    async fn poll(&self, tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        let access_token = fetch_access_token(
            &self.token_url,
            &self.client_id,
            &self.client_secret,
            FABRIC_SQL_SCOPE,
        )
        .await
        .map_err(|e| ConnectorError::AuthFailed(e.to_string()))?;

        let mut config = Config::new();
        config.host(&self.host);
        config.port(self.port);
        config.database(&self.database);
        config.authentication(AuthMethod::aad_token(access_token));
        if self.trust_server_certificate {
            config.trust_cert();
        }

        let tcp = TcpStream::connect(config.get_addr())
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;
        tcp.set_nodelay(true).map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;

        let mut client: Client<Compat<TcpStream>> =
            Client::connect(config, tcp.compat_write()).await.map_err(map_tiberius_error)?;

        let rows = client
            .simple_query(&self.query)
            .await
            .map_err(map_tiberius_error)?
            .into_first_result()
            .await
            .map_err(map_tiberius_error)?;

        Ok(rows
            .iter()
            .map(|row| {
                RawRecord::new(
                    self.connector_id.clone(),
                    SourceType::FabricRecord,
                    tenant_id,
                    row_to_json(row),
                )
            })
            .collect())
    }
}

fn map_tiberius_error(e: tiberius::error::Error) -> ConnectorError {
    match e {
        tiberius::error::Error::Server(_) => ConnectorError::AuthFailed(e.to_string()),
        other => ConnectorError::SourceUnavailable(other.to_string()),
    }
}

/// Best-effort dynamic column decoding, same approach as the `sql` connector's `row_to_json`
/// (tiberius has no "decode as JSON regardless of type" primitive either).
fn row_to_json(row: &Row) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (index, column) in row.columns().iter().enumerate() {
        let value = if let Some(v) = row.get::<i64, _>(index) {
            serde_json::Value::from(v)
        } else if let Some(v) = row.get::<i32, _>(index) {
            serde_json::Value::from(v)
        } else if let Some(v) = row.get::<f64, _>(index) {
            serde_json::Value::from(v)
        } else if let Some(v) = row.get::<bool, _>(index) {
            serde_json::Value::from(v)
        } else if let Some(v) = row.get::<DateTime<Utc>, _>(index) {
            serde_json::Value::String(v.to_rfc3339())
        } else if let Some(v) = row.get::<&str, _>(index) {
            serde_json::Value::String(v.to_string())
        } else {
            serde_json::Value::Null
        };
        object.insert(column.name().to_string(), value);
    }
    serde_json::Value::Object(object)
}
