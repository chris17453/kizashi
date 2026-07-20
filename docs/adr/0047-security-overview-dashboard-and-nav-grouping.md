# 0047. Security overview dashboard and navigation grouping

## Context

Three sessions' worth of security/compliance work (ADR-0043 through ADR-0046) added a growing
set of individually useful pages — Audit Log, Active Sessions, Users, Retention Policies, Egress
Allowlist — but nothing tied them together. A new admin or an auditor arriving at the Console UI
had no single starting point to answer "what is this workspace's overall security posture,"
only five separate pages to check one at a time with no indication of which to look at first.

Separately, the nav itself had grown to 26 flat links with no structure, itself a "level 2, not
level 10" UI smell flagged as a known follow-up in ADR-0045.

## Decision

Add `GET /security`, a dashboard that aggregates data every one of the five pages above already
exposes individually via their existing clients (no new backend endpoints): active session count,
admin actions in the last 7 days (approximated via each audit source's `list_recent`, capped at
200 entries per source — there's no dedicated count endpoint), RBAC role distribution (admin/
operator/viewer counts from `UsersClient`), retention policy coverage (enabled vs. total), and
egress allowlist size (explicitly flagging zero domains configured as a warning, since an empty
allowlist typically means "no restriction," not "nothing configured yet," in this platform's
model). Every metric links out to its own detail page.

Also reorganizes the nav into four labelled sections (Data & Pipeline, Configuration, Security &
Compliance, Platform) via a new `.nav-section` heading style, with "Security Overview" as the
first entry in the Security & Compliance group — giving that whole area of the product a visible
identity instead of being scattered flat alongside data-pipeline and config pages.

## Consequences

- The "admin actions in the last 7 days" figure is an approximation bounded by
  `RECENT_ACTIVITY_LOOKBACK_LIMIT` (200) per audit source — a tenant with more than 200 changes to
  a single service in 7 days will undercount. Acceptable for a dashboard tile; the Audit Log page
  itself (unbounded via pagination) remains the source of truth for exact history, and the
  dashboard says "in the last 7 days" rather than implying a total.
- No new backend calls or schema — this page is pure aggregation of existing clients, so it can't
  drift from what each detail page shows (same data, same clients, just summarized).
- The nav's section-grouping pattern (a `.nav-section` heading between groups) is now the
  established structure — any future page addition should land in the right existing group rather
  than being appended flat at the end, which is what created this problem in the first place.
