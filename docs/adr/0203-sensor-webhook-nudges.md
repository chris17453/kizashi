# ADR-0203: Webhook nudges for Sensors (platform capability)

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Kizashi Sensors are pulled on a schedule (Helm CronJobs or the agent-scheduler tick loop keyed on
`poll_interval_seconds`). Cursors live in `agents.last_checkpoint`, written from a
`KIZASHI_CHECKPOINT=` line on connector stdout. The ingestion gateway has no webhook route.

Yochō wants Graph change notifications and Zendesk trigger webhooks to cause an immediate pull,
with the scheduled pull still the source of truth.

## Decision

1. **Nudge, never payload.** A webhook only says "pull now". Its body is validated and then
   discarded; data always comes from the Sensor's delta/incremental pull. Missed webhooks are
   healed by the next scheduled pull.
2. **Route:** `POST /v1/sensors/{sensor_id}/nudge` on `ingestion-gateway`. Per-sensor shared
   secret or source-specific validation (Graph `validationToken` handshake and `clientState`;
   Zendesk webhook signature). Unauthenticated nudges are rejected and counted.
3. **Bus:** the gateway publishes `sensor.nudged {tenant_id, sensor_id, received_at}`. The
   scheduler consumes it and runs the Sensor now, coalescing: a nudge for a sensor that is
   already running or ran within a configurable debounce (default 30 s) is merged, not queued.
4. Nudges are rate-limited per sensor so a webhook storm cannot exceed source API limits
   (Zendesk incremental export: 10 req/min, 30 with High Volume).
5. Contract test for `sensor.nudged`, per CLAUDE.md §2.

## Consequences

- Built once in the platform; every Sensor can opt in.
- Latency from source change to ingest drops to seconds for nudge-capable sources; polling-only
  sources are unchanged.
- The gateway gets a public-facing route, so it needs the same egress/ingress hardening review as
  `/v1/ingest`.
