# ADR-0104: Action executions immutability trigger

- **Status:** accepted
- **Date:** 2026-07-20

## Context

A thirteenth audit pass, checking migration consistency across every audit-log-shaped table in
the platform, found `action_executions` (action-executor's execution audit log — CLAUDE.md §5,
spec §5.5) was the one exception: its own migration comment already claims "append-only... never
updated in place," but unlike `auth_audit_log`, `config_audit_log`, `retention_audit_log`,
ingestion-gateway's audit log, and egress-gateway's two audit logs (`egress_audit_log`,
`allowlist_audit_log`) — all of which pair table creation with a `BEFORE UPDATE OR DELETE`
rejection trigger — `action_executions` had none. Enforcement was purely
application-level: `ExecutionRepository`'s trait only exposes `insert`/`list_by_event`, no
update/delete method. That's a convention, not a guarantee — a bug bypassing the repository
trait, a future migration, or direct DB access could still mutate or delete rows with nothing at
the database level to stop it, unlike every one of its peers.

## Decision

New migration `0003_action_executions_immutable.sql`, identical shape to
`retention_audit_log_immutable.sql`: a `action_executions_reject_mutation()` trigger function
that `RAISE EXCEPTION`s on `UPDATE`/`DELETE`, applied via a `BEFORE UPDATE OR DELETE` trigger.
Two new real-Postgres integration tests
(`action_executions_rejects_update_at_the_database_level`,
`action_executions_rejects_delete_at_the_database_level`) prove the trigger actually rejects
both operations, matching the regression-test pattern every other audit table in this codebase
already has.

## Consequences

- Every audit-log-shaped table in the platform now has the same DB-level immutability guarantee
  — closes the last gap of this shape.
- No existing behavior changes for legitimate callers: `ExecutionRepository` never issued
  UPDATE/DELETE in the first place, so the trigger only ever fires against something that was
  already a bug or an out-of-band mutation.
