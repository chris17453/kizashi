# ADR-0164: Object 360 inline relationship history

## Status

Accepted

## Context

Object 360 already showed a related modeled object, relationship-instance
properties, and a link to the immutable relationship-history endpoint. The raw
endpoint handoff was useful for API consumers but forced browser operators to
leave the active investigation to inspect the evidence behind an edge.

## Decision

The bounded Object 360 read model will include immutable history for each
returned relationship instance. The browser will render that history as a
collapsed evidence disclosure on the related-object card while retaining the
direct API handoff for external tooling and deeper inspection.

History retrieval is tenant-scoped and failure-tolerant: if the optional
history projection is unavailable, the related object and its properties still
render and the card reports that edge history is unavailable.

## Consequences

Operators can inspect relationship properties and before/after edge snapshots
without losing Object 360 context. The response remains bounded by the
existing related-edge limit and does not change mutation or audit semantics.
External clients receive the same evidence in one read model, avoiding a
per-edge browser join.
