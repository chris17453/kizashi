# 0084. Events page links directly to each event's contributing record journey

## Context

The record→event→action lineage view (`/data/:id/journey`, ADR-0017) was only reachable by
already knowing a record's UUID — through Data Viewer search → record detail → "View record
journey". An investigator starting from the *other* end of the pipeline — spotting an anomalous
event on the Events page — had no way to trace it back to the record(s) that caused it without
separately searching Data Viewer and hoping to find the right one. This made the Events page a
dead end for exactly the investigative workflow the journey view exists for.

The backend already had everything needed: `common::Event::record_ids` (the record ids whose
analyzed signals satisfied the trigger that produced the event) has existed since ADR-0017, and
`dashboard-api`'s `GET /v1/events` already serializes the full `Event` struct, `record_ids`
included. The UI's `EventSummary` simply never deserialized that field — a pure UI-wiring gap,
identical in shape to ADR-0082's Data Viewer filters (backend already there, UI never wired it
up), not a new backend capability.

## Decision

`EventSummary` gains `record_ids: Vec<Uuid>` (`#[serde(default)]` for events predating the
field). The Events page's table gets a new trailing column: a single "View journey" link when an
event has exactly one contributing record (the common case — a threshold/count trigger), numbered
"Journey 1", "Journey 2", etc. links when a correlated trigger produced an event from multiple
records, or a plain dash when there are none (older events, or a trigger type that doesn't
populate the field). The Overview dashboard's compact "Recent Activity" preview is deliberately
left unchanged — it's already documented as "a glance, not a replacement for the full paginated
Events page," and adding investigative deep-links there would blur that distinction rather than
reinforce it.

## Consequences

- Purely additive: no backend change, no new dependency, no behavior change to existing fields.
- An event whose `record_ids` is empty (a stale event from before this field existed, or a
  future trigger type that doesn't populate it) shows a dash rather than a broken or misleading
  link — verified by a dedicated test, not just the happy path.
- This closes the concrete half of the "Record Journey has no standalone search entry point"
  finding from the fourth audit pass: the journey view is now reachable from both ends of the
  pipeline (record search, and event browsing), not just one.
