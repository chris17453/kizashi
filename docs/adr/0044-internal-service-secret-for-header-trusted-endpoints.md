# 0044. Require X-Internal-Secret on every endpoint that trusts X-Role/X-Tenant-Id/X-Username

## Context

Several backend services — config-admin-service, trigger-engine, auth-service, and
retention-service's retention-policy endpoints — trust `X-Role`, `X-Tenant-Id`, and/or
`X-Username` HTTP headers at face value to make authorization decisions (ADR-0016 RBAC v1 and
its follow-ups). The intended design is that only the Console UI, which holds the real
authenticated session server-side, ever sets these headers when calling a backend service on a
logged-in user's behalf.

A targeted audit (prompted by the same "IBM-level enterprise compliance" bar driving this
session's work, and following directly from the tenant-isolation fixes in ADR-0043) found that
this was never actually enforced as a trust boundary: every one of these services publishes its
port directly in `docker-compose.yml` (`config-admin-service` 8090, `trigger-engine` 8087,
`auth-service` 8089, `retention-service` 8091), so any network caller able to reach the host —
not just the Console UI — could set `X-Role: admin` (or any tenant id, or any username) on a
request and have it trusted outright. No ADR ever claimed network isolation, a service mesh, or
mTLS as a mitigation; the header-trust design was documented as "the trust boundary" without ever
asserting or enforcing that the port itself was unreachable by an untrusted party. This is a real,
unauthenticated privilege-escalation and cross-tenant-access path, not a documented/accepted
trade-off — the same class of finding as ADR-0042's retention-service ops-endpoints gap, just not
yet closed for the header-trusting endpoints.

## Decision

Every endpoint that trusts `X-Role`/`X-Tenant-Id`/`X-Username` now additionally requires a
shared-secret header, `X-Internal-Secret`, matching an `INTERNAL_API_SECRET` environment
variable — the exact v1 stopgap pattern already used for query-gateway's `/internal/tokens`
(ADR-0009) and retention-service's `/v1/sweep`/`/v1/reimport` (ADR-0042), now extended
consistently to every other header-trusting endpoint:

- **config-admin-service**: an axum middleware (`internal_secret::require_internal_secret`,
  `axum::middleware::from_fn_with_state`) applied once to the merged admin+sensor+
  analysis-config+saved-search-query router, everything except `/healthz`.
- **trigger-engine**: the same middleware pattern applied to the whole `/v1/triggers/*` router,
  `/healthz` (a separate, unmerged router) stays exempt.
- **auth-service**: the router is split into `public_routes` (login, SSO authorize/callback, the
  by-name branding lookup — all pre-session, browser-facing, never read the trust headers) and
  `protected_routes` (tenant branding PUT, user CRUD, user audit log — all reached only via an
  already-authenticated Console UI session), with the middleware layered on `protected_routes`
  only.
- **retention-service**: rather than a new middleware, the existing per-handler
  `has_valid_internal_secret` check (already used by `/v1/sweep`/`/v1/reimport` per ADR-0042) was
  made `pub(crate)` and called at the top of every `policy_handlers.rs` handler, matching that
  file's existing per-handler check style (`tenant_mismatch`, `require_operator`,
  `username_from_headers`) rather than introducing a second enforcement mechanism in the same
  crate.

Two different implementation shapes (router-level middleware vs. per-handler check) were used
deliberately: middleware where a service's routes are homogeneous enough that a single gate point
covers everything cleanly (config-admin-service, trigger-engine, auth-service), and a per-handler
call where the target file already had an established per-handler check convention and a working
shared-secret helper to extend (retention-service). Both close the same gap; consistency with each
file's existing style mattered more than uniformity of mechanism across crates.

**Console UI wiring**: rather than threading a new parameter through every one of the UI's ~15
`Http*Client` structs, `X-Internal-Secret` is set once as a default header on the single shared
`reqwest::Client` in `ui/src/main.rs` that gets cloned into every backend client. Every outbound
call from the UI carries it automatically, including to services that don't check it
(observability, ingestion-gateway, action-executor, egress-gateway) — those simply ignore the
extra header. `action-executor`'s `HttpTriggerClient` (the only other production caller of a
now-gated endpoint — it calls trigger-engine's `GET /v1/triggers/:id`) got the secret added
directly as a constructor parameter instead, since it's a single, already-parameterized client
rather than a fleet of them.

## Consequences

- `INTERNAL_API_SECRET` is now a required env var for config-admin-service, trigger-engine, and
  kizashi-ui in addition to auth-service and retention-service, which already required it.
  `docker-compose.yml` wires all of them to the same `${INTERNAL_API_SECRET:-change-me-in-production}`
  default — a single shared secret across every internal caller, not a per-pair secret. This is a
  v1 stopgap, same as ADR-0009/ADR-0042: it proves "the caller knows a secret only Kizashi's own
  services should know," not per-service identity. A real service-identity mechanism (mTLS, a
  service mesh, or short-lived per-service tokens) is a larger investment tracked as a future
  hardening item, not blocking this fix.
  This means the shared secret must be present wherever any of these five services deploys — a
  hard failure (`.expect(...)` panic on missing env var) at startup, not a silent bypass, if
  someone forgets it.
- `GET /v1/tenants/id/:id/branding` in auth-service was found during this work to have no
  check at all (not even `X-Role`/`X-Tenant-Id`) — it returns non-sensitive branding fields
  (product name, logo URL, accent color) by tenant id, which is a deliberately public-shaped
  lookup analogous to the by-name endpoint used pre-login. Left as-is; flagged here rather than
  silently expanded in scope, since it's a design choice (branding is not sensitive data) rather
  than the same class of bug as the endpoints this ADR closes.
- Any new handler added to one of these four services in the future must be added to the
  protected side of the router (or call the shared check) to inherit this protection — the
  router-level middleware approach used in three of the four services makes this the default
  rather than something that has to be remembered per-handler, directly addressing the "sibling
  handler missing a check" failure mode found twice already this session (ADR-0043).
