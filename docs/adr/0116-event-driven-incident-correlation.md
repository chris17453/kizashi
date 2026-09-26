# ADR-0116: Event-Driven Incident Correlation

- **Status:** accepted
- **Date:** 2026-07-24

## Context

Incident links previously required an operator or the Console correlation sweep. Events already
carry a normalized `group_key`, so the platform can safely handle the narrow case where one
tenant has exactly one active incident with the same key.

## Decision

The Incident Service consumes the durable `event.created` exchange through a dedicated queue.
For each event it queries active incidents in the event's tenant with the same normalized key:

- exactly one match: link the event and persist the key with actor `event-correlation`;
- zero matches, blank keys, or multiple matches: acknowledge without changing an incident;
- repository failures: negatively acknowledge and requeue for retry.

The `incident_events.group_key` column preserves the correlation context for future inspection.
Existing links remain valid with an empty key, and the existing operator-assisted sweep remains
available for ambiguous cases. Partial duplicate-as-update behavior is not introduced here.

## Consequences

New events can join an unambiguous active case without a UI round trip, while ambiguous matches
remain governed and auditable. The service now requires RabbitMQ connectivity in deployments and
the migration must be applied before correlation context can be persisted.
