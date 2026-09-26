# ADR-0184: Bounded cursor walking for filtered audit API pages

## Status

Accepted — 2026-07-25

## Context

The versioned audit feed API accepted search, service, change-type, and day
filters, but initially fetched only one merged backend page before applying
those filters. A matching immutable event just beyond that page could
therefore be omitted even though the browser feed and CSV export continued
walking cursor pages.

## Decision

When any audit filter is active, `GET /api/v1/audit-log` walks up to ten
cursor pages, applies the filter to each merged page, and returns a bounded
result with a continuation cursor when the budget or result limit is reached.
Unfiltered requests retain the single-page latency contract. The ten-page
bound protects tenant and service dependencies from unbounded API work while
making filtered results materially more complete and consistent with the
operator UI and CSV export.

## Consequences

API consumers can reliably search beyond the newest audit page and continue
through large histories with `next_before`. A heavily filtered request may
still require another request after the bounded walk, which is surfaced by
`has_more` rather than silently truncating the evidence set.
