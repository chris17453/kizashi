# ADR-0040: Console UI completes the OIDC/SSO login flow (closes ADR-0009's deferred half)

## Status

Accepted.

## Context

ADR-0009 (Auth Service v1 scope) built a complete, tested OAuth2/OIDC authorization-code-plus-
PKCE client in `auth-service` — `GET /v1/auth/oidc/:provider/authorize` and
`POST /v1/auth/oidc/:provider/callback` — deliberately built to serve both Entra ID and any
other OIDC-compliant "generic OAuth" provider with one implementation (spec §4/§8's three auth
modules). That ADR explicitly deferred the browser-facing half: "there is no cookie-based
browser session yet... Console UI's responsibility when it's built."

Console UI was since built (ADR-0014) and shipped local username/password login, but the OIDC
half was never wired up — no login button, no redirect handling, no callback route. This was
found during a live audit of every Console UI page: the login page only offered local
credentials despite the backend already having full enterprise-SSO support sitting unused. This
is exactly the "half-finished implementation" CLAUDE.md's non-negotiables warn against, not a
missing feature to design from scratch.

## Decision

- **`GET /login/sso?tenant_name=...`**: calls Auth Service's `/authorize`, stashes what the
  callback will need (CSRF token, PKCE verifier, workspace name) behind a new short-lived,
  single-use, `HttpOnly` cookie (`kizashi_oidc_flow`, scoped to `/login/sso`, 10-minute TTL) —
  there is nowhere else server-side to keep it between the two browser hops, same constraint
  ADR-0009 identified. Redirects the browser to the IdP.
  - **`SameSite=Lax`, not `Strict`** (unlike the main session cookie): the browser leaves the
    site for the IdP and returns via a top-level GET redirect — exactly the navigation `Strict`
    cookies are dropped on. Getting this wrong would silently break every SSO login attempt.
- **`GET /login/sso/callback?code=...&state=...`**: reads the flow cookie, **`take`s** (not
  `get`s) the pending flow so a captured/replayed callback URL can never mint a second session
  from the same authorization, verifies `state` against the stored CSRF token (rejecting
  outright on mismatch — this is exactly the forgery `state` exists to catch), then calls Auth
  Service's `/callback` and mints a normal Console UI session (ADR-0014) the same way local
  login does.
- **Auth Service's `OidcCallbackRequest.tenant_id: Uuid` changed to `tenant_name: String`**,
  resolved server-side via the same `tenant_repository.id_for_name` local login already uses —
  Console UI never has a bare `tenant_id` before authentication completes, only the workspace
  name the user typed, so the old contract was unusable as designed and had clearly never been
  exercised by a real caller.
- **`LoginResponse` gained an optional `username` field**, populated only by the OIDC callback
  (`userinfo.email`, falling back to `subject`) — necessary so the session created for an SSO
  user records their *real* identity rather than the workspace name, which matters directly for
  ADR-0039's audit-actor fix landed earlier this session: an SSO user's actions must attribute
  to them, not to "acme".
- **Provider hardcoded to `"entra"` for v1** (`DEFAULT_PROVIDER`), matching the only OIDC client
  Auth Service actually configures today (`entra_oidc_client()`, env-var-driven, one set of
  credentials platform-wide, not yet per-tenant). A per-tenant provider picker is real future
  work, not assumed away — it's simply not what exists to wire up yet.
- OIDC-authenticated sessions still default to `Role::Viewer` (unchanged from ADR-0009's own
  decision — "OIDC has no local role source yet").

## Consequences

- Enterprise customers can now actually sign in via their own identity provider through the
  Console UI, not just via a hand-crafted API call — this was a real, user-visible "this
  platform isn't enterprise-ready" gap, now closed.
- **What is not verified by automated tests, and cannot be**: the actual browser round-trip to
  a real IdP (Entra or otherwise) and back. ADR-0009 already named this limitation — "no amount
  of automated testing can exercise that hop end-to-end without a browser driver against a real
  IDP" — and it still holds. What *is* fully tested: the authorize redirect and cookie-setting
  logic, the callback's CSRF/replay defenses, the tenant-name-to-id resolution, and the full
  code-exchange-to-session-mint path against a stub OIDC server (mirroring the pattern
  `oidc_client_test.rs` already established for the backend client). Live-verified in this
  session: the graceful-degradation path (SSO not configured → clear on-page error, not a crash
  or hang) against the real deployed stack, since this environment has no real Entra tenant to
  test the success path against.
- No SSO provider configuration UI yet — Entra credentials are still env-var-driven
  platform-wide, not a per-tenant admin page. Building that (multi-provider, per-tenant
  configuration, stored and audit-logged) is real follow-up work, tracked but not started.
