# ADR-0160: Command-palette saved investigations

- Status: Accepted
- Date: 2026-07-25

## Context

Named investigation views were durable, but the command palette only searched
live records and workspace entities. Operators had to return to the Ontology or
Overview surface to reopen a saved focus.

## Decision

When the command palette searches, it also queries the authenticated saved-view
API and presents matching views as `Saved view` results. Single-object Ontology
views route to Object 360, multi-object views route to comparison, and other
saved surfaces reopen their owning workspace. Single-object results carry the
shared investigation-focus metadata.

## Consequences

Durable investigations are discoverable through the same keyboard-first command
surface as live data. Saved-view search failure degrades independently, leaving
live search results available.
