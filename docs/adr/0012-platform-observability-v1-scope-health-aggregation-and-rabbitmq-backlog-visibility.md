# ADR-0012: Platform Observability v1 scope: health aggregation and RabbitMQ backlog visibility

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §6 service #13 (Platform Observability) is specified in one line: "Metrics/health for all
services; pipeline backlog/lag visibility." Every existing service already exposes its own
`GET /healthz` (added by `scripts/new-service.sh`'s scaffold), but nothing aggregates those into
a single platform-wide view, and nothing surfaces the spec's other explicit requirement —
"pipeline backlog/lag" — which is the operationally important half of this service: knowing
whether ingested records are backing up somewhere between ingestion and action-execution.

The data-flow chain (spec §3) is `record.ingested` → `record.normalized` → `record.analyzed` →
`event.created`, all RabbitMQ exchanges (`common::bus`). RabbitMQ's management plugin (already
enabled in `docker-compose.yml`'s `rabbitmq:3-management-alpine` image, port 15672) exposes a
JSON HTTP API with per-queue message counts — the natural source for backlog/lag data without
building a second metrics pipeline.

Genuine per-service Prometheus-style request/latency instrumentation (`/metrics` endpoints) is
the other half of "Metrics/health for all services," but retrofitting it into all eleven
existing services is a cross-cutting change to every one of them, not something this crate can
deliver on its own — and CLAUDE.md's "no half-finished implementations" rule means this PR
should not add empty/stub `/metrics` endpoints to those services just to gesture at future work.

## Decision

`crates/observability` (matching the repo layout in CLAUDE.md §1, not `-service` suffixed) ships
two capabilities in v1:

1. **Health aggregation** — `GET /v1/health` fans out `GET /healthz` to every service in a
   configured registry (`SERVICE_REGISTRY` env var, `name=url` pairs) concurrently and returns
   each service's up/down status plus overall platform status, so "is everything up" is one
   call instead of eleven.
2. **Pipeline backlog visibility** — `GET /v1/backlog` reads queue depths from RabbitMQ's
   management HTTP API for the four pipeline exchanges' bound queues and returns them as a
   single ordered view of the ingest → normalize → analyze → act chain, so a growing backlog at
   any one stage is visible without opening the RabbitMQ management UI directly.

Per-service `/metrics` request/latency instrumentation is explicitly deferred, tracked as
follow-up work against each service individually (not a gap silently left here) — it needs a
real decision on what to measure and a shared `common` instrumentation helper, which is its own
scoped piece of work.

## Consequences

- Easier: both capabilities are built entirely from data already available (existing `/healthz`
  endpoints, RabbitMQ's already-enabled management API) — no new metrics storage, no new
  infrastructure dependency beyond an HTTP client, and both are genuinely useful to an operator
  today rather than placeholder scaffolding.
- Harder: `/v1/backlog` reports queue depth, not consumer processing latency — a queue can be
  momentarily deep because a burst just arrived, not because a consumer is stuck; distinguishing
  those needs latency instrumentation, which is exactly the deferred `/metrics` work. Until a
  service is added to `SERVICE_REGISTRY`, health aggregation won't know about it — this is
  operator configuration, not automatic service discovery, consistent with this platform's
  current deployment model (spec §10, docker-compose/Container Apps, no service mesh assumed).
