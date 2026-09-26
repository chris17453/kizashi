# ADR-0156: Object 360 relationship graph

- Status: Accepted
- Date: 2026-07-25

## Context

The Object 360 read model already returned a bounded set of immediate
relationships, but the workspace rendered those relationships only as a flat
list. Operators had to mentally reconstruct the local graph and leave the
investigation to use the richer Ontology graph.

## Decision

Render a bounded radial SVG graph in Object 360 from the existing related-object
read model. The current object is the center node, related objects are linked by
relationship labels, and each node deep-links to its own Object 360 workspace.
The existing relationship list remains below the graph as the accessible detail
and text fallback. A direct link opens the full Ontology graph at the same center
object.

## Consequences

Object investigation gains immediate spatial context without a new API or graph
store. The graph is intentionally limited to the read model's bounded immediate
neighborhood; multi-hop exploration and richer controls remain in the full
Ontology graph workspace.
