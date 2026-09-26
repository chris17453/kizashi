# ADR-0191: Data Source and Pipeline definition contracts

## Status

Accepted — 2026-07-25

## Context

ADR-0189 requires customers to define external systems and reusable data flows
without turning individual connector configuration into the product's only
composition mechanism. Existing Sensors remain deployment-level connector
registrations with opaque connector-specific configuration. They are not a
durable, reusable model of an authoritative system, a data-access policy, or a
versioned pipeline.

The platform needs one shared contract before a control-plane repository, Build
Studio, adapter runtime, or workflow can safely depend on it.

## Decision

Introduce shared `DataSource` and `PipelineDefinition` contracts in `common`.

A Data Source has a tenant, name, description, transport kind (`database`,
`api`, `stream`, `cdc`, `upload`, or `batch`), and explicit access mode
(`read`, `projection`, or `command`). Its `connection` JSON contains only
non-secret transport metadata; any password, secret, token, or API key is
rejected, and a secret-manager `credential_ref` is used instead. Command-mode
sources require a credential reference.

A Pipeline Definition is versioned and points to one Data Source plus an
optional target ontology object type. Its initial step vocabulary is bounded to
`extract`, `transform`, `validate`, `match`, `route`, and `write_back` with
object-shaped configuration. Inline `script` fields are rejected: transforms
stay declarative until a constrained, sandboxed script runtime is separately
designed. Projection pipelines cannot write back; command pipelines must
include an explicit `write_back` step.

## Consequences

The platform now has stable reusable contracts for the data and pipeline
planes, distinct from Sensors. The next slice persists these tenant-scoped
definitions with immutable configuration audit history, then adds execution
state, idempotency, retries, outbox/inbox delivery, and authoritative
confirmation reconciliation. The contracts alone do not connect to external
systems or permit writes.
