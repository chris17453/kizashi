# ADR-0030: Trigger dry-run test endpoint

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Spec §7's Console UI requirements imply a way to validate a trigger before trusting it in
production — an audit found no dry-run/simulate/preview affordance exists anywhere in the
`/triggers` page or `trigger-engine`'s API. Today the only way to find out whether a trigger is
configured correctly is to enable it and wait for real traffic to either fire it or not — for a
`CorrelatedOverWindow` trigger spanning multiple sources (ADR-0027), that feedback loop can be
long and the failure mode (a misconfigured `event_type` string, an unreachable `min_count`) is
silent: the trigger just never fires, with nothing telling the operator why.

`trigger-engine` already has everything needed to answer "would this trigger fire right now for
this entity?" without creating a real `Event`: `TriggerRepository::get_by_id` resolves the
trigger, and `evaluate_trigger` (extracted from `process_analyzed_record`) already runs the
correct evaluation path for all three condition shapes against `SignalRepository::window_stats`
— real, already-recorded signal history, not synthetic data.

## Decision

**`evaluate_trigger` becomes a public, reusable function** taking `&Arc<dyn SignalRepository>`
directly instead of the full `TriggerDeps` bundle — decoupling it from `event_store`/`publisher`,
which a dry run has no use for. `process_analyzed_record` is the only behavior change: it now
calls the same function with `&deps.signal_repository`.

**New `POST /v1/triggers/:id/test`** (`trigger-engine`'s API, alongside the existing
`GET /v1/triggers/:id`): given a tenant and a `group_key` (an entity to check, e.g. a customer
id), resolves the trigger and runs `evaluate_trigger` against that group_key's real,
already-recorded window stats. Returns whether it *would* fire right now, without writing an
`Event` or triggering any action — genuinely a dry run, not a stub that always returns a fixed
answer. **No `require_operator` gate** — reading whether a trigger would fire is not a write
path; anyone who can already view the trigger can test it.

**Console UI**: `/triggers` gains a small "Test" action per row — enter a `group_key`, see
"would fire: yes/no" plus the signal counts found, inline on the page. No new page.

## Consequences

- Easier: this is entirely additive — no schema change, no new table, reuses the exact
  evaluation logic that already runs in production (not a reimplementation that could drift
  from real behavior). An operator can now validate a `CorrelatedOverWindow` trigger's
  `event_type` spelling and `min_count` values against real history before trusting it live.
- Harder: the dry run only answers "would it fire *right now*, given signals already recorded"
  — it can't simulate "what if this new record arrived," since that would require synthesizing
  a fake signal rather than reading real history. That's an intentionally smaller, safer scope
  than a full trigger simulator; a "preview against a hypothetical record" mode is a distinct,
  larger feature deferred until real demand for it emerges.
