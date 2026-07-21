# ADR-0106: Dead letter queue visibility and replay

- **Status:** accepted
- **Date:** 2026-07-20

## Context

ADR-0105 gave all four record-pipeline consumers a dead-letter queue, closing the "requeue
forever" risk — but left a new gap: once a message lands in a dead-letter queue, there was zero
operator-facing way to see it existed or get it back into the pipeline once the underlying cause
was fixed. Live-checking against the running stack while building this confirmed the gap was
real, not hypothetical: `analysis-service`'s dead-letter queue already held 152 messages and
`action-executor`'s held 14, accumulated silently during this session's own testing, invisible
until this fix existed.

## Decision

Each of the four services gets two new operator endpoints, following retention-service's
`ops_handlers.rs` precedent exactly (internal-secret-gated, no `X-Tenant-Id`/`X-Role` — these are
service-to-service operational actions, not tenant-scoped reads, since a dead-letter queue mixes
messages from every tenant):

- `GET /v1/dead-letter` — `{"count": N}`, a cheap passive queue-declare, no message manipulation.
- `POST /v1/dead-letter/replay` — pops the oldest dead-lettered message, strips its retry-count
  header entirely (so it gets a full fresh `MAX_RETRIES` budget, not a partial one), republishes
  it onto the main queue, then acks it off the dead-letter queue. Returns `{"replayed": bool}` —
  `false` when the queue was empty. One message per call, deliberately: auto-replaying an entire
  backlog the moment an operator asks about it risks re-dead-lettering an unbounded batch
  immediately if the underlying cause isn't actually fixed yet.

New `dead_letter.rs` module per service (`DeadLetterManager` trait + `RabbitMqDeadLetterManager`
real impl using the existing lapin `Channel`, `InMemoryDeadLetterManager` test double) —
duplicated per service rather than shared, matching this session's established convention
(`retry.rs` is duplicated the same way). `analysis-service` and `normalization-service` didn't
have `INTERNAL_API_SECRET` wired at all before this (no prior HTTP API beyond `/healthz` needed
it) — both `docker-compose.yml` and `scripts/run-local.sh` gained the env var for them. The Helm
chart needed no change: its `deployment.yaml` already applies `INTERNAL_API_SECRET` to every
service via a shared `envFrom: secretRef`.

Console UI was deliberately NOT wired to this — matching retention-service's `/v1/sweep` and
`/v1/reimport`, which also have no Console UI page. These are ops-tool-triggered actions
(`curl`/a future small CLI), not tenant-facing admin UI.

Two bugs caught during live-RabbitMQ testing:
- The initial `basic_publish` calls in both the test helpers and `dead_letter.rs`'s own
  `replay_oldest` only awaited the outer "sent to broker" future and dropped the inner
  `PublisherConfirm` future lapin returns — the existing codebase convention (see
  `event_publisher.rs`) is to double-await both. Fixed in both the test helper and the real
  `replay_oldest` implementation (where the stakes are higher — acking the dead-letter message
  before the republish is durably confirmed risks losing it entirely on a crash between the two).
- Even with the publish confirmed, `count()`'s underlying passive `queue_declare` still lagged
  RabbitMQ's own internal queue statistics briefly and intermittently in the integration tests —
  a genuine eventual-consistency property of RabbitMQ's `message_count`, not a bug in this code.
  The test helper now polls (up to 20×50ms) instead of asserting immediately; confirmed stable
  across 3 consecutive full runs (36/36 passing) after the fix, versus intermittent failures
  before it.

## Consequences

- All four dead-letter queues now have both visibility and a recovery path — closes the gap
  ADR-0105 explicitly flagged as unaddressed follow-up.
- Live-verified against the real stack: `GET /v1/dead-letter` immediately surfaced the two
  real backlogs above; `POST /v1/dead-letter/replay` against `action-executor`'s queue correctly
  moved a message back through the pipeline, which failed 5 times against its actually-stale
  trigger reference and correctly landed back in the dead-letter queue — proving both the happy
  path and the "still broken" path work as designed.
- No message browsing/inspection endpoint exists yet (only a count and a blind replay-oldest) —
  an operator can't currently see *what* a dead-lettered message contains before replaying it.
  Accepted as v1 scope; a peek/list endpoint is a natural, separately-scoped follow-up if this
  proves insufficient in practice.
