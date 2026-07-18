# ADR-0010: Config admin service v1 scope

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §6 (service #11) lists Config/Admin Service's job as: "Manages connector configs,
normalization mappings, event type definitions, trigger definitions, retention policy,
branding/theming." That is six distinct config entity types. CLAUDE.md §5 additionally requires
that "every admin/config change is logged immutably... If a feature adds a new mutable config
entity, it ships with an audit-log write in the same PR" — not a follow-up.

Two of these entity types already exist and are already load-bearing: Normalization Service
(feature/0003) and Trigger Engine (feature/0005) each interim-own their `NormalizationMapping`
and `TriggerDefinition` tables directly in their own Postgres schemas, with each service's own
code explicitly documenting that this is temporary — "v1 owns this data directly... rather than
depending on Config/Admin Service (not yet built)" — and that the real fix, once Config/Admin
Service exists, is for those services to read through its API instead. Migrating both of those
already-shipped, already-tested services to read from a new service in the same PR that
introduces that service is a materially riskier, larger change than building the new service
alone — two working pipelines' read paths would change at once.

Building full CRUD + immutable audit logging for all six entity types in one PR, several of
which (connector configs, retention policy, branding/theming) have no consumer yet — no
connector crate reads a "connector config" today, no retention-service or console UI reads
branding/theming yet — would mean designing those schemas against consumers that don't exist,
guessing at shapes rather than building against a real need.

## Decision

Config Admin Service ships in this PR with full CRUD (create, read, update, list) plus an
immutable audit log for the two entity types that already have real, running consumers:
`TriggerDefinition` and `NormalizationMapping`. Both live in Config Admin Service's own Postgres
schema now — this PR does **not** migrate Trigger Engine or Normalization Service to read
through it yet, since that read-path cutover is exactly the kind of change that deserves its
own focused PR with its own test coverage of the migration, not a footnote in the service that
introduces the new authority. That cutover is tracked as explicit follow-up work, not silently
left undone.

`EventTypeDefinition`, connector configs, retention policy, and branding/theming are **not**
implemented in this PR — no stub endpoints, no 501-Not-Implemented placeholders (CLAUDE.md §0:
"No half-finished implementations"). They are added when the services that will actually
consume them exist (retention-service for retention policy, a connector crate for connector
configs, Console UI for branding/theming, an event-classification consumer for
EventTypeDefinition), each following the same audit-logged-CRUD pattern this PR establishes.

Every mutation (create/update) writes one immutable `config_audit_log` row in the same
transaction as the entity change, per CLAUDE.md §5 — before/after state, actor placeholder
(Auth Service's session doesn't carry a user identity yet beyond tenant, so `actor` records the
tenant_id for now; a real user identity is added once Console UI/session context exists),
timestamp, and change type.

## Consequences

- Easier: TriggerDefinition and NormalizationMapping get a real, tested, audited management API
  today, exactly matching what those two already-shipped services need to eventually read from.
  The audit-log pattern established here (one row per mutation, in the same transaction,
  before/after state) is the template every future entity type in this service follows —
  proven once, not redesigned per entity.
- Harder: Normalization Service and Trigger Engine still read their own tables directly, not
  through this service, until the follow-up migration PR lands — so Config Admin Service's
  writes are not yet the operational source of truth those pipelines actually consume. This is
  the deliberate, documented gap, not a silent inconsistency: both services' code comments
  already say so, and this ADR is the tracking record for closing it.
