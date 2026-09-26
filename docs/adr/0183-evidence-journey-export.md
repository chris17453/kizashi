# ADR-0183: Evidence journey export

- Status: Accepted
- Date: 2026-07-25

## Context

Record Journey is the source-evidence anchor for the operating model. It
already traces a raw record into signals, cases, governed executions, and
modeled entities, and the authenticated API exposes the same bounded journey
at `/api/v1/data/records/:id/journey`. Operators need a portable handoff from
that starting point as well as from downstream investigations.

## Decision

Add a one-click evidence-journey JSON export to Record Journey. The browser
fetches the existing authenticated endpoint at click time with no-store
caching and downloads the server-owned lineage contract using a deterministic
filename.

## Consequences

- Evidence handoffs retain the raw record identity and downstream lineage.
- Tenant scoping, authorization, and bounds remain centralized in the API.
- Replay and modeling controls remain separate from the read-only export.
