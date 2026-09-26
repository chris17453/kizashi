# ADR-0140: Global Deleted Object-Type History

- Status: Accepted
- Date: 2026-07-25

## Context

Object-type history was previously loaded only for the currently selected
object type. Deleting a modeled type therefore removed its definition from the
Ontology audit, leaving object, relationship, and action contract histories
with inconsistent retention behavior.

## Decision

Expose tenant-scoped global object-type history through the ontology service,
Query Gateway, and `GET /api/v1/ontology/object-types/history`. The Console
groups immutable records by object-type ID, derives deleted names from retained
snapshots, and renders all active and deleted object-type contracts in a
dedicated audit panel. Selected-type history remains available as a focused
view.

## Consequences

The Ontology contract audit now covers all three definition families without
restoring deleted records. Reads remain tenant-bound, and existing per-type
history consumers continue to work unchanged.
