# ADR-0138: Versioned Global Action History API

- Status: Accepted
- Date: 2026-07-25

## Context

The Console's Ontology workbench needs global governed action history so that
deleted contracts remain auditable. The internal read path existed, but API
consumers could only retrieve history for a currently known action-type ID.

## Decision

Expose `GET /api/v1/ontology/action-types/history` through the authenticated
Console API. The handler uses the same tenant- and role-aware Query Gateway
client as the workbench and returns the ontology service's immutable history
records, including entries for deleted contracts. The existing per-contract
history endpoint remains available.

## Consequences

External operators and integrations can build the same complete audit view as
the Console without restoring deleted definitions or making unscoped backend
requests. The resource is read-only and preserves the existing API error and
tenant-boundary behavior.
