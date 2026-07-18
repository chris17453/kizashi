# ADR-0009: Auth service v1 scope: local login plus unified OIDC

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §4/§8 requires three auth modules: Entra ID (OIDC), local login (Postgres, hashed
credentials), and generic OAuth (any OIDC-compliant provider). ADR-0008 (Query Gateway) already
established the session mechanism downstream services trust: an opaque bearer token, hashed at
rest, resolved to a `tenant_id` via `query_gateway.query_api_tokens`. ADR-0008 explicitly framed
Auth Service's job as becoming *the thing that writes rows into that table* after a real login,
not a different mechanism layered on top.

Two scoping questions:

1. **Entra vs. generic OAuth are the same protocol.** Entra ID is itself an OIDC-compliant
   provider — its authorization endpoint, token endpoint, and userinfo endpoint all follow the
   same OIDC contract any other compliant provider does. Building a separate "Entra client" and
   a separate "generic OAuth client" would mean two implementations of the same
   authorization-code-plus-PKCE exchange, differing only in which URLs and client credentials
   are configured. That duplication buys nothing.
2. **A full browser-redirect login flow is inherently interactive.** Real OAuth2/OIDC always
   requires a human in a browser to authenticate with the IDP and consent; no amount of
   automated testing can exercise that hop end-to-end without a browser driver against a real
   (or Selenium-automated fake) IDP, which is disproportionate for this build-out.

## Decision

Auth Service ships two login paths in v1, both ending in the same place: a POST to Query
Gateway's new internal `POST /internal/tokens` endpoint (added in this PR) to mint a session
token for the authenticated tenant, so Auth Service never touches `query_api_tokens` directly
(spec §2 principle 1).

1. **Local login** (`POST /v1/auth/local/login`): username/password checked against
   `auth_service.local_users` (Argon2id-hashed, spec §8 "hashed credentials"), fully functional
   and fully tested — no external dependency.
2. **Unified OIDC** (Entra and generic OAuth are the *same* client, configured per-tenant):
   `GET /v1/auth/oidc/:provider/authorize` returns the authorization URL (with PKCE challenge)
   for the caller to redirect a browser to — Auth Service does not perform the redirect itself,
   since it has no session/cookie layer yet (that's Console UI's job once it exists).
   `POST /v1/auth/oidc/:provider/callback` accepts the authorization code the browser received
   and completes the code-for-token exchange plus a userinfo fetch, then mints the session the
   same way local login does. The OIDC client itself (`oauth2` crate) is fully unit-testable
   against a stub IDP implementing the standard `/authorize`, `/token`, `/userinfo` endpoints —
   what isn't automated is the human browser hop in between, which is true of any OIDC
   integration, not a gap specific to this build-out.

## Consequences

- Easier: one OIDC client implementation serves both "Entra ID" and "generic OAuth" spec
  requirements — a tenant configured with Entra's endpoints and a tenant configured with any
  other OIDC provider's endpoints go through identical code. Local login has zero external
  dependencies and is exercised by real Postgres integration tests, same as every other
  Postgres-backed service so far.
- Harder: there is no cookie-based browser session yet — a caller must carry the bearer token
  itself (as an `Authorization: Bearer` header, same as any other Query Gateway client) rather
  than relying on a set-cookie response from Auth Service. Browser-session ergonomics (secure
  cookies, CSRF protection, logout) are Console UI's responsibility when it's built, not
  something to bolt onto Auth Service speculatively now.
