# ADR-0168: Relationship cardinality enforcement

## Status

Accepted

## Context

Relationship contracts stored a cardinality value, but the ontology write
boundary only checked endpoint object types. A `many-to-one`, `one-to-many`,
or `one-to-one` contract could therefore accept a second edge on its unique
side, leaving the Object 360 creation surface unable to give truthful
eligibility feedback.

## Decision

The ontology service will enforce cardinality on relationship creation and
update, excluding the current instance during an update. `many-to-one` keeps
the source endpoint unique, `one-to-many` keeps the target endpoint unique,
and `one-to-one` keeps both endpoints unique. Unknown cardinalities are
rejected. Object 360 will project the same constrained eligibility and reason
for its bounded creation options.

## Consequences

Relationship multiplicity is now authoritative at the service boundary rather
than a UI convention. Operators see unavailable contracts before submission,
while direct API callers receive a conflict for a valid contract whose unique
endpoint is already occupied.
