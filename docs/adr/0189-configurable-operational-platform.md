# ADR-0189: Configurable operational data platform

## Status

Accepted — 2026-07-25

## Context

Kizashi has a tenant-scoped ontology, event-driven services, PostgreSQL,
RabbitMQ, ClickHouse, MinIO, governed actions, and an operator console. These
capabilities are currently configured through separate technical surfaces. A
customer cannot yet model an operational domain, connect live or batch data,
transform it, safely write back to an external system of record, define review
workflows, and publish an application from one coherent platform.

The ontology's existing JSON schema supports basic JSON primitives, but it does
not provide the shared rich type vocabulary, provenance, source ownership, or
write policies needed for enterprise operational modeling. Kizashi must not
become the canonical inventory, ERP, billing, or ledger database by default.

## Decision

Evolve Kizashi into a configurable operational platform composed of five
governed planes:

1. **Model plane** — versioned entity, property, relationship, and action
   contracts using a shared extensible type system. Types include identity,
   numeric, temporal, monetary, media/blob reference, spatial, vector, nested,
   and entity-reference values. A type also declares constraints, display
   semantics, sensitivity, provenance, source ownership, and write policy.
2. **Data plane** — configurable external sources and data stores connected by
   batch, API, CDC, or stream contracts. External operational databases remain
   systems of record; Kizashi stores governed projections, evidence, and
   operational context.
3. **Pipeline plane** — versioned source-to-model flows composed of reusable
   extraction, transform, validation, matching, routing, and write-back steps.
   Each execution is asynchronous, idempotent, observable, retryable, and
   dead-letter recoverable.
4. **Workflow plane** — declarative triggers, review queues, SLAs, approvals,
   and governed commands. Writes use idempotency keys, optimistic concurrency,
   outbox/inbox delivery, and authoritative confirmation events before model
   projections are reconciled.
5. **Application plane** — generated and composable internal and client-facing
   applications built from typed forms, tables, detail views, uploads, queues,
   dashboards, and actions, bound to the same permissions and contracts.

The user-facing composition point is a unified Build Studio with Models, Data
Sources, Pipelines, Workflows, and Apps sections. The existing Ontology,
connector, action, and queue surfaces migrate into these explicit domains over
time; no invoice, OCR, inventory, or vendor workflow is hardcoded.

Documents are stored as typed, content-addressed references to S3-compatible
object storage, never arbitrary binary blobs in ontology JSON. OCR providers
such as PaddleOCR are pluggable pipeline processors running in independently
scalable workers. Their results retain page/region evidence and confidence so
low-confidence values can enter human review instead of silently mutating
business data.

## Consequences

The system gains a stable configuration contract that can support invoices,
credit, billing, AR/AP, inventory, and future domains without bespoke services.
It also adds substantial implementation work: schema migration/versioning,
connector credentials, durable execution state, idempotent command adapters,
object-storage access controls, a worker runtime, and a separate app-builder
surface. The initial implementation must stay backward-compatible with current
object schemas and mapping rules while each plane is introduced in vertical,
tested slices.
