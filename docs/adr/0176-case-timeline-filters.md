# ADR-0176: Case timeline filters

## Status

Accepted — 2026-07-25

## Context

Incident detail already presents a chronological case rail combining linked
signals, lifecycle changes, governed responses, and operator findings. During a
busy investigation, operators need to isolate one source or time window without
leaving the case or losing the evidence handoffs.

## Decision

Add client-side source-kind, free-text, and inclusive start/end date filters to
the rendered case timeline. Filtering uses the existing authenticated page
payload, updates visible-versus-total status, and leaves each entry's existing
links intact. No new server query or mutation boundary is introduced.

## Consequences

Case reconstruction becomes faster while preserving the complete server-rendered
timeline and its audit semantics. The bounded local filter avoids additional
requests and remains tenant-scoped by construction; server-side filtering can be
introduced later if case timelines outgrow the current bounded read model.
