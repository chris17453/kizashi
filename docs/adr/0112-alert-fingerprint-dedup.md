# ADR-0112: Alert Fingerprint Dedup

- **Status:** accepted
- **Date:** 2026-07-20

## Context

The Keep-comparison research (which also produced ADR-0111's Incidents MVP and the Sensors
marketplace reskin) identified one more gap: Keep fingerprints every alert (a hash over
configurable fields, after stripping ignored ones) so a noisy, repeatedly-firing source
doesn't flood the pipeline with what is really the same underlying problem reported over and
over. Two tiers: same fingerprint + identical hash = full duplicate, suppressed outright; same
fingerprint + different hash = partial duplicate, treated as an update to the existing alert
rather than a brand-new one.

Kizashi has no equivalent today — every ingested record flows through normalization, analysis,
and trigger evaluation independently, so a source that repeats the same signal (a flapping
health check, a ticket system re-sending an unchanged webhook) can fire the same trigger
repeatedly, each producing its own Event and Action executions. Incidents (ADR-0111) groups
related Events together after the fact, manually; this is a different, earlier layer — stopping
exact-duplicate noise from reaching the pipeline at all.

Two things need to be gotten right for Kizashi specifically, not just copied from Keep's model:

1. **Where "suppress" cuts.** Kizashi's spec principle is that raw ingested data is never
   dropped — `RawRecord` is already durably written to ingestion-service's own database before
   normalization-service ever sees it (`ingestion-gateway` → `ingestion-service` DB →
   `record.ingested` → normalization-service). Suppression can't mean "don't store it"; that
   row already exists. It has to mean "don't let normalization-service's `record.normalized`
   event reach analysis-service/trigger-engine" — the raw and normalized data both stay fully
   auditable and visible on the Data page, but the pipeline downstream of normalization never
   re-reacts to something it's already reacted to.
2. **What "configurable fields" means for Kizashi's data model.** `NormalizationMapping`
   already owns the per-tenant-per-`source_type` knowledge of a raw payload's shape (its
   `field_map`) — the natural place to also declare which of those *normalized* target fields
   participate in the fingerprint, rather than inventing a second raw-JSON-path config
   mechanism just for dedup.

## Decision

**Scope this MVP to exact-duplicate suppression only.** Partial-duplicate-as-update is
deferred: doing it correctly raises real design questions this codebase hasn't answered yet
(does it mutate an existing Event's `occurred_at`/count? does it reopen a resolved Incident it's
linked to?) that belong in their own ADR once exact-duplicate suppression is proven in
production — same "defer the harder half" call ADR-0111 made for auto-correlation and
AI-generated summaries.

- **Config:** `NormalizationMapping` gains two new optional fields — `dedup_fields:
  Vec<String>` (names from the mapping's own `field_map` keys; empty = dedup disabled for this
  mapping, opt-in per source type rather than silently changing existing tenants' behavior) and
  `dedup_window_seconds: Option<i64>` (bounds how long a fingerprint is remembered before a
  repeat is treated as new again; unset = no expiry). Both `#[serde(default)]` so this is a
  purely additive change to the existing create/update API — no breaking change for
  config-admin-service or its callers.
- **Computation:** normalization-service computes the fingerprint (SHA-256 over the configured
  `dedup_fields`' *normalized* values, sorted by field name for stability) immediately after
  applying the mapping, using the same values already being written to `normalized_payload` —
  no second extraction pass over the raw JSON.
- **Storage:** a new `record_fingerprints` table in normalization-service's own Postgres schema
  (same per-service-owned-schema pattern as every other service this session):
  `(tenant_id, fingerprint, first_seen_record_id, last_seen_record_id, occurrence_count,
  first_seen_at, last_seen_at)`, primary key `(tenant_id, fingerprint)`.
- **Suppression:** if a fingerprint was already seen within `dedup_window_seconds` (or ever, if
  unset), normalization-service still writes `normalized_payload` back to ingestion-service (so
  the record stays fully visible/investigable on the Data page) but does **not** publish
  `record.normalized` — analysis-service and trigger-engine never see it, so no re-analysis, no
  repeated trigger fire, no duplicate Action executions. `occurrence_count`/`last_seen_at` are
  updated regardless, so the suppression itself is observable (a future UI surface — a
  "Duplicates suppressed: N" indicator — is a cheap follow-up once this lands, not required for
  the MVP).

## Consequences

A noisy, flapping source stops flooding triggers/actions with duplicate work once an operator
opts a mapping into dedup, without violating the "raw/normalized data is never silently
dropped" principle — everything is still stored and visible, only the *event-driven reaction*
to an exact repeat is suppressed. Opt-in via empty `dedup_fields` means this ships with zero
behavior change for every existing tenant/mapping until someone deliberately configures it.
Partial-duplicate-as-update, and any UI for configuring `dedup_fields`/viewing suppression
counts, remain open follow-up work — this ADR scopes the backend mechanism only; no
implementation has started yet.
