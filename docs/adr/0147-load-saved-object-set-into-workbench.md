# ADR-0147: Load Saved Object Sets into the Ontology Workbench

- Status: Accepted
- Date: 2026-07-25

## Context

Saved ontology investigation sets reopened in side-by-side comparison, but
comparison was a read-only handoff. Operators had to select the same entities
again before using relationship actions or governed bulk updates.

## Decision

Add a comparison-workspace action that restores the compared live object IDs
to the browser's existing ontology selection state and returns to `/ontology`.
The action reuses the current bounded selection model and therefore makes the
saved set available to existing workbench actions without creating a second
selection store or widening mutation limits.

## Consequences

Saved sets become reusable operational workbench inputs as well as comparison
views. The handoff is client-local and preserves the existing tenant-scoped
saved-view persistence; objects that are no longer available are omitted from
the restored selection.
