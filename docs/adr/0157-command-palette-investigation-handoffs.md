# ADR-0157: Command palette investigation handoffs

- Status: Accepted
- Date: 2026-07-25

## Context

The command palette searches live workspace data and creates result links after
the shared shell has rendered. Static entity links already use the canonical
Object 360 workspace and persist investigation focus, but dynamically created
entity results still used the legacy Ontology URL and did not update the focus
rail.

## Decision

Live command-palette results for modeled entities route directly to Object 360.
Entity, case, signal, and decision results also carry the shared investigation
context metadata so selecting them updates the shell focus and recent-history
rail. Other result categories retain their existing owning workspace routes.

## Consequences

Search-driven investigations now preserve the same canonical context as static
handoffs. The change is client-side and does not alter search or persistence
contracts.
