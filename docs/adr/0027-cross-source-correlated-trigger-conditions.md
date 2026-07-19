# ADR-0027: Cross-source correlated trigger conditions

- **Status:** accepted
- **Date:** 2026-07-19

## Context

ADR-0001 scoped v1's `TriggerCondition` to two closed shapes (`CountOverWindow`,
`ThresholdOverWindow`), both evaluated against signals of a *single* `event_type` within a
`group_key`/window, and explicitly deferred compound conditions: *"If a future requirement
needs compound conditions (AND/OR across shapes, cross-field comparisons), that is a new ADR
extending this enum... tracked as a future ADR when a real use case demands it."*

That real use case has arrived: operators reading from multiple agents/connectors need triggers
that combine signals across data streams for the same entity — e.g. "fire when a customer has
a negative-sentiment email *and* an unresolved chat message within the same window," not just
within one source type. Today `TriggerDefinition.event_type_match` is a single string and
`TriggerRepository::active_triggers_for` looks a trigger up by exact match on that one string —
there is no way for one trigger to be found/evaluated across more than one `event_type`.

The existing building blocks are sufficient to extend rather than replace: `analyzed_signals`
already stores every classified signal keyed by `(tenant_id, event_type, group_key,
entity_ref)` regardless of source, so signals from an email connector and a chat connector
already land in the same queryable table under the same `group_key` — nothing new needed there.

## Decision

**New `TriggerCondition` variant**, additive to the existing enum (no change to the two
existing shapes or their evaluation):

```rust
CorrelatedOverWindow { conditions: Vec<CorrelatedCondition> }

pub struct CorrelatedCondition {
    pub event_type: String,
    pub min_count: u32,
}
```

Fires when *every* listed `event_type` has accumulated at least `min_count` signals for the
same `group_key` within the trigger's window — a closed, enumerable shape (like the two it
sits alongside), not an open-ended AND/OR/NOT expression tree. That broader compound-logic
case remains explicitly out of scope, deferred again exactly as ADR-0001 anticipated, until a
real use case needs OR-across-sources or negation rather than "all of these must be present."

**Trigger lookup** (`TriggerRepository::active_triggers_for`) is extended, not replaced: a
trigger is found for an incoming `event_type` if either `event_type_match` matches it directly
(the existing single-event-type path, unchanged), *or* the trigger's `condition` is a
`CorrelatedOverWindow` whose `conditions` list contains that `event_type` — checked via a
Postgres JSONB containment query (`condition -> 'conditions' @> '[{"event_type": "..."}]'`)
against the same `condition` column already stored as JSONB, no schema/table change. A
correlated trigger's `event_type_match` field is set to the first listed condition's event
type purely as a display/audit label; it plays no role in lookup for that shape.

**Evaluation**: `TriggerDefinition::evaluate(count, field_values)` is untouched — existing
callers, tests, and the fuzz test covering it are unaffected. A new
`TriggerDefinition::evaluate_correlated(&self, counts: &HashMap<String, u32>) -> bool` handles
only the new shape. `process_analyzed_record` branches: for a `CorrelatedOverWindow` trigger it
queries `SignalRepository::window_stats` once per listed `event_type` (not just the
newly-arrived candidate's own type), builds the counts map, and calls
`evaluate_correlated`; every other shape's evaluation path is unchanged.

**Console UI**: authoring a correlated trigger through `/triggers`' form is explicitly deferred
as follow-up — the API (`config-admin-service`'s existing `POST /v1/trigger-definitions`, which
already accepts arbitrary `TriggerCondition` JSON with no shape-specific validation) supports
it today. A dedicated multi-condition builder in the Console UI form is real, scoped follow-up
work, not a stub.

## Consequences

- Easier: no new table, no new sync mechanism, no change to the two existing condition shapes
  or `Event`/signal schema — the correlated shape rides entirely on infrastructure ADR-0006 and
  ADR-0018 already built. Adding a third shape later (e.g. sequence/ordering-aware correlation)
  is additive in the same way.
- Harder: a correlated trigger now needs `window_stats` called once per listed event type
  instead of once, so its evaluation cost scales with the number of correlated sources (bounded
  and small in practice — v1 doesn't limit `conditions.len()`, a follow-up concern if abuse
  becomes real). The JSONB containment lookup means `active_triggers_for` no longer has a
  single flat index to rely on for the correlated path; if this becomes a hot-path performance
  concern at scale, a normalized `trigger_event_types` join table is the natural follow-up,
  deferred until real load data justifies it (mirrors ADR-0001's own "don't build ahead of a
  real requirement" stance). Console UI authoring support is a known gap until its own
  follow-up lands, tracked here rather than silently deferred.
