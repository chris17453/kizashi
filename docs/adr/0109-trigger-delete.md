# ADR-0109: Trigger Delete

- **Status:** accepted
- **Date:** 2026-07-20

## Context

ADR-0108 gave Triggers an enable/disable toggle to close the feature-parity gap with Sensors
and Retention Policies, but explicitly deferred delete: "config-admin-service has no delete
endpoint for trigger definitions yet." A follow-up audit pass confirmed that gap was still
real — `TriggerDefinitionRepository` had no `delete` method, `TriggerPublisher` only ever
published a bare `TriggerDefinition` (no way to signal "this no longer exists"), and
trigger-engine's own mirrored copy of every trigger definition (the one it actually evaluates
against `record.analyzed` messages, per ADR-0018) had no corresponding sync path either. Once
created, a trigger could be disabled forever but never actually removed, in either
config-admin-service's Postgres or trigger-engine's. Sensors already solved the identical
problem: `SensorChangeEvent` (`Upserted(Sensor)` / `Deleted { id, tenant_id }`) lets
agent-scheduler's consumer distinguish an upsert from a removal over the same fanout exchange,
since a delete has no entity payload to carry.

## Decision

Mirror the Sensor pattern exactly rather than inventing a new shape:

- New `common::TriggerChangeEvent` enum (`Upserted(TriggerDefinition)` / `Deleted { id,
  tenant_id }`), replacing the bare `TriggerDefinition` previously published on
  `trigger.changed`. This is a breaking wire-format change to that exchange, accepted since
  config-admin-service (publisher) and trigger-engine (sole consumer) ship together in this
  same PR.
- `TriggerDefinitionRepository::delete` (config-admin-service) and `TriggerRepository::delete`
  (trigger-engine) — both simple id-scoped deletes, the former inside an audit-logged
  transaction (`ChangeType::Deleted`, same shape as create/update) matching
  `SensorRepository::delete`.
- `DELETE /v1/trigger-definitions/:id` (operator-gated, actor-attributed) in
  config-admin-service, publishing `TriggerChangeEvent::Deleted` after a successful delete.
- trigger-engine's `trigger.changed` consumer now matches on `TriggerChangeEvent::Upserted` vs
  `::Deleted` and calls `upsert`/`delete` on its own repository accordingly, instead of always
  upserting.
- `TriggersClient::delete_trigger` (Console UI) and `POST /triggers/:id/delete`
  (`trigger_delete_handler.rs`), operator-gated, with a confirm() dialog on the Remove button —
  same shape as `post_delete_retention_policy`.

## Consequences

A trigger's full lifecycle (create, disable/enable, delete) is now available end-to-end through
the Console UI, with proper RBAC gating, audit-log actor attribution, and — critically —
trigger-engine's own evaluation copy staying in sync on delete, not just on create/update. The
`trigger.changed` wire format changed shape (enum instead of bare struct); any future consumer
of that exchange must deserialize `TriggerChangeEvent`, not `TriggerDefinition` directly. No
bulk-delete was added for triggers (unlike Sensors/Retention Policies) since it wasn't part of
the identified gap and can be added later as its own small increment if needed.
