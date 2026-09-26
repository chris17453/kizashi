# ADR-0148: Object 360 Investigation Timeline

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 exposed related objects, source signals, incidents, governed
decisions, and immutable model history as separate panels. Operators had to
reconstruct the order of an investigation across those panels.

## Decision

Extend the tenant-scoped Object 360 read model with a bounded timeline that
combines model-history entries, lineage-backed signals, linked cases, and
governed decisions. Each entry carries a typed kind, title, detail, timestamp,
and optional deep link. The API sorts newest first and caps the result at 100
entries; the browser renders the same contract as a chronological investigation
rail.

## Consequences

Object investigations have one chronological context surface while retaining
the specialized panels for detail. Timeline entries remain links to the
existing event, incident, and action workspaces, and the bounded read model
prevents an object page from becoming an unbounded activity export.
