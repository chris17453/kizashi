# ADR-0026: End-to-end tenant-isolation test for Query Gateway's real proxy path

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Query Gateway is spec §6's designated single tenant-enforcement point for all UI/dashboard
traffic: it resolves a caller's bearer token to a tenant via `TokenStore`, then forwards the
request to Dashboard API with `X-Tenant-Id` set from that *resolved* identity — Dashboard API
never re-derives identity itself and trusts the header completely (documented directly in
`dashboard_api::handlers::tenant_id_from_headers`'s doc comment). This is the exact mechanism
CLAUDE.md §5's tenant-isolation rule is written for, and an audit found it had the thinnest
test coverage of any tenant boundary in the codebase: `proxy_handler_test.rs` only asserted
header-forwarding behavior against a mocked `TokenStore` and a stubbed upstream — nothing
proved that two *real*, independently-minted session tokens for two different tenants actually
produce different, correctly-scoped results through the real HTTP hop.

## Decision

Added `crates/query-gateway/tests/tenant_isolation_integration_test.rs`, which spins up:
- a real `dashboard-api` server (`dashboard_api::build_router`) backed by a real
  `ClickHouseEventQueryRepository` against real ClickHouse,
- a real `query-gateway` server (`query_gateway::build_router`) backed by a real
  `PostgresTokenStore` against real Postgres, pointed at the dashboard-api instance above,

mints two real session tokens via `TokenStore::mint_token` (the same code path Auth Service
uses in production, not a hand-rolled test double), inserts one ClickHouse event owned by
tenant A, and proves through actual HTTP requests to the real gateway that:
1. tenant A's token retrieves its own event (200, correct body),
2. tenant B's token requesting the *same event id* gets nothing (404) — proving the resolved
   identity, not any client-supplied value, is what reaches the downstream query,
3. listing events through tenant A's token never includes a row tagged with another tenant_id,
4. a token that was never minted is rejected (401) before ever reaching dashboard-api.

`dashboard-api` was added as a query-gateway dev-dependency specifically for this test — the
only way to prove the *real* proxy path end-to-end is to run the *real* downstream service, not
a stand-in.

## Consequences

- This is the strongest tenant-isolation proof in the codebase to date: two independently-real
  services, real credential minting, real network hops, real ClickHouse row-level scoping —
  not mocks at any layer.
- Confirms (rather than fixes) existing behavior: `proxy_handler.rs` already builds the
  outbound request with only its own resolved `x-tenant-id` header, never forwarding the
  original request's headers wholesale, so a malicious client-supplied `X-Tenant-Id` was
  already structurally unable to leak through — this ADR records that this is now verified by
  a real end-to-end test, not established solely by code inspection.
