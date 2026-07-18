# ADR-0008: Query gateway interim auth model

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §6 (service #8) defines Query Gateway as "Single dashboard/UI-facing entry point; user
auth enforcement" — the console/UI-facing analogue of Ingestion Gateway (service #2, built in
feature/0002). Real user auth (spec §4, §8) is Entra ID OIDC, local Postgres-hashed-credential
login, and generic OAuth, all owned by Auth Service (spec §6, service #10 — not built yet in
this sequence; it's task #9 in this build-out's plan, after this one).

Query Gateway cannot be deferred until Auth Service exists — Dashboard/Query API Service (spec
§6, service #9, built alongside Query Gateway in this PR) has nothing to sit behind otherwise,
and every later console/UI work depends on there being a query path to build against. Spec §8's
core invariant — "gateway layer: auth context scopes all downstream queries" — is what actually
matters architecturally, not which identity provider issued the credential.

## Decision

Query Gateway authenticates with **opaque bearer tokens**, hashed at rest exactly like
Ingestion Gateway's API keys (SHA-256, never the plaintext) and resolved to a `tenant_id` the
same way. This is deliberately the same shape as `ingestion-gateway`'s `ApiKeyStore` — not
duplicated by choice of laziness, but because the underlying mechanism (bearer token → tenant
identity, hashed storage, gateway-layer resolution) is identical regardless of *how* the token
was issued. The `query_api_tokens` table this PR creates is exactly what Auth Service will
write into once it exists: a successful Entra/local/OAuth login mints a row here (or an
equivalent this schema migrates into Auth Service's ownership), not a fundamentally different
mechanism Query Gateway has to learn later.

No rate limiting is added — spec §6 only lists rate limiting for Ingestion Gateway's row, not
Query Gateway's, and dashboard read traffic has a different shape (interactive, human-paced)
than agent/connector ingestion traffic.

## Consequences

- Easier: Dashboard/Query API Service is buildable and demoable today; the token table and
  resolution logic Query Gateway ships now is not throwaway — Auth Service's eventual job is to
  become the thing that *writes rows into this table* (or a renamed equivalent) after a real
  login, not to replace the mechanism.
- Harder: there is no login flow yet — tokens must be issued manually (e.g. directly via SQL,
  or a small admin script) until Auth Service ships. No refresh, no expiry-driven re-auth, no
  RBAC beyond tenant scoping. This is the explicit, temporary gap Auth Service (task #9) closes;
  it is not silently presented as "auth is done."
