# ADR-0187: Persistent ontology investigation workbench

## Status

Accepted — 2026-07-25

## Context

Search and Ontology could persist selected object IDs in browser storage and
route to comparison, but returning to the Ontology page showed only a count.
Selections from another filter or search page were effectively invisible,
making the “Load into workbench” action feel like a redirect rather than an
investigation workspace.

## Decision

Render a persistent, operator-visible Investigation Workbench on Ontology
whenever the browser-scoped entity set is non-empty. The panel lists visible
and off-page selections, links each entity directly to Object 360, provides a
bounded comparison action, and supports removing one entity or clearing the
set. It reuses the existing selection storage and bulk-action boundaries; no
new mutation or server-side state is introduced.

## Consequences

Operators can move between search, filters, comparison, and Object 360 without
losing sight of the active investigation set. Off-page IDs remain actionable
through canonical Object 360 links, while the existing bounded comparison and
bulk-operation limits continue to protect the workspace.
