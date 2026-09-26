# ADR-0206: Signal retention tiers extend platform retention policy

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Kizashi `retention-service` owns TTL policy per (tenant, data_class) with archive, reimport and
compliance holds, and enforces only `DataClass::Raw` today. Yochō wants signals kept forever but
downsampled: hot full resolution, warm 1 h rollups, cold 1 d rollups on Blob, yearly Parquet
archive, with signals that contributed to an alert or score change pinned at full resolution.

## Decision

1. Add `DataClass::Signal` to the platform retention model. Its policy row holds the tier windows
   (`hot_days`, `warm_days`, `cold_days`) instead of one `ttl_days`.
2. `retention-service` stays the **single place policy is configured and audited**. Enforcement
   is ClickHouse-native: `yocho-store` renders the policy into table `TTL ... GROUP BY` and
   storage-policy clauses and re-applies them when the policy changes (`retention.changed` bus
   event). The sweep does not row-scan ClickHouse.
3. Rollups use `AggregateFunction` state columns (quantiles, uniq) so rollups of rollups stay
   correct.
4. **Pinning:** pinned signals are copied to `yocho_signals_pinned` (no downsampling TTL) at the
   moment they contribute to an alert or score change. Lineage reads the pinned table first.
5. Defaults until the open items are decided: hot 90 days, warm 2 years, cold indefinitely.

## Consequences

- One retention admin screen covers raw, events and signals.
- Changing tier windows rewrites table TTLs; this is a ClickHouse `ALTER`, audited like any other
  retention change.
