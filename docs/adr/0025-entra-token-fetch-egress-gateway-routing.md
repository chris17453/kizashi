# ADR-0025: Route the Entra client-credentials token fetch through Egress Gateway

- **Status:** accepted
- **Date:** 2026-07-19

## Context

ADR-0021 (Egress Gateway) and its follow-up connector wiring explicitly logged a known gap:
`connector_runtime::fetch_access_token` — the Entra ID app-only (client-credentials) flow used
by `graph-mail`, `graph-teams`, and `fabric` — built its own `reqwest::Client` internally via
`oauth2::reqwest::async_http_client`, bypassing `EGRESS_PROXY_URL` entirely even when a
connector's data-plane calls were already proxied. This meant the call to
`login.microsoftonline.com` (or any customer-configured Entra tenant's token endpoint) was
invisible to Egress Gateway's audit log and allowlist — a real hole in "every outbound call
this platform makes to an external system routes through one proxy," which is the entire point
of ADR-0021.

## Decision

`fetch_access_token` now takes a caller-provided `reqwest::Client` instead of building one
internally, and performs the OAuth2 HTTP exchange through it via a custom
`request_async` closure (`oauth2` v4 pins `http` 0.2 while `reqwest` v0.12 pins `http` 1.x, so
the closure does an explicit bytes-level method/status/header conversion between them — there is
no ecosystem type reuse available here). Every caller now passes the exact same
`build_outbound_client`-constructed client it already uses for its data-plane calls:

- `graph-mail`/`graph-teams`: pass `self.client.clone()` — they already hold a proxied client
  for their Graph API calls (wired in the connector-wiring PR after ADR-0021).
- `fabric`: gets a **new** `token_client: reqwest::Client` field, since its data path is TDS
  (`tiberius`), not HTTP — there was previously no `reqwest::Client` anywhere in this
  connector. `main.rs` now builds one via `build_outbound_client` with the same
  opt-in `EGRESS_PROXY_URL` treatment every other connector's outbound client gets.
- `action-executor`'s `GraphSendMailActionDispatcher` (ADR-0024): passes `self.client.clone()`
  — the same client it already uses for the `sendMail` call itself.

No new opt-in surface is introduced — this reuses each caller's existing
`EGRESS_PROXY_URL`-controlled client, so a deployment that hasn't opted in still behaves
identically (an unproxied default client), and one that has opted in now gets its token fetch
covered too, closing the gap without a new config knob.

## Consequences

- `fetch_access_token`'s signature is a breaking change within this workspace (internal-only
  crate, no external consumers) — every call site was updated in the same PR.
- Live-verified against the real deployed `egress-gateway`: ran `connector-fabric` locally with
  `EGRESS_PROXY_URL` pointed at it and deliberately-invalid Entra credentials; the token request
  reached the real `login.microsoftonline.com` and was rejected (expected, fake credentials),
  and a direct Postgres query confirmed `egress_gateway.egress_audit_log` recorded
  `login.microsoftonline.com:443` with the correct `tenant_id`/`connector_id` — proving the
  token endpoint call is now genuinely tunneled and audited, not just accepted-and-ignored
  config.
- Known gap, still not closed: `action-executor`'s `HttpActionDispatcher`/`SmtpActionDispatcher`
  and every connector's *data-plane* calls were already covered by the original ADR-0021
  wiring; IMAP/SMTP's raw TCP protocols remain structurally unable to route through an HTTP
  CONNECT tunnel (documented in ADR-0022/ADR-0023) — unchanged by this PR.
