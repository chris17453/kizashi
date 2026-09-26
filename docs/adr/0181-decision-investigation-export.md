# ADR-0181: Decision investigation export

- Status: Accepted
- Date: 2026-07-25

## Context

Action detail is the operator’s canonical view of an immutable governed
decision, including its contract snapshot, parameters, target entities, source
evidence, review posture, and retry boundary. The authenticated API already
exposes that bounded context at `/api/v1/actions/:id/360`, but the page had no
portable handoff control.

## Decision

Add a one-click decision-investigation JSON export to action detail. At click
time the browser requests the existing authenticated Action 360 contract with
no-store caching and downloads the server-owned response using a deterministic
filename.

## Consequences

- Operators can hand off a governed decision with its evidence and review
  context intact.
- Authorization, tenant scoping, immutable identity, and response bounds stay
  centralized in the existing API.
- The action outcome and review state are not mutated by export.
