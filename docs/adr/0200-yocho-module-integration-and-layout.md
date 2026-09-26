# ADR-0200: Yochō module integration, workspace layout, and tenancy

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

`docs/yocho.md` adds Yochō, a churn early-warning module: signal extraction, entity attribution,
health scoring, lineage, a case library, and an analyst workbench, built on Kizashi's ingestion,
bus, storage, auth and console. The plan lists eight conflicts with Kizashi and says they must be
closed before Phase 0. This ADR settles the repository question and the conventions every later
Yochō ADR (0201–0208) relies on. ADR numbers start at 0200 because 0113–0193 are already in use
by uncommitted work on the main working tree; the gap avoids collisions.

The plan proposes a nested `yocho/crates/`, `yocho/packs/`, `yocho/services/` tree. CLAUDE.md §1
puts every crate under `crates/`. The plan leaves tenancy open ("single or multi-tenant");
Kizashi is multi-tenant by spec §1 and CLAUDE.md §5 requires `tenant_id` on every row. The plan
still says "agents" for connectors; Kizashi now calls deployable connector-pollers **Sensors**
and reserves "Agent" for AI/LLM analysis profiles.

## Decision

1. **Same repository, same Cargo workspace.** Yochō crates live in the Kizashi workspace so
   `common` changes and Yochō changes land atomically (the reason CLAUDE.md §1 chose a mono-repo).
2. **Flat layout under `crates/`, `yocho-` prefix.** Libraries: `crates/yocho-core`,
   `yocho-registry`, `yocho-store`, `yocho-crypto`, `yocho-model`, `yocho-lineage`,
   `yocho-labels`, `yocho-replay`. Packs: `crates/yocho-pack-customer-health`. Services:
   `crates/yocho-entity-resolver`, `yocho-extractor`, `yocho-signal-writer`, `yocho-detector`,
   `yocho-scorer`, `yocho-workbench`. All new crates get scaffolded with `scripts/new-service.sh`
   (services) or by hand from the same skeleton (libraries); MIT, like the rest of the workspace.
3. **Multi-tenant from day one.** Every Yochō table and every signal row carries `tenant_id`, and
   every query path gets a tenant-isolation test (CLAUDE.md §2, §5). The plan's "BU and account
   scoping" is a *sub-tenant* scope layered under the tenant, not a replacement for it.
4. **Terminology.** Source connectors are Kizashi **Sensors**; Yochō extends existing Sensors
   (graph-mail, zendesk, sql/fabric for ERP and AR) rather than adding connector crates, as the
   plan already states. "Agent" stays reserved for AI analysis profiles.
5. **Yochō is a module, not a fork.** Anything generic the plan needs (webhook nudges, key
   provider, ClickHouse migrations, config versioning) is built as a platform capability in the
   existing crates and consumed by Yochō, not duplicated inside `yocho-*` crates.

## Consequences

- One CI run covers Yochō and Kizashi; Yochō inherits the 85% coverage ratchet, clippy, fmt,
  audit and deny gates.
- The plan's proposed tree in `docs/yocho.md` is superseded by §2 above; the plan is kept as
  written and this ADR is the authority on layout.
- If Yochō later needs an independent release cadence, splitting it out is a new ADR.
