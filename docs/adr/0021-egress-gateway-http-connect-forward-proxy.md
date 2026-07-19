# ADR-0021: Egress Gateway — HTTP CONNECT forward proxy with per-tenant audit logging

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Gap-closing roadmap Phase 4 (`docs/adr/` peers, gap analysis backlog): every outbound HTTP
call this platform makes to a system it doesn't own — connectors polling Zendesk/Microsoft
Graph/Fabric/customer SQL endpoints, `action-executor`'s `HttpActionDispatcher` posting to a
trigger's webhook URL, `connector-runtime::fetch_access_token`'s OAuth2 token fetches — happens
today as a direct, unlogged, unrestricted `reqwest` call from whichever process makes it. For
an enterprise platform sold to other companies (CLAUDE.md's framing), there is no answer today
to "what external hosts did tenant X's connectors talk to last week," and no mechanism to stop
a misconfigured or compromised connector from reaching an arbitrary host. Spec has no explicit
slot for this component; placing it in the architecture is this ADR's job.

## Decision

New service, `crates/egress-gateway`: an HTTP **CONNECT-method forward proxy**. Every outbound
`reqwest::Client` in this codebase (connectors, `action-executor`'s dispatcher, connector-runtime's
token-fetch path) gets a new optional `EGRESS_PROXY_URL` env var; when set,
`reqwest::Client::builder().proxy(reqwest::Proxy::all(egress_proxy_url).basic_auth(tenant_id,
connector_id))` routes every request — `http://` and `https://` alike — through Egress Gateway.
For `https://` targets (the overwhelming majority: every real integration this platform talks
to), the client issues an HTTP `CONNECT host:443` request first; Egress Gateway logs one audit
row (tenant_id, connector_id, destination host:port, timestamp, allowed/denied) and then, if
allowed, does a raw byte-for-byte TCP relay between the caller and the destination — the TLS
handshake and all subsequent traffic stay end-to-end encrypted between the connector and the
real destination. Egress Gateway therefore **never sees request paths, headers, or bodies for
HTTPS targets** — only the destination host:port and, via `Proxy-Authorization`, which
tenant/connector is calling. This is a deliberate scope boundary: destination-level audit
logging and allowlisting, not deep request inspection/rewriting.

**Identity via `Proxy-Authorization`, not mTLS or a new token type.** `reqwest::Proxy::basic_auth`
already sends `Proxy-Authorization: Basic base64(tenant_id:connector_id)` on every CONNECT —
reusing an HTTP mechanism every proxy-aware client already implements means zero new client-side
protocol work, just a config flag. Egress Gateway parses that header to attribute the audit row;
a CONNECT with no/malformed `Proxy-Authorization` is logged as `tenant_id: unknown` and audited
but not denied in v1 (see Consequences) — attribution quality depends on every caller actually
setting the proxy, which is opt-in, not enforced by anything except operator discipline for now.

**Per-tenant domain allowlist is optional and Egress-Gateway-owned**, not synced from
config-admin-service like Triggers/Agents/AnalysisConfig — this is the one config entity in the
system with no other consumer, so there's no event-driven-sync case to make; Egress Gateway
runs its own small CRUD (`GET/PUT /v1/allowlist`, audit-logged like every other admin surface)
directly. When a tenant has no allowlist configured, every destination is allowed (opt-in
restriction, not default-deny) — matches how this feature is being introduced into an already-
running system rather than a greenfield one.

Rejected: **A literal HTTP forward-proxy with no Kizashi code at all** (e.g. Squid/mitmproxy
pointed at by `.proxy()`). Simpler to stand up, but gets none of the tenant/connector-scoped
audit trail CLAUDE.md §5 requires — a generic proxy sees "some request went to
`api.zendesk.com`," not "tenant X's Zendesk Agent did." Not viable for the actual requirement.

Rejected: **A TLS-terminating/MITM proxy that decrypts and re-encrypts to inspect full request
bodies.** Would let Egress Gateway log/allowlist on URL path or body content, not just
destination host — but requires distributing a custom CA cert to every connector process (real
operational burden and, per the plan's own framing, this is Phase 4's *first* cut; the
CONNECT-tunnel-and-log-destination-only shape is explicitly called out as "the simplest thing,
worth validating before building a heavier request-rewriting proxy"). If destination-only
audit logging turns out insufficient for a customer's compliance need, MITM is the documented
next step, not built speculatively now.

Rejected: **Per-connector explicit client wrapper** (a shared crate function every connector
calls instead of a network-level proxy). Would require every connector's HTTP call site to
opt in individually and offers no protection against a connector bypassing the wrapper
(accidentally or not); a network-level proxy is enforceable at the infrastructure layer
(egress firewall rules can require all outbound traffic to route through the proxy) in a way
a library convention cannot be.

## Consequences

- **Adoption is opt-in per env var, not yet enforced.** Setting `EGRESS_PROXY_URL` on a
  connector/action-executor deployment is what makes it start routing through Egress Gateway;
  nothing today prevents a connector from being deployed without it (that would need network-
  level egress firewall rules restricting all outbound traffic to the gateway's address,
  which is an infrastructure/deployment-environment decision out of this ADR's scope — flagged
  as the real enforcement mechanism, not solved here).
- **HTTPS traffic is logged at the destination-host granularity only** — "tenant X's Zendesk
  Agent connected to `acme.zendesk.com` at 14:32" is knowable; "...and fetched ticket #4521" is
  not, by design (see Decision). If a customer's compliance requirement needs full-body outbound
  audit, that's the MITM follow-up noted above.
- **Unauthenticated CONNECT requests are logged, not rejected**, in v1 — a deliberate choice to
  ship visibility first without breaking any caller that hasn't been updated to set proxy
  credentials yet. Tightening this to reject-if-unattributed is a natural, small follow-up once
  every outbound caller in the codebase has been migrated to set `EGRESS_PROXY_URL`.
- **No allowlist enforcement changes existing behavior** — every tenant's traffic is unrestricted
  by default, matching current (no gateway) behavior exactly, until an operator opts a tenant
  into an allowlist.
