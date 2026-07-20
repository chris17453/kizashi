# 0069. Global Audit Log page search

## Context

A UI/UX audit found the global Audit Log page (`GET /audit-log`, ADR-0045) had cursor
pagination (`?before=`) but no search — an investigator has to scan a fully unbounded,
most-recent-first feed to find a specific actor's or entity's change, exactly the kind of
friction search was already added to fix on every other list page (ADR-0062). This is distinct
from `/audit-log/:service/:entity_id` (one record's own history), which is naturally small and
doesn't need search.

## Decision

`GET /audit-log` accepts `?q=` and filters on a case-insensitive substring match across
`actor`, `entity_type`, and `change_type` — whichever field the investigator remembers. Same as
Triggers (ADR-0066) and Login Attempts (ADR-0063), this page is already cursor-paginated, so the
filter only applies to the *currently fetched page*, not the tenant's full audit history — an
accepted, explicitly documented limitation, not full server-side search. The `next_before`
cursor for "Load older" is computed from the full fetched page *before* the search filter is
applied, so pagination keeps advancing through real history regardless of what's currently
displayed. `q` carries through the "Load older" link so paging preserves the search term. The
CSV export intentionally does not accept `q` — it's a compliance export of "everything from this
cursor forward," not a filtered download.

## Consequences

- An actor/entity that only appears on a later page won't be found by search until the
  investigator pages to it — same accepted v1 limitation as Triggers/Login Attempts.
- Distinct "no results on this page" vs. "no audit activity yet" empty states, matching every
  other search-enabled list page's convention.
