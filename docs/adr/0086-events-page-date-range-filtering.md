# 0086. Events page date-range filtering

## Context

The fifth "data explorer" audit pass, continuing the explicit request for a fully functional
data explorer, found the Events page had no way to scope a search to a specific incident
window — an investigator chasing "what fired last Tuesday" had to page through the full,
most-recent-first event history by hand. `dashboard-api`'s `ListEventsQuery` (the backend behind
`GET /v1/events`) already accepts `since`/`until`; the Data Viewer already exposed the same
pattern for record ingestion dates (ADR-0082). The UI simply never forwarded these two fields for
Events.

## Decision

`EventsQuery` gains `from`/`to` (`YYYY-MM-DD`, matching `<input type="date">`), parsed via a
page-local `parse_date_range` helper — deliberately duplicated from `data_handler.rs`'s own copy
rather than shared, per this codebase's existing convention of small, page-local filter helpers.
`from` maps to start-of-day, `to` to end-of-day, both UTC. `EventsClient::list_events` gained
`since`/`until: Option<DateTime<Utc>>` parameters, forwarded by `HttpEventsClient` as query
params via reqwest's own `.query()` encoding (safe, unlike a template `href`). The two call sites
that don't need real filtering — the Overview dashboard's KPI tile and the Reports page's
event-type breakdown, both pulling up to 1000 most-recent events for an aggregate count — pass
`None, None`, preserving their existing unfiltered behavior.

Unlike `q`/`sort` (ADR-0062/ADR-0070), which only reorder/filter the *current fetched page* since
the backend has no substring-match or sort query of its own, `since`/`until` are forwarded to the
backend and scope the actual `list_events` call — the date range applies to the tenant's full
event history, not just the current page.

The search form gained `<input type="date">` fields for `from`/`to`, propagated as hidden inputs
through the sort-header links and Previous/Next pagination forms, matching the exact pattern
already used for `q`/`sort`/`dir` on this page.

## Consequences

- No backend change — `dashboard-api`'s `ListEventsQuery::since`/`until` already existed; this is
  purely additive UI wiring, the same "backend already supports it, UI never forwarded it" gap
  class found for Data Viewer's date-range/normalization filters (ADR-0082) and Events→Record
  Journey linking (ADR-0084).
- The chart's fixed 30-day window (`daily_counts`) is unaffected — it's a separate, always-on
  30-day summary regardless of the table's active date-range filter, same as before this change.
