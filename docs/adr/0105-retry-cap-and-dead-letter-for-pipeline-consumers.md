# ADR-0105: Retry cap and dead-letter for pipeline consumers

- **Status:** accepted
- **Date:** 2026-07-20

## Context

A fourteenth audit pass, checking RabbitMQ consumer error handling for consistency, found that
analysis-service is the only one of the four record-pipeline consumers
(`record.ingested → record.normalized → record.analyzed → event.created`) that caps retries and
dead-letters a permanently-failing message — its `retry.rs` module tracks a per-message
`x-analysis-retry-count` header and republishes to a dead-letter queue after `MAX_RETRIES = 5`
failed attempts, explicitly "so it can't starve other tenants' messages in the same
single-consumer FIFO queue."

`normalization-service`, `trigger-engine`, and `action-executor` had no such mechanism: on a
processing failure, each unconditionally `nack(requeue: true)`s with no cap. A permanently-failing
message — a downstream DB constraint violation, a record that always fails a trigger/action
lookup, a poisoned tenant config — gets redelivered and reprocessed forever, blocking the rest of
that tenant's (and, in a single shared queue, potentially other tenants') messages behind it
indefinitely. This is exactly the risk analysis-service's mechanism was built to prevent, just not
applied to its three structurally identical peers.

## Decision

Replicated analysis-service's `retry.rs` module (identical logic: `retry_count`,
`with_incremented_retry_count`, `should_dead_letter`, `MAX_RETRIES = 5`) into
`normalization-service`, `trigger-engine`, and `action-executor`, each with its own header name
(`x-normalization-retry-count`, `x-trigger-engine-retry-count`, `x-action-executor-retry-count`)
to avoid any cross-service header confusion, and each with its own dead-letter queue
(`<service>.<queue-name>.dead`), declared alongside the main queue at startup.

Each service's main processing-failure branch now checks `should_dead_letter` before deciding
whether to republish with an incremented retry header back onto the main queue, or republish
as-is onto the dead-letter queue — the same shape as analysis-service's existing branch. The
secondary config-sync consumers each service also runs (`mapping.changed` for
normalization-service, `trigger.changed` for trigger-engine) were left untouched — those already
have low, bounded cardinality (one message per config change, not per ingested record) and a
different risk profile, matching this session's practice of not applying a fix beyond its actual
scope.

## Consequences

- All four record-pipeline consumers now share the same poison-message containment: no queue can
  be starved forever by a single permanently-failing message.
- Every service's retry-cap/dead-letter logic is covered by the same 5 pure unit tests
  analysis-service's `retry_test.rs` already established (no existing precedent for a live
  RabbitMQ dead-letter integration test in this codebase — analysis-service itself has none — so
  none was added here either, to stay consistent with the reference implementation's own bar).
- Operators gain three new dead-letter queues to monitor
  (`normalization-service.record.ingested.dead`, `trigger-engine.record.analyzed.dead`,
  `action-executor.event.created.dead`) alongside the existing
  `analysis-service.record.normalized.dead` — no tooling to inspect/replay them exists yet in any
  of the four services; that's unchanged scope, not introduced by this PR.
