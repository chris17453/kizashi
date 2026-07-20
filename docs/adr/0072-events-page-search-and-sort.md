# 0072. Events page search and sortable columns

## Context

A fresh gap audit found the Events page had pagination only — no search or column sorting,
unlike every comparable list page (Triggers, Users, Active Sessions, Login Attempts). `event_type`,
`group_key`, and `status` are all natural candidates an investigator would filter/sort by when
looking for a specific event or class of events.

## Decision

`GET /events` accepts `?q=` (case-insensitive substring match across `event_type`/`group_key`/
`status`) and `?sort=event_type|group_key|status|occurred_at` with `?dir=asc|desc`, same pattern
as Triggers (ADR-0066/ADR-0070). Since `list_events` is server-paginated, both only apply to the
*current page's* already-fetched events, not the tenant's full event history — the same accepted
limitation already documented for Triggers/Login Attempts/the global Audit Log. An unset `sort`
keeps the existing most-recent-first default. `q`/`sort`/`dir` all carry through the search form
and Previous/Next pagination links.

## Consequences

- An event that only appears on a later page won't be found by search until the operator pages
  to it — same accepted v1 limitation as every other paginated-list search on this platform.
- The events-over-time chart at the top of the page is unaffected by `q`/`sort` — it's an
  independent 30-day daily-count summary, not a view of the filtered/sorted table below it.
