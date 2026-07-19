# ADR-0037: analysis-service health check reflects real consumer liveness

## Status

Accepted.

## Context

`analysis-service`'s `/healthz` returned a hardcoded `"ok"` string as long as the axum HTTP
server itself was up. This is what Docker's health check and container orchestration relied on
to decide the process was working.

In production against the real watkinslabs tenant, the `record.normalized` RabbitMQ consumer
stopped making progress (queue depth grew 384 → 520 → 563 messages, `rabbitmqctl list_queues`
showed 0 consumers) while `/healthz` kept reporting healthy the entire time. Nothing paged,
nothing restarted the container automatically, and the only reason it was caught was a human
noticing the backlog directly.

Root-caused to two structural issues in `crates/analysis-service/src/main.rs`:

1. The `record.normalized` consume loop ran inline in `main()` (not `tokio::spawn`ed), and its
   `tokio::select!` treated a `None` from the consumer stream (i.e. the AMQP consumer stream
   closing — connection drop, channel error, broker-side cancel) as `return`, silently ending
   the whole process with no log line beyond whatever Docker's restart policy produced
   downstream. `/healthz` never observed this because the healthz server runs in its own spawned
   task, independent of the consume loop's fate.
2. Even short of a full process exit, nothing distinguished "the consume loop is scheduling
   normally but the queue happens to be empty" from "the consume loop has wedged and stopped
   being polled by the tokio runtime at all." An HTTP-server-up check can't see that distinction
   by construction.

## Decision

- **`ConsumerHeartbeat`** (`crates/analysis-service/src/health.rs`): an `Arc`-shared
  `Mutex<Instant>` with `tick()` and `is_alive()`. The main consume loop calls `.tick()` on
  *every* iteration of its inner `tokio::select!` — both the "delivery received" branch and the
  `deadline` timeout branch (which fires unconditionally every `max_wait`, default 500ms, even
  when the queue is empty). This makes the heartbeat a genuine "the loop is still being
  scheduled" signal, not a "there happened to be a message" signal.
- `/healthz` returns `503 Service Unavailable` when `now - last_tick >= STALE_THRESHOLD` (30s —
  comfortably above the 500ms idle-timeout cadence, so normal idle periods never trip it) and
  `200 OK` otherwise.
- The `record.normalized` consume loop is now wrapped in an outer `'reconnect` loop: if the
  consumer stream ends (`None` from `consumer.next()`), the process logs the event, backs off
  1s, and re-establishes `basic_consume` on the existing channel rather than returning from
  `main()`. This closes the silent-process-death failure mode directly, independent of the
  health check — a stale heartbeat should now only ever be a transient signal during the
  reconnect backoff window, not a permanent one requiring an external restart.
- **Retry cap + dead-letter queue** (`crates/analysis-service/src/retry.rs`): live verification
  against the real deployed stack after the two fixes above showed the queue *still* wasn't
  draining (stuck at 501 messages, 1 consumer attached) — confirming the second root cause
  identified during the incident: pre-existing poison messages from long-dead test tenants
  (`AI/ML backend unreachable`, permanent failures) were being `nack(requeue: true)`'d forever
  with no retry limit, hot-looping ahead of the real tenant's messages in the same
  single-consumer FIFO queue. Rather than leave this as a follow-up, it was fixed in this same
  PR since it's the direct, live-confirmed cause of the backlog not draining. AMQP's native
  `nack(requeue)` doesn't expose an attempt counter, so retries are now tracked via a custom
  `x-analysis-retry-count` header: on failure, a message below `MAX_RETRIES` (5) is
  re-published to the same queue with the header incremented (`with_incremented_retry_count`)
  and the original delivery acked; at or above the cap, it's published instead to
  `analysis-service.record.normalized.dead` (a new durable queue, for operator inspection —
  never silently dropped, per the audit-trail principle in CLAUDE.md §5) and acked.

## Consequences

- Docker/orchestration health checks against `/healthz` now reflect actual consumer liveness,
  not just "the HTTP server thread happens to be scheduled."
- The reconnect loop trades "process exits, relies on restart policy" for "process stays up and
  self-heals," which is strictly better for a queue-draining service where restart-policy churn
  itself has a cost (reconnect storms, lost in-flight state).
- A permanently-failing tenant's messages now cap out at 5 attempts and land in a dedicated
  dead-letter queue instead of hot-looping forever — confirmed live: queue depth actually
  decreased (501 → 469 over ~20s) after redeploy, versus being stuck at 501 before this fix.
- **Explicitly out of scope for this ADR**: the `analysis_config.changed` consume loop still
  uses unbounded `nack(requeue: true)` — it's a low-volume, config-sync queue (not implicated in
  the observed incident), so the same retry-cap treatment is deferred rather than applied
  speculatively. No operator-facing UI yet exists for inspecting/replaying the new dead-letter
  queue — tracked as a follow-up alongside Sensor naming Stage 3 and the other backlog items.
