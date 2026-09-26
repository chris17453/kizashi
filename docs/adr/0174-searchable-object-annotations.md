# ADR-0174: Searchable object annotations

## Status

Accepted

## Context

Object annotations provide durable investigation context, but limiting discovery
to Object 360 makes that context difficult to recover from the global command
surface. Operators frequently remember a note or handoff phrase rather than the
object identifier.

## Decision

Include a bounded tenant-scoped annotation category in global search and the
command palette. Search matches annotation body, author, and object ID, and
every result deep-links to the canonical Object 360 workspace while preserving
object investigation focus metadata in the palette.

## Consequences

Operator context is discoverable across the console without duplicating
annotation storage or exposing cross-tenant data. Results remain bounded and
reuse the ontology service’s existing read contract.
