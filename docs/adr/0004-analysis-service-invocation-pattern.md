# ADR-0004: Analysis service invocation pattern

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §11 flags "Analysis Service invocation pattern: synchronous per-record calls to
Foundry/ML vs. batched invocation" as a sprint-0 open item. Spec §3's data flow has
Normalization Service publish `record.normalized` to RabbitMQ and Analysis Service consume it,
call Azure AI Foundry/Azure ML, and publish `record.analyzed` — spec §2 principle 4 requires
every stage to be "independent, asynchronously connected" and separately scalable/retryable,
which already rules out a synchronous request/response coupling between Normalization and
Analysis.

The open question is narrower than the ADR title suggests: given the bus already decouples the
stages, should Analysis Service call Foundry/ML once per `record.normalized` message as it
arrives, or accumulate messages into batches before calling? Per-record calls are simpler and
give the lowest per-record latency, but most AI/ML inference APIs (Foundry included) charge and
rate-limit per call, and batching amortizes both cost and rate-limit pressure — material for an
enterprise platform expected to run at real ingestion volume across many tenants.

## Decision

Analysis Service consumes `record.normalized` messages and invokes Foundry/ML in **micro-
batches**: it accumulates consumed records into a batch bounded by *either* a max batch size or
a max wait time (whichever is reached first — e.g. up to N records or T milliseconds), then
issues one batched inference call per batch, fanning the results back out and publishing one
`record.analyzed` per input record. This is an invocation-pattern detail internal to Analysis
Service, not part of the bus contract — `record.normalized` and `record.analyzed` stay
one-message-per-record regardless of how Analysis Service batches its own Foundry/ML calls.

Batch size and max wait are per-tenant-configurable (config-over-code, spec §2 principle 5),
since latency/cost tradeoffs will differ by tenant SLA. A batch never mixes records from
different tenants in a single Foundry/ML call, preserving tenant isolation (spec §8) even
inside the batching layer.

## Consequences

- Easier: fewer Foundry/ML API calls under load reduces both cost and exposure to per-call
  rate limits; the batching window is a single tunable per tenant rather than a
  per-connector or per-event-type setting to reason about.
- Harder: per-record latency through Analysis Service is bounded by the batch wait window, not
  zero — a record can wait up to `max_wait` before its batch flushes. This is an explicit,
  configurable tradeoff (small `max_wait` approximates per-record calls when a tenant's SLA
  demands it), not a hidden cost.
