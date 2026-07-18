# ADR-0006: Trigger engine event-type classification for v1

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §6 defines the Aggregation/Trigger Engine's job as: "groups by group_key; evaluates
TriggerDefinitions; writes Events; publishes event.created." `TriggerDefinition.event_type_match`
(spec §5.4) is matched against `Event.event_type` (spec §5.2), and `Event.event_type` is meant
to reference an `EventTypeDefinition` (spec §5.3) — a named, versioned classification like
`"sentiment.negative"` with its own field schema.

Nothing in the pipeline built so far actually *produces* that classification. Analysis Service
(built in feature/0004) calls Azure AI Foundry/ML and publishes whatever JSON result the model
returns (e.g. `{"sentiment": -0.8}`) — it does not decide "this is a `sentiment.negative`
event." `EventTypeDefinition` management is Config/Admin Service's job (spec §6, service #11,
task #10 in this build-out — not built yet), and that's also where an operator would define the
mapping from raw analysis output to a named event type. Building a full classification-rule
engine now would mean guessing at Config/Admin Service's eventual API shape before it exists.

## Decision

For v1, Trigger Engine derives candidate event types directly from the shape of the
`AnalyzedRecord.analysis` object it already has, rather than waiting on a not-yet-built
classification config: **every top-level numeric key in `analysis` is treated as an event type
named after that key** (e.g. `analysis: {"sentiment": -0.8}` yields one candidate event with
`event_type = "sentiment"` and that key's value as its numeric signal). `group_key` and
`entity_ref` are both taken from `normalized_payload.entity_ref` when present (falling back to
the record's own id when a mapping hasn't populated `entity_ref` yet), since that's the
convention NormalizationMapping examples already establish for "the thing this record is about."

Each candidate event type is recorded as a durable, tenant/group_key/window-queryable signal
(`analyzed_signals` table, Trigger Engine's own Postgres schema), then every enabled
`TriggerDefinition` whose `event_type_match` equals that key is evaluated against the window's
accumulated count/values via `TriggerDefinition::evaluate` (already built, ADR-0001). A firing
trigger writes an `Event` (event_type = the key, e.g. `"sentiment"`) to ClickHouse and publishes
`event.created`.

This is a placeholder classification scheme, not the final design — it directly ties trigger
matching to whatever field names an AI/ML model happens to return, which is brittle and
non-obvious to operators. When Config/Admin Service ships `EventTypeDefinition` management, this
scheme is replaced by looking up the tenant's configured classification rules instead of
inferring from JSON keys; that is a new ADR at that point, not a silent behavior change.

## Consequences

- Easier: Trigger Engine is buildable and testable today without a stub or fake Config/Admin
  Service API; every downstream piece (window aggregation, `TriggerDefinition::evaluate`, Event
  writes, `event.created` publish) can be built and proven against real infra now.
- Harder: operators cannot yet define custom classification logic (e.g. "a sentiment below -0.5
  AND containing the word 'refund'") — event types are exactly the AI/ML model's own output
  field names, which is confusing if a tenant's Foundry deployment changes its output shape.
  This is the deliberate, temporary cost of not blocking Trigger Engine on Config/Admin Service;
  it is why this ADR exists instead of just shipping the behavior undocumented.
