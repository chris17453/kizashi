# ADR-0131: Incident 360 timeline read model

**Status:** Accepted  
**Date:** 2026-07-25

## Context

The Console incident detail page already presents a bounded investigation timeline combining
case creation, linked signals, immutable case activity, operator notes, and governed responses.
The versioned `GET /api/v1/incidents/:id/360` resource exposed those source collections separately,
forcing external operator clients to duplicate the Console's ordering and presentation logic.

## Decision

Add a `timeline` collection to the incident 360 response. Each entry includes its kind, actor,
timestamp, summary, detail, optional Console-relative link, and a failure posture. The collection
is tenant-scoped through the existing incident, query, execution, and audit clients, sorted newest
first, and bounded to 100 entries. The existing source collections remain available for clients
that need their native shapes.

## Consequences

External operator shells can render the same evidence-to-response chronology as the Console with a
single read. Timeline composition remains a read-model concern and does not create or persist a
second activity log. New source categories can be added additively while preserving the existing
360 contract.
