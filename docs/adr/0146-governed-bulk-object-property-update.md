# ADR-0146: Governed Bulk Object Property Update

- Status: Accepted
- Date: 2026-07-25

## Context

The Ontology workbench supported selecting entities for comparison,
relationships, and governed actions, but correcting the same modeled property
across a bounded set required opening each object individually.

## Decision

Add an operator-only bulk property update workflow for up to 25 selected
objects. The browser route and `POST /api/v1/ontology/objects/bulk-update`
accept one property and JSON value, merge it into each object's existing
properties, and call the normal object update contract for every object.

## Consequences

Every successful object change retains normal schema validation, actor
attribution, and immutable history. Partial failures are reported per object;
the bounded batch prevents an accidental workspace-wide mutation.
