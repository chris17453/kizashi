# ADR-0017: Event record lineage — `record_ids` field on Event

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Kizashi's data model has always had a well-defined pipeline (spec §3): ingest → normalize →
analyze → aggregate/trigger → act. Every hop except one was already traceable end-to-end:
`RawRecord.normalized_payload` carries the normalization result on the same row (no lookup
needed), `AnalyzedRecord` wraps a `RawRecord` directly, and `ActionExecution.event_id` is a
hard foreign key back to the `Event` that caused it (`crates/action-executor/migrations/
0001_create_action_executions.sql`). The one missing hop: `Event` carried no reference back to
the `RawRecord`(s) whose analyzed signals actually satisfied the `TriggerDefinition` condition
that produced it. `crates/trigger_engine::process_analyzed_record` computes `window_stats`
(count and numeric values within the trigger's window) to evaluate the condition, but discarded
which specific signals — and therefore which records — contributed, before constructing the
`Event`.

This is a real gap against the platform's own stated purpose: an operator investigating "why
did this event fire" or "what caused this alert" has no way to trace it back to the source
data. It is also a blocker for any future investigative/link-analysis UI (the Console UI's
direction per this session's standing goal) — you cannot build a "record journey" or
relationship view without the underlying data actually carrying the relationship.

## Decision

`common::Event` gains `record_ids: Vec<Uuid>` — the `RawRecord` ids whose `AnalyzedSignal`s
were counted when the firing `TriggerDefinition`'s condition was evaluated true.
`SignalRepository::window_stats` returns `(count, values)` extended to
`(count, values, record_ids)` — the same underlying `analyzed_signals` query already scans
every matching row for aggregation, so returning `record_id` alongside `numeric_value` is free
(no new query, no new round trip). `process_analyzed_record` attaches them via a builder,
`Event::new(...).with_record_ids(window_record_ids)`, keeping `Event::new`'s existing signature
and every other call site (tests, other services) unaffected. The ClickHouse `events` table
gains a matching `record_ids Array(UUID)` column; `#[serde(default)]` on the field means older
serialized Events (from before this field existed, or a query against a not-yet-migrated
ClickHouse instance) deserialize to an empty list rather than failing.

Both directions are covered by real-infrastructure tests: `signal_repository`'s
`window_stats` test asserts the returned record ids match the in-window signals;
`process_analyzed_record` tests assert both a single-record threshold-trigger fire and a
multi-record count-over-window fire produce an `Event` carrying the correct record id(s);
`event_created_contract_test` asserts `record_ids` round-trips through the wire message; a new
`dashboard-api` integration test against real ClickHouse proves the column round-trips through
insert → `ClickHouseEventQueryRepository::get_event`/`list_events`.

## Consequences

- Easier: the record→event hop is now a stored fact, not something that would need heuristic
  reconstruction from time-windowed signal queries (which would give wrong answers once
  signals are retained/deleted on a different schedule than events, or windows overlap). This
  unblocks the Console UI's planned record-journey/link-analysis view without any further
  backend work — `GET /data/:id` (already exists) plus `GET /v1/events/:id` (already exists,
  now returns `record_ids`) is enough to trace a record to the event(s) it produced.
- Existing ClickHouse deployments need a one-time `ALTER TABLE events ADD COLUMN IF NOT EXISTS
  record_ids Array(UUID)` — `CREATE TABLE IF NOT EXISTS` in `ensure_schema()` is a no-op
  against an already-existing table, so this is not automatic for a running environment (only
  for a fresh one). Applied directly against this build's dev ClickHouse instance as part of
  this change; a production rollout runbook would need the same step called out explicitly,
  not left implicit.
- `Event → ActionExecution` (via `event_id`) was already solid and needed no change — this ADR
  closes the only remaining gap in the chain, not a redesign of the whole lineage model.
- `record_ids` can be empty for a `ThresholdOverWindow` trigger whose window legitimately
  contains only the triggering record's own signal (still populated, length 1) or for any
  `Event` written before this field existed — callers must treat an empty list as "no lineage
  recorded," not "zero records involved," when reading historical data.
