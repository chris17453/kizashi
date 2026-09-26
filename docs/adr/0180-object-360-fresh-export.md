# ADR-0180: Fresh Object 360 investigation export

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 already offered a browser export from its last hydrated view. That
could become stale during a long-running investigation, especially after an
operator refreshes or another actor changes the modeled context.

## Decision

Keep the existing Object 360 export control, but fetch the authenticated,
tenant-scoped `/api/v1/ontology/objects/:id/360` read model at click time with
no-store caching. Download the fresh bounded response as JSON using the
existing deterministic filename.

## Consequences

- Object handoffs include the latest relationships, evidence, annotations,
  timeline, governed decisions, and immutable history available to the API.
- The server remains authoritative for authorization, tenant scope, and bounds.
- Export failures are visible to the operator and do not mutate the workspace.
