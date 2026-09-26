# ADR-0177: Signal timeline filters

## Status

Accepted — 2026-07-25

## Context

Signal detail already shows a response waterfall and a chronological execution
table, but an event with multiple downstream stages forces operators to scan the
entire chain. The signal page is the canonical evidence context for deciding
whether to attach a case or invoke a governed response.

## Decision

Add client-side downstream-step and free-text filtering to both event timeline
representations. The same controls hide matching waterfall rows and table rows,
report visible-versus-total steps, preserve all existing handoff links, and use
only the authenticated page payload.

## Consequences

Operators can isolate source, normalization, analysis, triggering, action, or
case-handoff stages without leaving signal evidence. No backend query or tenant
boundary changes; the bounded timeline remains server-rendered and auditable.
