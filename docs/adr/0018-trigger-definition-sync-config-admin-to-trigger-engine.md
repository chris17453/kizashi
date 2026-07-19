# ADR-0018: Trigger definition sync — config-admin-service publishes, trigger-engine consumes

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Discovered while live-verifying the Record Journey feature (docs/features.md, 2026-07-19
entry): `config-admin-service` is the only service with a CRUD API for `TriggerDefinition`
(`POST/PUT /v1/trigger-definitions`, backing the Console UI's Triggers page), and it writes
those rows to its own `config_admin_service.trigger_definitions` Postgres schema. But
`trigger-engine` — the service that actually *evaluates* triggers against every
`record.analyzed` message (`crates/trigger-engine/src/process_analyzed_record.rs`) — reads
triggers exclusively from its own, separate `trigger_engine.trigger_definitions` schema
(`crates/trigger-engine/src/trigger_repository.rs`). Nothing keeps the two in sync.

Each service owning its own Postgres schema is deliberate (see `crates/*/migrations`, one set
per service) and consistent with the "no service reads another service's schema directly"
principle spec §2 states. But the practical effect here is that a trigger created or edited
through the Console UI — the only trigger-authoring surface that exists — never reaches the
component that fires it. In this dev environment, `trigger_engine.trigger_definitions` already
holds thousands of rows inserted directly (likely by an earlier load-test), which is how
triggers have "worked" in ad-hoc testing so far without anyone noticing the gap. This is a
functional gap, not cosmetic: the entire Triggers feature is decorative without it.

## Decision

`config-admin-service` publishes a `trigger.changed` message (fanout exchange, same shape as
`record.ingested`/`record.normalized`/`record.analyzed`/`event.created`) on every successful
create/update of a `TriggerDefinition`, carrying the full definition. `trigger-engine` gains a
new consumer that upserts the definition into its own `trigger_definitions` table by `id`.

Rejected alternatives:
- **trigger-engine calls config-admin-service's HTTP API per lookup.** `active_triggers_for`
  runs on every `record.analyzed` message — per the standing scale target (thousands of
  inboxes/agents, hundreds of source APIs), that's a synchronous cross-service HTTP round-trip
  in the hottest path in the system. A local Postgres read stays fast at that volume; an
  event-driven cache does not add per-message latency.
- **Both services read the same physical table (drop the per-service schema boundary here).**
  Breaks the "each service owns its schema" convention with no compensating benefit, and this
  fix should generalize to other config surfaces later (e.g. if API keys or retention policies
  ever need consumption elsewhere) — the event-driven pattern already exists three other times
  in this system (`record.ingested` → `record.normalized` → `record.analyzed` →
  `event.created`); reusing it here is the smaller, more consistent change.
- **A scheduled full-resync poll (trigger-engine polls config-admin-service on a timer).**
  Works but adds propagation lag and doesn't fit the fanout-per-change pattern already used
  everywhere else on this bus; only worth it if trigger-engine needed to *discover* triggers it
  doesn't yet know the id of, which it doesn't — config-admin-service is the sole writer.

New exchange constant `TRIGGER_CHANGED_EXCHANGE = "trigger.changed"` in `crates/common/src/
bus.rs`, alongside the four existing pipeline exchanges. Message payload is the
`TriggerDefinition` itself (already `Serialize`/`Deserialize`) — no new message type needed.

Deletes are out of scope for this ADR: `TriggerDefinition` has no delete endpoint today (only
create/update, `enabled: false` is how a trigger is turned off), so upsert-only sync is
sufficient. If a delete endpoint is added later, it needs its own `trigger.deleted` event or a
tombstone convention — flagged here, not solved here.

## Consequences

- **Backfill for pre-existing rows.** Any `TriggerDefinition` created before this ADR lands
  exists only in `config_admin_service.trigger_definitions` and will not retroactively appear
  in `trigger_engine.trigger_definitions` — publishing only happens on new writes going
  forward. A one-time backfill (a small `psql`/script step, or a `config-admin-service`
  startup routine that republishes every existing row once) is needed per environment that
  already has real trigger data; call this out explicitly during rollout, don't let it be a
  silent gap the same way the original one was.
- **Eventual consistency window.** Between a trigger being created/updated and trigger-engine's
  consumer processing the `trigger.changed` message, there's a small window where the new/edited
  definition isn't yet active. Acceptable for this system (same eventual-consistency shape as
  every other pipeline hop already), but worth noting in the Console UI if a "trigger not yet
  active" indicator is ever wanted.
- **trigger-engine's schema is now a read replica, not a second source of truth** — any direct
  writes to `trigger_engine.trigger_definitions` (as was done ad hoc for load-testing so far)
  will be silently overwritten the next time `config-admin-service` republishes that same `id`,
  or will drift permanently if the id was never created in `config-admin-service` at all. Test
  fixtures/seed scripts should create triggers through `config-admin-service`'s API from now on,
  not by inserting into `trigger_engine`'s schema directly.
