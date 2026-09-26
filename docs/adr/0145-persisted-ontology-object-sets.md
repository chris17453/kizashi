# ADR-0145: Persisted Ontology Object Sets

- Status: Accepted
- Date: 2026-07-25

## Context

Ontology selection was useful for immediate comparison and governed actions,
but the selected IDs lived only in browser storage. An operator could not name
an investigation set, reopen it on another session, or share the same bounded
comparison through the existing saved-view API.

## Decision

Persist selected ontology IDs as `object_ids` in the existing tenant-scoped
saved-view filter. A named set with at least two IDs opens `/ontology/compare`
with a maximum of six objects, matching the compare surface's existing bound.
The Console adds a save-set control that tracks the current selection, while
the existing saved-view API exposes the stored filter to external clients.

## Consequences

Investigation sets become durable workspace artifacts without a second storage
model. They retain the existing tenant isolation and saved-view lifecycle, and
the six-object bound keeps comparison readable and predictable.
