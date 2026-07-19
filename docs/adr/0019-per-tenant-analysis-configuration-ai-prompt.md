# ADR-0019: Per-tenant analysis configuration (AI prompt)

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Task backlog item "AI prompt generation for agent actions" (`docs/features.md`, deferred at
the same time the Agent registry shipped) was scoped by re-reading the codebase: Analysis
Service (`crates/analysis-service/src/analysis_client.rs`) calls Azure AI Foundry/ML with one
fixed, global request shape for every tenant — `{"tenant_id": ..., "inputs": [...]}` — with no
operator control over what the model is asked to look for. There is no "AI Agent training"
surface anywhere in the system: the only tenant-facing configuration today is `TriggerDefinition`
(what counts as an event, evaluated mechanically against numeric fields the AI happens to
return) and `NormalizationMapping` (how raw payloads map to normalized fields). Nothing lets an
operator shape *what the AI itself analyzes for*.

This is a real product gap, not just a missing UI page: two tenants with identical connectors
ingesting identical data get identical AI analysis today, regardless of what each tenant
actually cares about (e.g. a support-ticket tenant wanting urgency/sentiment vs. a
compliance-log tenant wanting policy-violation flags).

## Decision

Add `AnalysisConfig { tenant_id, prompt, updated_at }` — one row per tenant, a free-text prompt
operators write describing what the AI should analyze for. `config-admin-service` owns CRUD
(`GET/PUT /v1/analysis-config`, operator-only write, audit-logged — same shape as every other
config entity in this service) and publishes an `analysis_config.changed` fanout message on
every write (new `ANALYSIS_CONFIG_CHANGED_EXCHANGE` in `crates/common/src/bus.rs`), reusing
ADR-0018's sync pattern exactly: `analysis-service` gains its **first-ever Postgres schema**
(it was previously stateless, pure pass-through) with a `PostgresAnalysisConfigRepository` and
a consumer that upserts on `analysis_config.changed`, then looks up the tenant's prompt before
every Foundry/ML batch call and includes it in the request body:
`{"tenant_id": ..., "prompt": <tenant's prompt or omitted>, "inputs": [...]}`.

Rejected: calling config-admin-service's HTTP API synchronously per batch — same reasoning as
ADR-0018, this runs in the hottest path in the system (every `record.normalized` batch) and a
local Postgres read stays fast at the standing scale target (thousands of inboxes/hundreds of
source APIs) where a cross-service HTTP round-trip per batch would not.

Scope for v1: one prompt per tenant, not per-connector/per-agent — matches the existing
tenant-level batching granularity from ADR-0004 (a batch never mixes tenants, so there's
already a natural per-tenant boundary to hang this on). Per-connector-type prompts are a
plausible v2 if operators need it, tracked as a follow-up, not built speculatively now.

The Console UI surface for this (a page where an operator writes/edits their tenant's prompt)
reuses the existing config-editing pattern already established for Triggers/API Keys — a
form + the same audit-log-visible-elsewhere convention — rather than inventing a new UI
paradigm.

## Consequences

- **First schema for analysis-service.** It goes from a pure stateless pass-through to owning
  a small Postgres table + running migrations — the same shape every other stateful service in
  this system already has, not a new pattern, but it does mean analysis-service now needs
  `DATABASE_URL` wired in `docker-compose.yml`/`scripts/run-local.sh`/`.env.example`, which it
  never needed before.
- **No prompt yet = today's exact behavior.** `AnalysisConfig` lookup returning nothing for a
  tenant simply omits `prompt` from the Foundry request body, so this is purely additive —
  every existing tenant keeps the current global analysis behavior until an operator opts in
  by writing a prompt.
- **Foundry request shape assumption.** This ADR assumes Foundry's endpoint accepts an optional
  `prompt` field alongside `inputs` and folds it into whatever inference it runs — the same
  assumption level as the original `{"inputs": [...]}` contract from ADR-0004, which was never
  validated against a real Foundry endpoint either (this system stub-tests that boundary, per
  CLAUDE.md's "no vendor lock-in" stance — Foundry itself is external and not owned by this
  repo). If the real Foundry contract differs, this is the seam to adjust, not the client
  trait's shape.
- **"Agentic" event surfacing is still not solved by this ADR alone.** This makes the AI's
  *input* configurable per tenant; `trigger-engine`'s classification of AI output into
  candidate events (`crates/trigger-engine/src/classify.rs`) remains mechanical (any numeric
  key becomes a candidate). Whether the AI's own output should ever *decide* trigger firing
  (rather than feeding numeric values into existing threshold/count conditions) is a larger,
  separate decision — flagged here as explicitly out of scope, not silently assumed solved.
