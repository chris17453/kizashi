# ADR-0020: In-platform Agent Scheduler for connector polling

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Spec §3 describes connectors as "CronJob-scheduled pollers." In practice today, registering an
Agent in the Console UI (`POST /v1/agents`) only creates a config record — nothing in the
platform actually causes that Agent's connector to run on a schedule. The only path to a
connector actually polling is: an operator visits `/agents/generate`, gets a deploy script
(bash/PowerShell/`docker compose run`), and is responsible for wiring that into their own
cron/K8s CronJob/systemd timer outside the platform entirely. "Scale-out: dynamic per-agent
connector scheduling" (tracked task, split from a combined search+scheduling item) names this
gap directly: at the standing scale target (thousands of inboxes, hundreds of source APIs), a
platform that can't schedule its own ingestion is not actually operable — every one of those
Agents would need external, hand-wired scheduling infrastructure per tenant.

Each connector (`crates/connectors/{zendesk,graph-mail,graph-teams,sql,fabric,generic}`) is
already a fully isolated, env-var-configured binary sharing `connector-runtime`'s poll-cycle
logic (`poll_runner.rs`, ADR-0013) — one process, one poll cycle, then exit. This shape is
deliberately CronJob-like and this ADR keeps it that way rather than unifying connectors into
one long-running multi-tenant process, for the same isolation/blast-radius reasons ADR-0013
originally chose per-invocation env config over a shared daemon.

## Decision

Add a new service, `crates/agent-scheduler`, whose only job is: for every enabled `Agent`, on
that Agent's configured interval, invoke its connector binary with the right environment —
i.e., automate exactly what the deploy-script wizard's output does by hand today.

1. **`Agent.config` gains a `poll_interval_seconds` field** (already an opaque
   `serde_json::Value` per connector, config-over-code convention unchanged) — operator-set at
   register/edit time via the existing Agents UI, defaulting to a sane platform-wide value
   (e.g. 300s) when absent so existing Agents aren't silently left unscheduled.
2. **Agent Scheduler syncs its own copy of enabled Agents** the same way `trigger-engine` and
   `analysis-service` sync their configs (ADR-0018/ADR-0019 pattern): config-admin-service
   already writes `Agent` rows and has an audit-logged CRUD API; extend it to publish an
   `agent.changed` fanout message on create/update/delete, consumed here. Reused rather than a
   new pattern, and keeps Agent Scheduler's polling-decision loop off the hot request path of
   any other service.
3. **Invocation is process-per-poll, not a shared library call.** Agent Scheduler shells out
   to run each due connector exactly like the deploy script does — `docker run` (or, in a
   Kubernetes deployment, creating a one-shot `Job` from a template) with the same environment
   variables (`TENANT_ID`, `CONNECTOR_ID`, `INGESTION_GATEWAY_URL`,
   `INGESTION_GATEWAY_API_KEY`, plus the connector-specific fields already in `Agent.config`)
   already computed today by `ui/src/agent_script_handler.rs::build_scripts`. This keeps every
   connector crash/hang isolated to its own process/container, matching ADR-0013's isolation
   stance, and requires zero changes to any existing connector crate.
4. **A pluggable `Invoker` trait** abstracts *how* a due poll actually gets run, with two
   implementations: `DockerInvoker` (runs `docker run --rm <image> ...` against the local
   Docker socket — the docker-compose deployment path) and `KubernetesJobInvoker` (creates a
   one-shot `batch/v1 Job` from a per-connector-type template — the K8s deployment path spec
   §10 describes as the eventual target). v1 ships `DockerInvoker` only; `KubernetesJobInvoker`
   is a documented follow-up, not built speculatively now.
5. **API key handling stays exactly as today**: Agent Scheduler does not mint or store API
   keys itself. Each Agent's `config` must already carry a reference to (not the plaintext of)
   an API key — in practice, the same key the deploy-script wizard mints and shows once. This
   ADR does not change API key lifecycle (ADR from Phase 1c); it only automates *invocation*,
   which already assumed a key existed.

Rejected: **Kubernetes CronJob objects as the only mechanism.** Would exactly match spec's
"CronJob-scheduled pollers" language and needs no long-running scheduler process at all, but
makes the docker-compose deployment path (spec §10's stated *first* deployment target) unable
to schedule anything — this platform must be operable via docker-compose alone. `Invoker`
abstraction lets both paths exist without picking one now.

Rejected: **Unifying connectors into one shared-library, in-process scheduler** (calling each
connector's `poll()` as a function call in Agent Scheduler's own process instead of spawning a
process). Removes the isolation ADR-0013 explicitly wanted, and turns any one connector's
panic/hang into an outage for every tenant's scheduling, not just that Agent's.

## Consequences

- **New always-on service** — every deployment (docker-compose, and later K8s) now runs one
  more process. It has no HTTP write surface of its own beyond `/healthz`; all state comes
  from the `agent.changed` sync, so it has no schema migrations of its own beyond a small
  local `agents` mirror table (same shape as `trigger_engine.trigger_definitions`).
- **Docker socket access is a real operational/security surface.** `DockerInvoker` needs the
  Docker socket mounted into the scheduler's container — broader host access than any other
  service in this platform currently has. This must be called out explicitly in deployment
  docs, not silently assumed; a rootless/sidecar Docker access pattern is worth revisiting if
  this becomes a compliance concern for a customer.
- **Poll cadence is best-effort, not exact-cron.** Agent Scheduler's own tick loop (e.g. every
  10–30s, checking which Agents are due) means actual invocation can lag an Agent's configured
  interval by up to one tick — acceptable for a poller, not acceptable if a future feature ever
  needs cron-precision scheduling (flagged, not solved, here).
- **First deployment of `KubernetesJobInvoker` is unscoped** — this ADR documents the shape
  (`Invoker` trait, one impl per orchestrator) but only Phase 1 (`DockerInvoker`) ships with
  this decision. Do not assume K8s scheduling works until that follow-up lands.
