# 0045. A global, browsable audit log page

## Context

Every admin/config mutation across the platform already writes an immutable audit row
(CLAUDE.md §5), and the Console UI could already display one entity's history at
`/audit-log/:service/:entity_id` — but only if the viewer already knew which entity to look at.
There was no way to browse "everything that changed recently" without first navigating to a
specific trigger, mapping, retention policy, or user and clicking into its own history.

This is a real gap against the enterprise-compliance bar the rest of this session's security work
has been closing: a SOC2/ISO27001-style auditor's first ask is typically "show me a log of every
admin action in the last N days," not "let me look up one record I already know about." A
platform that can only answer the second question isn't compliance-ready no matter how immutable
its underlying audit rows are.

## Decision

Add a new endpoint, `GET /v1/audit-log` (distinct from the existing `GET /v1/audit-log/:entity_id`
— no entity id in the path), to each of the three services that own an audit log table
(config-admin-service, auth-service, retention-service). It lists the tenant's most recent audit
entries across every entity type that service owns, most-recent-first, with simple
timestamp-cursor pagination (`?limit=&before=`, default limit 50, capped at 200 to prevent an
unbounded query). This required a new `AuditLogReader::list_recent` trait method per service
(`ORDER BY changed_at DESC`, the opposite of `list_for_entity`'s ASC, since this is a "recent
activity" feed rather than one entity's history read top-to-bottom) — implemented against the
same three `_audit_log` Postgres tables already backing the entity-scoped endpoint, so no schema
change was needed. Each new route landed in the same `X-Internal-Secret`-protected router group
as its sibling entity-scoped route (ADR-0044), inheriting that gate automatically rather than
needing a fresh security review.

The Console UI gets one new page, `GET /audit-log`, that calls all three services' `list_recent`
via a small addition to the existing `AuditLogClient` trait (already shared by all three
`Arc<dyn AuditLogClient>` fields on `AppState` — one HTTP client implementation, three base URLs),
merges the three result sets in memory, sorts by `changed_at` descending, and truncates to the
page size. The "load older" link's cursor is the oldest `changed_at` shown on the current page,
re-queried against all three services independently — this means a service with disproportionately
more recent activity can temporarily crowd out the others on one page, but no entry is ever
skipped: continuing to page with a strictly-decreasing cursor eventually surfaces everything, the
same trade-off inherent to any fan-in-merge-then-cursor pagination scheme. A fully correct
compound cursor (service + timestamp) was judged not worth the added complexity for a v1 activity
feed used for browsing, not exhaustive export.

## Consequences

- Three services now each carry two audit-read endpoints with overlapping but distinct shapes
  (`list_for_entity` / `list_recent`) against the same table — any future service that adds its
  own audit log table should add both from the start, not just the entity-scoped one, to avoid
  reintroducing this gap.
- The nav gained one entry ("Audit Log") between Users and Platform Health — the Console UI now
  has 24 distinct pages, still organized as a flat list; if the nav keeps growing, grouping it
  into sections (Data, Configuration, Security & Compliance, Platform) is a natural follow-up, not
  addressed here.
- No new database migrations — this is entirely new read paths against existing immutable tables,
  consistent with "audit log is append-only" (CLAUDE.md §5): nothing about how entries are
  written changed.
- Pagination is timestamp-cursor only, not exhaustive-export-safe under concurrent writes at the
  cursor boundary (an entry written with the exact same `changed_at` as the cursor could
  theoretically be skipped). Acceptable for a v1 browsing UI; a true compliance-export feature
  (CSV/PDF report of a fixed date range, needed eventually per spec's stated audit/compliance
  ambitions) should not reuse this pagination scheme as-is and is out of scope here.
