# ADR-0192: Pipeline Definition control plane

## Status

Accepted — 2026-07-25

## Context

ADR-0191 established shared Data Source and Pipeline Definition contracts, but
they were not yet usable as tenant configuration. A pipeline must not be able
to bind to a Data Source from another tenant merely because its UUID is known.
Configuration mutations also need the platform's existing immutable audit
record before execution infrastructure consumes them.

## Decision

Persist tenant-scoped Data Sources and Pipeline Definitions in Config Admin.
Expose authenticated CRUD endpoints at `/v1/data-sources` and
`/v1/pipeline-definitions`; reads require tenant identity and changes require
an Operator plus an actor name. Pipeline create and update look up the selected
Data Source by both tenant and ID before persistence.

Pipeline create, update, and delete run their entity change and an
`audit_log` entry in one transaction. Updates retain `created_at`, increment
the version, and set a server timestamp. API clients cannot choose the ID or
initial version of a new Pipeline Definition.

## Consequences

Pipeline configuration is reusable, tenant-isolated, versioned, and auditable.
This remains a control plane: no endpoint executes a pipeline, reads an
external system, or permits write-back. The next slice owns durable execution,
idempotency, retry, and reconciliation state.
