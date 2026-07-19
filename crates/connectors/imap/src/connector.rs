#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_imap::types::Fetch;
use async_trait::async_trait;
use common::connector::{Connector, ConnectorError};
use common::raw_record::{RawRecord, SourceType};
use futures_util::TryStreamExt;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::message::parse_message;

/// Either a TLS or a plain TCP connection to the IMAP server, so `ImapConnector::poll` doesn't
/// need to be duplicated per transport. TLS is the production default (`use_tls: true`); plain
/// exists for on-prem servers that terminate TLS elsewhere and, pragmatically, for testing
/// against a real IMAP server (greenmail) that doesn't ship a trusted TLS cert (ADR-0022).
enum ImapStream {
    Tls(Compat<async_native_tls::TlsStream<Compat<tokio::net::TcpStream>>>),
    Plain(tokio::net::TcpStream),
}

impl std::fmt::Debug for ImapStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImapStream::Tls(_) => write!(f, "ImapStream::Tls"),
            ImapStream::Plain(_) => write!(f, "ImapStream::Plain"),
        }
    }
}

impl AsyncRead for ImapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
            ImapStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ImapStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
            ImapStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_flush(cx),
            ImapStream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
            ImapStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Polls a generic IMAP mailbox (Gmail, self-hosted, anything RFC 3501) for messages received
/// since `since_date`, mapping each to a `RawRecord`. Auth is plain username/password (IMAP's
/// `LOGIN` command) — XOAUTH2 is deferred to a follow-up since it needs a per-provider token
/// refresh flow this v1 doesn't have infrastructure for yet (see ADR-0022).
///
/// Follows the same stateless-cursor design as `zendesk` (ADR-0013): `since_date` is passed in
/// per invocation (e.g. computed by the scheduler as "now - poll interval") rather than the
/// connector persisting a cursor itself, since each CronJob run is a fresh process.
pub struct ImapConnector {
    connector_id: String,
    host: String,
    port: u16,
    username: String,
    password: String,
    mailbox: String,
    since_date: chrono::NaiveDate,
    use_tls: bool,
}

impl ImapConnector {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connector_id: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
        mailbox: impl Into<String>,
        since_date: chrono::NaiveDate,
        use_tls: bool,
    ) -> Self {
        Self {
            connector_id: connector_id.into(),
            host: host.into(),
            port,
            username: username.into(),
            password: password.into(),
            mailbox: mailbox.into(),
            since_date,
            use_tls,
        }
    }
}

#[async_trait]
impl Connector for ImapConnector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::Message
    }

    async fn poll(&self, tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        let tcp_stream = tokio::net::TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;

        let stream = if self.use_tls {
            let tls = async_native_tls::TlsConnector::new();
            let tls_stream = tls.connect(&self.host, tcp_stream.compat()).await.map_err(|e| {
                ConnectorError::SourceUnavailable(format!("TLS handshake failed: {e}"))
            })?;
            ImapStream::Tls(tls_stream.compat())
        } else {
            ImapStream::Plain(tcp_stream)
        };

        let client = async_imap::Client::new(stream);
        let mut session = client
            .login(&self.username, &self.password)
            .await
            .map_err(|(e, _client)| ConnectorError::AuthFailed(e.to_string()))?;

        session
            .select(&self.mailbox)
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(format!("SELECT failed: {e}")))?;

        let search_query = format!("SINCE {}", self.since_date.format("%d-%b-%Y"));
        let uids = session
            .uid_search(&search_query)
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(format!("SEARCH failed: {e}")))?;

        if uids.is_empty() {
            let _ = session.logout().await;
            return Ok(Vec::new());
        }

        let uid_set = uids.into_iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let mut records = Vec::new();
        {
            let mut fetch_stream = session
                .uid_fetch(&uid_set, "RFC822")
                .await
                .map_err(|e| ConnectorError::SourceUnavailable(format!("FETCH failed: {e}")))?;

            while let Some(fetch) = fetch_stream.try_next().await.map_err(|e| {
                ConnectorError::SourceUnavailable(format!("FETCH stream error: {e}"))
            })? {
                if let Some(record) = fetch_to_record(&fetch, &self.connector_id, tenant_id)? {
                    records.push(record);
                }
            }
        }

        let _ = session.logout().await;
        Ok(records)
    }
}

fn fetch_to_record(
    fetch: &Fetch,
    connector_id: &str,
    tenant_id: uuid::Uuid,
) -> Result<Option<RawRecord>, ConnectorError> {
    let Some(uid) = fetch.uid else {
        return Ok(None);
    };
    let Some(body) = fetch.body() else {
        return Ok(None);
    };
    parse_message(uid, body, connector_id, tenant_id).map(Some)
}
