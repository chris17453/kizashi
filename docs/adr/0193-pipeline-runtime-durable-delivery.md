# ADR-0193: Pipeline Runtime durable delivery

## Status

Accepted — 2026-07-25

## Context

Pipeline Definitions are configuration, not evidence that a pipeline ran or
that an authoritative system accepted a command. The platform requires
asynchronous, retryable, dead-letter recoverable execution plus idempotency,
outbox/inbox delivery, and source confirmation before reconciliation.

## Decision

Create a separate `pipeline-runtime` service with its own PostgreSQL schema.
Each execution snapshots a positive Pipeline Definition version and is
idempotent on `(tenant_id, pipeline_definition_id, idempotency_key)`. Creating
an execution writes a `pipeline.execution.queued` outbox record in the same
transaction.

Command dispatch moves an execution into `awaiting_confirmation` and records a
second outbox event. Authoritative confirmations use a tenant-scoped source
event ID inbox. A new inbox record atomically changes only an awaiting command
to `confirmed` and writes a confirmation outbox event; duplicate confirmation
events are acknowledged without a second state transition.

The HTTP boundary is internal-service-only and requires the existing shared
internal secret plus a tenant header. It accepts a configuration version
snapshot but does not itself resolve Pipeline Definitions; the future execution
worker/config client owns that read before submission.

The runtime leases unpublished outbox rows with `FOR UPDATE SKIP LOCKED` and a
short expiry, publishes them to the durable `pipeline.execution` topic, and
marks a row published only after broker confirmation. A crash can therefore
cause a duplicate delivery but cannot silently discard an event.

Confirmation reconciliation requires an authoritative source version and
evidence descriptor. The runtime applies only that confirmed projection and
opens a tenant-scoped workflow exception when reconciliation fails. Exception
cases are linked to the execution, receive a 24-hour SLA, and expose a
protected queue plus operator decision endpoint for approval, rejection, or
resolution.

## Consequences

The system has a durable coordination layer for projection and command
pipelines without treating a dispatch attempt as authoritative success. The
runtime now has bounded outbox retries/dead-letter handling, a concrete command
adapter boundary, confirmation reconciliation, and a durable workflow queue.
The next workflow work is richer review requirements and app-facing queue
composition.
