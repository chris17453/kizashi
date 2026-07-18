# ADR-0013: Connectors v1 scope: shared poller runtime, env-driven per-tenant config, Fabric SQL endpoint only

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §6 service #1 (Connectors/Agents) is "CronJob-scheduled pollers" (spec §3) — one process
invocation per poll cycle that reads from a source system, maps to `RawRecord` (`common`'s
`Connector` trait, already scaffolded by `scripts/new-connector.sh`), and hands the results to
Ingestion Gateway. Six connectors are named in spec §5.1's `connector_id` examples: `zendesk`,
`graph:mail`, `graph:teams`, plus `sql`, `fabric:sql`/`fabric:onelake` (ADR-0003), and a
`generic` HTTP connector for sources with no dedicated integration yet.

Every connector needs the same two pieces of infrastructure: (1) an HTTP client that posts
polled records to Ingestion Gateway's `POST /v1/ingest` with an API key, and (2) for the three
Entra-backed sources (`graph:mail`, `graph:teams`, `fabric:sql` per ADR-0003), an OAuth2
client-credentials token fetch. Building this once and sharing it avoids six near-identical
copies of retry/error-handling logic drifting apart.

Connector configuration (which tenant, which source-specific credentials) is listed under
Config/Admin Service in spec §6's table, but ADR-0010 deliberately did not build
connector-config CRUD there yet ("no real consumer exists yet") — this PR is that consumer,
raising the same scope question ADR-0011 already answered for retention policy.

## Decision

1. **A new shared library crate, `crates/connectors/connector-runtime`**, provides:
   - `IngestionClient` trait + `HttpIngestionClient` impl — `POST {ingestion_gateway_url}/v1/ingest`
     with the connector's API key, one request per polled `RawRecord` (matching Ingestion
     Gateway's existing single-record `POST /v1/ingest` shape — no batch endpoint exists yet,
     out of scope here).
   - `run_poll_cycle(connector, tenant_id, ingestion_client) -> PollSummary` — calls
     `connector.poll(tenant_id)`, posts each record, counts successes/failures, logs but does
     not abort on a single record's post failure (consistent with Ingestion Service's own
     "durable write is the source of truth, publish failure doesn't roll back" philosophy —
     here, one record failing to post shouldn't lose the rest of the batch).
   - `entra_client_credentials::fetch_token(tenant_id, client_id, client_secret, scope)` — the
     OAuth2 client-credentials (app-only) flow ADR-0003 specifies, shared by `graph-mail`,
     `graph-teams`, and `fabric`, using the same `oauth2` crate `auth-service` already depends
     on (a new grant type on an existing dependency, not a new one).
2. **Connector configuration is env-var-driven per process invocation in v1** — one CronJob
   instance = one tenant + one connector + one set of source credentials via env vars (e.g.
   `TENANT_ID`, `ZENDESK_SUBDOMAIN`, `ZENDESK_EMAIL`, `ZENDESK_API_TOKEN`,
   `INGESTION_GATEWAY_URL`, `INGESTION_GATEWAY_API_KEY`). This mirrors every other service's
   env-driven config in this repo and matches the actual CronJob deployment shape (spec §3) —
   one scheduled job per tenant/connector pair is how the orchestrator would invoke this
   regardless of where the config value originates. Wiring these processes up to read from
   Config/Admin Service's future connector-config CRUD (rather than raw env vars) is tracked as
   follow-up, the same deferred-consolidation pattern ADR-0010 and ADR-0011 already established
   — not silently skipped.
3. **Fabric connector ships the SQL endpoint variant only** (`fabric:sql:<dataset>` from
   ADR-0003), not OneLake. Fabric's SQL analytics endpoint is queried the same way the generic
   `sql` connector queries any database — a T-SQL `SELECT` — just with an Entra access token in
   place of a username/password, so it reuses the `sql` connector's row-mapping logic behind a
   different auth path. OneLake is a file-storage API with a materially different polling model
   (list+read blobs, not query rows) and is deferred as its own connector, not built as a stub.

## Consequences

- Easier: every connector's `main.rs` is a thin composition of `connector-runtime`'s pieces
  plus source-specific polling logic — the auth/posting/error-counting code exists exactly
  once; adding a seventh connector later is mostly "implement `Connector::poll`," not
  "reimplement the whole CronJob shape."
- Harder: none of these connectors can be integration-tested against a live external account
  (no real Zendesk/Entra/Fabric tenant credentials available here) — every connector's tests
  verify request/response shape against a real stub HTTP server standing in for the external
  API (the same level of "real" testing this repo's other external-HTTP clients already use),
  not a live third-party account; this is an inherent limit of testing third-party SaaS
  integrations in CI, the same caveat ADR-0009 already documents for OIDC's browser hop. Fabric
  OneLake and connector-config-via-Config/Admin-Service remain open, tracked follow-ups.
