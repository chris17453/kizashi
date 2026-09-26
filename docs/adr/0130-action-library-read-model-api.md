# ADR-0130: Action Library read-model API

## Status

Accepted

## Context

The Console Action Library combines governed action contracts with target eligibility, execution
outcomes, and contract history. The API exposed each underlying resource separately, forcing an
external operator console to reconstruct the same read model and repeat eligibility logic.

## Decision

Add `GET /api/v1/actions/library`. It returns filtered contract definitions, target counts,
eligible-target counts, execution posture, and contract history. It uses the authenticated
tenant's ontology client and keeps the existing lower-level action-type, invocation, and history
resources available for specialized clients.

## Consequences

- External command centers can render the same governed-action readiness view as the Console.
- Eligibility remains derived from current tenant-scoped ontology objects and action preconditions.
- The response is a bounded read model over existing resources; no new persistence path is added.
