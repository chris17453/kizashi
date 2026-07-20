# 0066. Triggers page search

## Context

A UI/UX audit found Triggers had pagination (`?page=`) but, unlike Users/API Keys/Active
Sessions/Login Attempts/Field Mappings, no search — a tenant with many triggers has no way to
find one by name without paging through the whole list.

## Decision

`GET /triggers` accepts `?q=` and filters on a case-insensitive substring match of the trigger's
`name`, same shape as ADR-0062's search. Unlike Users/API Keys, `list_triggers` is already
server-paginated (`limit`/`offset`), so this filter can only apply to the *current page's*
already-fetched triggers, not the tenant's full set — the same accepted "doesn't compose with
pagination in one request" limitation ADR-0063 documented for Login Attempts, not a full
server-side search. `q` is carried through the Previous/Next pagination links as a hidden field
so paging preserves the search term. Bookmarkable `GET` form, "Clear" link shown only when `q` is
non-empty, distinct "no results on this page" vs "no triggers configured" empty states.

## Consequences

- A trigger that exists on a later page won't be found by search unless the operator pages to
  it first — an accepted v1 limitation, not silently shipped as complete search. Worth
  revisiting with real server-side search if trigger counts grow large enough for this to bite
  in practice.
- Same pattern is the direct template for the remaining list pages still missing search
  (Sessions has search already; Analysis Configs and Normalization Mappings' sort/pagination are
  separate follow-ups).
