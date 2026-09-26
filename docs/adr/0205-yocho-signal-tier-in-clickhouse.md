# ADR-0205: Signals as a new ClickHouse tier, with promotion to Kizashi Events

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Kizashi Events (spec §5.2) are per-occurrence rows in ClickHouse with a status lifecycle (new,
triggered, actioned, dismissed) that drive the trigger engine, incidents and actions. ClickHouse is
reached over raw HTTP; tables are created at service startup with `CREATE TABLE IF NOT EXISTS`;
there are no migrations, TTLs, codecs or rollups.

Yochō signals are a different shape: high-volume, narrow, append-only measurements
(`entity_type, entity_id, signal_type, ts, value, dims`) that are downsampled over time and never
have a status.

## Decision

1. **Separate signal tier.** A narrow append-only `yocho_signals` table (plus rollup tables) in
   the same ClickHouse, owned by `yocho-store`. Row: `tenant_id, signal_id (ULID), entity_type,
   entity_id, signal_type, ts, value, dims Map, extractor_version, config_version, provenance
   (encrypted)`. Ordered by `(tenant_id, signal_type, entity_id, ts)`.
2. **Signals are not Events.** Only *actionable* outputs (detector anomalies, score threshold
   crossings, urgent-and-unanswered) are promoted: the scorer or detector publishes a Kizashi
   `event.created`-compatible Event referencing the signal ids. Triggers, incidents, alert dedup
   (ADR-0112) and actions then work unchanged.
3. **Versioned ClickHouse migrations.** `yocho-store` introduces numbered, checked-in ClickHouse
   DDL applied by a migration runner with a `schema_migrations` table, instead of startup
   `IF NOT EXISTS`. Existing Kizashi tables may adopt it later.
4. **Idempotent writes.** Every signal write carries an idempotency key; the table uses
   `ReplacingMergeTree` on the idempotency key, so duplicate delivery collapses.
5. Signal registry (types, value kind, unit, rollup rules, retention tier) lives in Postgres
   (`yocho-registry`). Adding a signal is a registry row plus an extractor, with no DDL.

## Consequences

- Kizashi's trigger/action/incident machinery is reused as-is for alerts.
- Two ClickHouse styles coexist for a while (startup DDL vs migrations) until Kizashi adopts the
  runner.
- Late-bound attribution (ADR-0208) joins signals to entities at query time.
