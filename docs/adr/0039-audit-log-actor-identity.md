# ADR-0039: Audit log entries record the real actor, not the tenant id

## Status

Accepted.

## Context

A live UI audit (screenshotting every Console UI page, prompted by direct user feedback) found
the Audit History page displaying raw UUIDs in its "Actor" column instead of usernames. Tracing
this back surfaced a much bigger, systemic problem than a display bug: **every audit-log write
across the entire platform records `tenant_id` as the `actor`**, never the identity of the user
who actually performed the action.

```rust
// crates/auth-service/src/local_user_repository.rs (before)
actor: user.tenant_id.to_string(),
```

This pattern was present in every backend service that writes to CLAUDE.md §5's required
immutable audit log:
- `auth-service` (local user create/update-role/delete)
- `config-admin-service` (sensors, triggers, normalization mappings, analysis config)
- `retention-service` (retention policies)
- `ingestion-gateway` (API key create/revoke)

Since every audit row is already tenant-scoped (`tenant_id` is its own column), recording the
tenant id *again* as the "actor" made the field pure noise — the audit trail could never answer
its core question, "who did this," which is the entire point of an immutable audit log for a
platform whose spec explicitly assumes a customer's compliance team will eventually review it.

## Decision

- **Wire contract**: the Console UI sends a new header, `X-Username` (case-insensitive, sent as
  `x-username` matching this codebase's existing lowercase convention for `x-tenant-id`/
  `x-role`), carrying the signed-in session's username, on every mutating (create/update/
  delete) request.
- Each backend service gained a `username_from_headers(headers) -> Result<String, (StatusCode,
  &'static str)>` helper (named and shaped identically across all four services, mirroring the
  existing `tenant_id_from_headers`/`role_from_headers` pattern) that reads this header and
  returns `401 "missing X-Username header"` if absent.
- Every repository method that writes an `AuditLogEntry` gained an `actor: &str` parameter
  (previously several didn't have one at all — e.g. `LocalUserRepository::create` — and simply
  hardcoded the tenant id inline). Handlers now extract the real username via the new helper and
  thread it through instead of `tenant_id.to_string()`.
- **Landed as one coordinated change**, not four independent PRs, deliberately: since the
  backend services *require* the header once they read it, and the UI is the only caller that
  sends it, merging backend-only or UI-only would put the two out of sync — either 401s on every
  admin write (backend merged first) or silently-still-wrong audit rows (UI merged first, before
  backends read it). This was implemented via six parallel work units (one per backend service,
  two for the UI client layer split by file group) sharing one exact wire contract, then
  integrated and verified together before shipping.

## Consequences

- The Audit History page (and any future compliance tooling) can now show/query on real
  usernames.
- Non-UI callers of these APIs (direct `curl`/API integrations, if any exist) must now send
  `X-Username` on writes or receive `401`. No such callers are known to exist against these
  specific endpoints today (they're Console-UI-only per ADR-0010's direct-call trust boundary),
  but this is a breaking change to the wire contract worth flagging if that assumption changes.
- `agent-scheduler`'s own writes to `config-admin-service` (if any — verified none exist; it only
  *reads* sensor config) are unaffected.
- This does not add authentication/authorization for the header's value — the UI is trusted to
  send its session's real username, same trust boundary as `X-Tenant-Id`/`X-Role` already have.
  A user cannot forge a *different* user's identity through this header any more easily than
  they already could forge `X-Role` — this is an internal service-to-service contract behind the
  Console UI's own session authentication, not a new external trust boundary.
