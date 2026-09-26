# ADR-0122: Governed partial-duplicate incident correlation

## Status

Accepted

## Context

The incident correlation consumer previously handled only exact normalized group-key matches. A new signal for the same entity and event type, but with a new attempt or request group key, remained unlinked even when one active investigation clearly owned that entity. Automatically choosing among several cases would risk contaminating investigations.

## Decision

Persist event type and entity reference on every new incident-event link. Correlation evaluates in order:

1. An exact group-key match is authoritative. One active case receives an exact link; multiple active cases remain ambiguous.
2. Only when no exact match exists, one active case with the same tenant, event type, and entity reference may receive a partial link. The event remains linked with its new group key and is audited as `event-partial-correlation`.
3. Zero candidates are ignored; multiple partial candidates remain ambiguous and require operator review.

Existing links without identity fields are not retroactively inferred and cannot become partial-match candidates. This preserves a fail-closed boundary around historical data.

## Consequences

- Repeated signals for one modeled entity can enrich the existing investigation without requiring exact request keys.
- Ambiguous ownership never causes an implicit cross-case link.
- Correlation context storage grows slightly for new links and supports later evidence-provenance UI/API projection.
- Historical links require no risky backfill; operators can continue to link them explicitly.
- Incident evidence rows derive exact-key, partial-duplicate, and manual-link labels from the
  immutable incident-event audit entry, keeping operator-facing provenance on the same source of
  truth as the backend decision.
