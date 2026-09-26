# ADR-0126: Event lifecycle history API

## Status

Accepted

## Context

The Console event-detail view exposed immutable status transitions, and the larger event 360
response included them, but external clients lacked a focused endpoint for lifecycle history.

## Decision

Add `GET /api/v1/events/:id/status-history`. The request first verifies the event through the
caller's session or service-account query token, then delegates history to Query Gateway,
preserving its tenant boundary. A missing event is returned as `404`; upstream query failures are
returned as gateway errors.

## Consequences

- Automation can render or audit lifecycle transitions without fetching the full event 360 graph.
- The history remains read-only and append-only; event payloads and status mutation semantics are
  unchanged.
