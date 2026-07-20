# ADR-0097: Egress allowlist audit log

- **Status:** accepted
- **Date:** 2026-07-20

## Context

An eighth Console UI audit pass set out to check whether Egress Allowlist (ADR-0021) had an
audit-history link like every other config-mutating page (ADR-0092/0094). It didn't — but the
deeper problem wasn't a missing link, it was that `egress-gateway`'s `PUT /v1/allowlist` never
wrote an audit entry anywhere. CLAUDE.md §5 requires "if a feature adds a new mutable config
entity, it ships with an audit-log write in the same PR" — this entity had shipped with none.
Bolting a UI link onto a non-existent audit trail would have been worse than doing nothing, since
it would misrepresent an unaudited config surface as an audited one.

## Decision

Add real backend audit infrastructure to `egress-gateway`, mirroring config-admin-service's
`audit_log.rs` module shape (a standard `AuditLogEntry`-equivalent table, a `record_*` function
that writes inside the same Postgres transaction as the entity mutation, a `BEFORE UPDATE OR
DELETE` trigger that `RAISE EXCEPTION`s for DB-level immutability, and a read-only reader trait):

- New `allowlist_audit_log` table + `allowlist_audit_log.rs` module with types renamed
  (`AllowlistAuditLogEntry`, `AllowlistChangeType`, etc.) to avoid clashing with
  `egress-gateway`'s pre-existing `AuditLogEntry` (the unrelated proxy-decision log).
  `entity_id` is the tenant_id itself — the allowlist is a singleton-per-tenant resource, not a
  row-based collection, so there's no separate entity to key on.
- `AllowlistRepository::set_domains` gained an `actor: &str` parameter and now runs inside a
  transaction: read the existing row (to compute `before`/`Created`-vs-`Updated`), upsert, write
  the audit row, commit.
- `PUT /v1/allowlist` now requires an `X-Username` header (the actor) in addition to the existing
  `X-Role` operator-or-above check.
- New `GET /v1/audit-log/:entity_id` on `egress-gateway`, deliberately matching the shared shape
  `HttpAuditLogClient` already expects (used by config-admin-service, retention-service, and
  auth-service) — unlike ADR-0094's `IngestionGatewayApiKeyAuditLogClient`, no new UI client type
  is needed. `ui/src/lib.rs`'s `AppState` gained `egress_audit_log_client: Arc<dyn
  AuditLogClient>`, and `audit_log_handler.rs`'s `service` switch gained `"egress" =>
  &state.egress_audit_log_client`.
- `egress_allowlist_handler.rs`'s template gained `tenant_id: Uuid`, and
  `egress_allowlist.html` gained a "View change history" link to
  `/audit-log/egress/{{ tenant_id }}`, matching the pattern on every other config page.

Live-verified against a running stack: saving an allowlist change and then loading
`/audit-log/egress/<tenant_id>` shows the real entry (actor, `created`/`updated`, before/after
domain lists).

## Consequences

- Egress Allowlist is now audited to the same bar as every other mutable config entity in the
  platform — closes a real CLAUDE.md §5 gap, not just a UI-completeness gap.
- `set_domains` callers across the codebase (tests, any future caller) must supply an actor
  string; three call sites needed updating when this shipped.
- The transactional write means a failure recording the audit entry now rolls back the allowlist
  change itself, rather than silently leaving the config mutated with no trail — this is the
  correct failure mode for an audited config surface, but it does mean `PUT /v1/allowlist` can
  now fail for a reason unrelated to the allowlist data itself (an audit-log write/backend
  error).
