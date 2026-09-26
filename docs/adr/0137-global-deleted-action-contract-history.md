# ADR-0137: Global Deleted Action Contract History

- Status: Accepted
- Date: 2026-07-25

## Context

The Ontology workbench previously loaded governed action history only for action
types that still existed. Deleting a contract therefore removed its definition
from the workbench and made its immutable history undiscoverable, weakening the
audit trail.

## Decision

Expose a tenant-scoped global action-type history resource through the ontology
service and Query Gateway. The Console groups that history by action-type ID,
derives deleted contract names from the retained snapshots, and renders deleted
contracts alongside active contracts.

## Consequences

Deleted governed action contracts remain visible with their create, update, and
delete events and before/after state. The read model requires no restoration of
deleted definitions, while the history endpoint returns only the authenticated
tenant's records.
