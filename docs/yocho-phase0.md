# Yochō Phase 0 — Foundation work breakdown

Plan: `docs/yocho.md`. Integration decisions: ADR-0200 … ADR-0208.

**Exit criterion (from the plan):** a synthetic signal flows end to end into ClickHouse,
attributed, encrypted and audited.

| # | Branch (indicative) | Scope | Depends on |
| --- | --- | --- | --- |
| P0-1 | `feature/…-yocho-core` | `crates/yocho-core`: `SignalId` (ULID), `Signal`, `SignalValue` (gauge/counter/categorical/score), `EntityRef`, `Provenance`; `Extractor`/`Detector`/`Scorer` traits; `signal.emitted` bus message + contract test | — |
| P0-2 | `feature/…-yocho-crypto` | `crates/yocho-crypto`: `KeyProvider` trait + local provider, per-subject DEK envelope (AES-256-GCM), DEK cache, HMAC blind index, shred + audit + `subject.shredded` (ADR-0204) | — |
| P0-3 | `feature/…-yocho-store` | `crates/yocho-store`: versioned ClickHouse migration runner, `yocho_signals` DDL (codecs, `ReplacingMergeTree` on idempotency key), writer and reader, tenant-isolation test (ADR-0205) | P0-1 |
| P0-4 | `feature/…-yocho-registry` | `crates/yocho-registry`: Postgres signal registry (name, value kind, unit, producer, rollup rules, tier), CRUD via config-admin with immutable audit | P0-1 |
| P0-5 | `feature/…-bu-account-scoping` | auth-service: business-unit and account sub-scopes under tenant; role grants scoped to BU/account; header propagation; isolation tests | — |
| P0-6 | `feature/…-content-view-audit` | append-only audit of content views (who viewed which content, which customer drill-down) | — |
| P0-7 | `feature/…-yocho-entity-resolver` | deterministic tier + versioned identity→customer/BU mapping + ClickHouse dictionary export (ADR-0208) | P0-1, P0-3 |
| P0-8 | `feature/…-yocho-signal-writer` | `crates/yocho-signal-writer` service: consume `signal.emitted`, encrypt provenance, validate against registry, write ClickHouse; end-to-end synthetic-signal test on the real compose stack (exit criterion) | P0-2, P0-3, P0-4, P0-7 |

Parallel start: P0-1, P0-2, P0-5, P0-6. Then P0-3, P0-4. Then P0-7. Then P0-8.

## Open product items that do not block Phase 0

Everything in the plan's "Open items" except the eight Kizashi-integration items (closed by the
ADRs above, as reversible defaults). Defaults used where Phase 0 needs a number: raw mail buffer
TTL 90 days, operator-configurable per tenant and source (ADR-0201); signal tiers hot 90 d / warm 2 y / cold indefinite (ADR-0206).
