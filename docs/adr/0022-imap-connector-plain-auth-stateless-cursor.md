# ADR-0022: IMAP connector — plain LOGIN auth, stateless date cursor, real-server integration testing

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Phase 5 of the gap-closing roadmap calls for an IMAP inbound connector for non-M365 mail
(Gmail, self-hosted, anything speaking RFC 3501) — the six connectors built so far
(`zendesk`, `graph-mail`, `graph-teams`, `sql`, `fabric`, `generic`) cover HTTP APIs and SQL
wire protocols, but nothing that speaks IMAP directly. Three real design questions came up
building it: how to authenticate, how to avoid re-fetching the same messages on every poll
without a persisted cursor (ADR-0013's stateless-CronJob-process constraint), and how to test
an IMAP client against something more real than mocks.

## Decision

1. **Auth is plain IMAP `LOGIN` (username/password) in v1.** Every IMAP-speaking mail
   provider supports it, including self-hosted servers. XOAUTH2 (required by Gmail/Workspace
   if password auth is disabled, and generally preferred for security) is deferred to a
   follow-up — it needs a per-provider OAuth2 token refresh flow this crate doesn't have
   infrastructure for yet, and building it now would mean guessing at a shape before there's a
   real provider driving it. `connector-runtime::fetch_access_token` already handles the
   client-credentials flow for Graph/Fabric; a user-delegated refresh-token flow for IMAP
   XOAUTH2 is a different enough shape that it's a separate follow-up, not a small addition.
2. **The connector takes `since_date` (a plain calendar date) as an IMAP `SEARCH SINCE`
   cursor, passed in per invocation** — the same stateless-cursor design `zendesk` already
   uses for its `start_time` (ADR-0013): the connector does not persist any state itself,
   since each CronJob run is a fresh process. IMAP's `SINCE` search only has day granularity
   (not a timestamp), so unlike Zendesk's Unix-timestamp `start_time` this is a
   `chrono::NaiveDate`; a caller polling more than once a day will re-fetch that day's earlier
   messages, an acceptable/known v1 limitation (idempotent re-ingestion, not silent data loss
   or duplication downstream — Ingestion Service's write path is already idempotent-by-design
   per its own ADR). Per-message UID-based incremental tracking is a natural follow-up once
   connector-config gets a real persisted-state store (tracked, not built here).
3. **TLS is configurable (`use_tls: bool`, defaulting to `true` in `main.rs`), not
   hardcoded.** Production IMAP servers overwhelmingly run TLS-only (implicit TLS on 993 or
   STARTTLS on 143) and that's the default; plain-TCP support exists both for the rare
   on-prem server that terminates TLS elsewhere, and pragmatically because it's what makes the
   connector's own live integration tests possible (below) without needing to also stand up a
   trusted TLS cert for a throwaway test server. This is implemented as a small
   `ImapStream` enum (TLS variant wrapping `async-native-tls`'s `TlsStream`, plain variant a
   raw `TcpStream`) with manual `AsyncRead`/`AsyncWrite` impls that dispatch to whichever
   variant is active, rather than duplicating `ImapConnector::poll`'s body per transport.
4. **Live integration testing uses a real IMAP server (`greenmail/standalone`), the same "test
   against the real thing" convention already used for Postgres/RabbitMQ/ClickHouse/MinIO (and
   a throwaway SQL Server container standing in for Fabric's TDS protocol).** Greenmail is a
   small, fast-starting, in-memory IMAP+SMTP test server built exactly for this purpose —
   `docker run -d -p 13143:3143 -p 13025:3025 -e
   GREENMAIL_OPTS='-Dgreenmail.setup.test.smtp -Dgreenmail.setup.test.imap
   -Dgreenmail.users=testuser:testpass@example.com -Dgreenmail.hostname=0.0.0.0'
   greenmail/standalone:2.0.1`, seeded with a real message via `curl --url smtp://...
   --upload-file`, then polled with the real connector (TLS off, since greenmail's test image
   ships no certificate) via `tests/imap_connector_integration_test.rs`, gated on
   `IMAP_TEST_HOST`/`IMAP_TEST_PORT`/`IMAP_TEST_USERNAME`/`IMAP_TEST_PASSWORD` env vars, same
   `expect()`-panics-if-unset convention `FABRIC_TEST_HOST`/`FABRIC_TEST_PORT` already
   established. This actually exercises the real TCP connect, real IMAP `LOGIN`/`SELECT`/
   `SEARCH`/`FETCH` command sequence, and real RFC822 body parsing — not a mock of any of it.

## Consequences

- Message-mapping logic (`parse_message`, RFC822 bytes → `RawRecord`) is a pure function
  unit-tested against static byte fixtures, kept separate from `ImapConnector::poll`'s network
  I/O, matching this codebase's existing split between pure business logic (fast, exhaustive
  unit tests) and I/O boundaries (slower, real-infra integration tests).
- Known gap, explicitly not built here: XOAUTH2 auth, UID-based incremental cursor tracking,
  and routing this connector's outbound TCP through Egress Gateway (ADR-0021's HTTP CONNECT
  tunnel doesn't support raw non-HTTP protocols like IMAP) — all tracked as follow-ups in
  `docs/features.md`, not silently dropped.
- A new `greenmail/standalone:2.0.1` image dependency exists for local/CI testing only — it is
  never part of the deployed platform, the same relationship the throwaway `mssql` container
  has to Fabric's tests.
