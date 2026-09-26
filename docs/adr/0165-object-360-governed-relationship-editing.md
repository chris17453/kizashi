# ADR-0165: Object 360 governed relationship editing

## Status

Accepted

## Context

Object 360 exposed relationship-instance properties and immutable history, but
operators had to return to the global Ontology workspace to correct an edge.
That broke investigation focus and left relationship property contracts less
visible at the point of mutation.

## Decision

Each related-object card in Object 360 will expose an operator-only edit form
for the current relationship instance. The form reuses the existing governed
relationship update endpoint, preserves the tenant-scoped source and target
endpoints, displays the relationship property contract, and validates the
submitted JSON object in the browser before submission. The ontology service
also validates declared relationship properties at the write boundary and
continues recording immutable link history for every update.

## Consequences

Operators can correct relationship evidence without leaving Object 360. The
same actor attribution, endpoint/type validation, and immutable history used by
the global Ontology workspace remain authoritative. Relationship schemas now
apply to instance writes as well as object-facing presentation.
