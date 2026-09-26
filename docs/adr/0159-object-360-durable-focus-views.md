# ADR-0159: Object 360 durable focus views

- Status: Accepted
- Date: 2026-07-25

## Context

Operators could save multi-entity comparison sets from the Ontology workbench,
but an Object 360 investigation had no direct way to preserve its current entity
as a named workspace view. Leaving the page meant recreating the focus later.

## Decision

Object 360 exposes a named “Save focus” form using the existing tenant-scoped
Ontology saved-view persistence. A saved view containing one object reopens the
canonical Object 360 route; multi-object saved sets retain their comparison
workspace behavior. The same authenticated saved-view mutation and naming
validation remain authoritative.

## Consequences

Single-entity investigations are durable and discoverable alongside existing
Ontology views, without creating a second persistence model. Existing two-to-six
entity comparison sets remain unchanged.
