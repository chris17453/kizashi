# 0073. Table header `scope="col"` attributes sitewide

## Context

A fresh accessibility audit found zero `<th scope="col">` usage anywhere across the Console
UI's 18 templates that render a `<table>` with column headers. Without `scope`, a screen reader
can't reliably associate a data cell with its column header on any list page (Users, Sessions,
Triggers, Events, API Keys, Sensors, and every other table-based page) — a cheap, systemic,
WCAG-relevant fix.

## Decision

Every plain `<th>` in every template was changed to `<th scope="col">` — a mechanical,
sitewide sed across all 17 templates that had one (`security_overview.html`'s three tables use
a label/value row shape with no header row at all, so it's correctly excluded). Sortable-column
header links (Users, Sessions, Triggers, Events) are unaffected structurally — the `<a>` toggle
link still lives inside the `<th scope="col">`, same as before.

Verified with build + the full test suite (417 tests, 0 broken by the markup change — nothing
asserted on the exact `<th>` string) plus new spot-check assertions on three representative
existing tests (Users, Triggers, Events) confirming `scope="col"` renders, rather than one new
test per template for a single sitewide markup convention.

## Consequences

- Purely additive markup; no behavior change.
- Any new table added later should follow the same convention — `<th scope="col">`, not bare
  `<th>` — though nothing currently enforces this automatically (e.g. a lint or snapshot test);
  a future contributor could regress it on a new page without noticing.
