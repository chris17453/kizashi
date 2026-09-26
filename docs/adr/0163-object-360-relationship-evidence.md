# ADR-0163: Object 360 relationship evidence

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 displayed neighboring objects and relationship labels, but omitted
relationship-instance properties and the edge’s immutable history. Operators
could see that a connection existed without inspecting the evidence carried by
that edge or its governed changes.

## Decision

The Object 360 related-object projection now includes the relationship-instance
ID, properties, and a direct versioned history URL. The browser renders compact
relationship properties beside each neighbor and links to immutable edge
history, while preserving the existing Object 360 deep link for the neighbor.

## Consequences

Graph investigations retain edge-level context and auditability. The projection
remains bounded by the existing 50 related-object limit and does not add a new
mutation or persistence path.
