# ADR-0107: Event Detail View

- **Status:** accepted
- **Date:** 2026-07-20

## Context

The Events page (spec §7) only ever rendered a flat table row per event: type, group key,
status, occurred-at, and a link to a record's journey when exactly one record contributed. An
operator investigating a specific event — what its full payload was, which raw records fed it,
what actions fired because of it and when — had no page to land on. This gap was surfaced by
comparing Kizashi's investigation surface against Keep (github.com/keephq/keep), another AIOps
platform, whose alert detail view was the concrete reference point for what was missing.

The backend already had everything needed: `dashboard-api`'s `GET /v1/events/:id` handler
existed but was unused by any UI client, `query-gateway` already proxied it, and
`ExecutionClient::list_executions_for_event` / `IngestionStatsClient::get_record` already
existed for the record-journey page. No new backend endpoints were required.

## Decision

Add `GET /events/:id` to `kizashi-ui`: a new `EventsClient::get_event` client method calling
the existing proxied endpoint, and a new `event_detail_handler.rs` + `event_detail.html` page
showing:

- Event metadata (status, group key, entity, occurred/recorded at, source connectors) and the
  full JSON payload, pretty-printed.
- A chronological timeline merging the event-fired moment with every action execution
  triggered by it, each annotated with latency from the event firing and a pass/fail indicator.
- The contributing raw records (resolved via `record_ids` on the event), linking to each
  record's detail and journey pages.

The Events table's event-type cell now links to this page. The page is read-only, session-
gated like every other Console page, and reuses the existing `require_session` /
`format_latency` conventions already established by `record_journey_handler.rs` rather than
introducing new shared abstractions for a single new page.

## Consequences

Operators get a single landing page for "what happened because of this event" instead of
having to cross-reference the Events table, Data table, and individual record journeys by
hand. No new backend surface area, audit logging, or tenant-isolation concerns are introduced
since the page is a pure read composed from already-tenant-scoped, already-tested client calls.
Follow-up UI investigation-tooling ideas from the same Keep comparison (a real Pipeline
topology graph, bulk actions on the Events table) are deliberately out of scope for this ADR
and remain open backlog items.
