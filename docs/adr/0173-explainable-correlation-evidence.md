# ADR-0173: Explainable correlation evidence

## Status

Accepted

## Context

The incident correlation review correctly limited safe candidates to signals
whose normalized group key mapped to exactly one active case. The operator UI
showed the proposed target, but not the rationale that made the candidate safe,
and machine clients received the same incomplete explanation.

## Decision

Include a confidence posture and bounded evidence list on every safe correlation
candidate. The current deterministic candidate has `high` confidence because
the normalized group key matches exactly and there is exactly one active target
case. Ambiguous groups remain explicitly unresolved and do not receive a
guessed confidence.

## Consequences

Operators and integrations can inspect why a proposed link is safe before
applying it. The evidence is derived from the same server-side candidate set
that is revalidated at write time, so presentation cannot broaden correlation
authority or bypass the existing ambiguity boundary.
