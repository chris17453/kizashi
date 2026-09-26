# ADR-0175: Object 360 timeline filters

## Status

Accepted — 2026-07-25

## Context

Object 360 presents a bounded chronological rail combining source evidence, model
changes, signals, cases, governed decisions, and operator annotations. As the
read model becomes more useful, a long rail is harder to use during active
investigation when an operator needs to isolate one evidence class or find a
specific handoff.

## Decision

Add client-side kind, free-text, and inclusive date-window filtering to the
existing bounded Object 360 timeline. Filtering operates on the already
authenticated read model, retains the original deep links, reapplies after a
context refresh, and reports visible versus total entries. The API contract
remains unchanged and no additional backend query is introduced.

## Consequences

Operators can focus the investigation rail without losing context or issuing
extra requests. Because the filter is local to the bounded payload, it does not
change tenant isolation or pagination semantics; a future server-side timeline
query can be introduced independently if the read-model bound requires it.
