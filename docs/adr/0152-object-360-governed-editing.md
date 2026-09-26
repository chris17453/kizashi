# ADR-0152: Object 360 Governed Editing

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 provided a read-oriented investigation workspace while object
editing lived only in the broader Ontology page. Operators had to leave the
investigation context to correct a modeled property, even though the existing
update contract already enforced operator access, schema validation, actor
attribution, and immutable history.

## Decision

Add an operator-only Object 360 edit form that hydrates the current object
properties, object type, and source lineage from the versioned 360 read model.
Submit through the existing `/ontology/objects/:id/edit` path and accept a
strict same-object return route so a successful correction returns to Object
360. JSON validation remains enforced in the browser for feedback and by the
ontology service as the authoritative boundary.

## Consequences

Object investigation and correction now share one workspace while preserving
the established mutation and audit semantics. Viewers retain a read-only
Object 360 experience, and arbitrary redirects are not accepted by the update
handler.
