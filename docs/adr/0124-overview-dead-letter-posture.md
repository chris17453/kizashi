# ADR-0124: Overview dead-letter posture

## Status

Accepted

## Context

Dead-letter queues were visible in Action Center, but the landing dashboard did not include their
pressure in the operator attention posture. This made pipeline recovery discoverable only after
opening a separate surface.

## Decision

The overview reads the existing execution-client queue summaries and adds the total known
dead-letter message count to its attention rail. Queues with an unknown count but confirmed
messages contribute one item, preserving an actionable signal without inventing precision. The
overview links directly to Action Center's pipeline recovery section.

## Consequences

- Operators see recovery pressure alongside incidents, SLA breaches, and stale connectors.
- Execution-service failures degrade to zero for this KPI and are surfaced through the overview's
  existing backend-error disclosure.
- No new queue or tenant data path is introduced; replay and inspection remain in Action Center.
