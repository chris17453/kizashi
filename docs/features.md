# Feature Log

Append-only. One entry per feature/fix/chore/doc change that lands on `main`, added in the same
PR that implements it — never batched, never backfilled after the fact. Newest entries at the
bottom. Do not edit or delete prior entries; corrections are new entries that reference the one
being corrected.

Entry format:

```
## [YYYY-MM-DD] <branch-id> — <title>
- **Type:** feature | fix | debug | docs | chore
- **Branch:** <type>/<NNNN>-<short-desc>
- **Summary:** what this adds/changes and why (1-3 sentences)
- **Tests:** what was added/run to verify it (be specific — actual test names/counts, not "added tests")
- **PR:** <link or #number>
- **ADR:** <link, if this touched a spec §11 open item — else "n/a">
```

---

## [2026-07-18] chore/0001-bootstrap-scaffolding — Repo bootstrap and foundational `common` crate
- **Type:** chore
- **Branch:** chore/0001-bootstrap-scaffolding
- **Summary:** Establishes the buildable foundation the rest of Kizashi is built on: the Cargo
  workspace root, remaining `scripts/` (bootstrap, new-service, new-connector, ci-local,
  adr-new), `docker-compose.yml` (Postgres/RabbitMQ/ClickHouse), `.github/workflows/ci.yml`
  wrapping `ci-local.sh`, `.env.example`, `.gitignore`, `rustfmt.toml`, `deny.toml`, `LICENSE`
  (MIT per spec §1), and the first workspace member, `crates/common` — the shared schema crate
  (`RawRecord`, `Event`, `EventTypeDefinition`, `TriggerDefinition`, `ActionExecution`,
  `NormalizationMapping`, spec §5) plus the `Connector` trait every connector implements
  (spec §6). `TriggerDefinition::evaluate` implements the v1 fixed-shape condition DSL
  (`CountOverWindow`, `ThresholdOverWindow`) per ADR-0001. `NormalizationMapping::apply`
  implements JSONPath-lite field mapping, never panicking on malformed operator config.
- **Tests:** `cargo test --workspace` — 28 passed, 0 failed (unit tests per type, each in a
  sibling `_test.rs` file per CLAUDE.md §2, plus `proptest` property tests
  `evaluate_never_panics_on_arbitrary_input` and `apply_never_panics_on_arbitrary_path_and_payload`
  fuzzing the trigger evaluator and normalization mapping engine). `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
- **PR:** #1
- **ADR:** docs/adr/0001-trigger-condition-dsl-shape.md, docs/adr/0002-mono-repo-layout.md

---

## [2026-07-18] docs/0001-adr-open-items — Remaining spec §11 ADRs
- **Type:** docs
- **Branch:** docs/0001-adr-open-items
- **Summary:** Closes out the remaining spec §11 open items with ADRs: ADR-0003 (Fabric/OneLake
  connector auth flow — per-tenant Entra app-registration client-credentials flow, no shared
  platform service principal against customer tenants), ADR-0004 (Analysis Service invocation
  pattern — micro-batched calls to Foundry/ML, per-tenant-configurable batch size/max wait,
  never mixing tenants in one batch), ADR-0005 (archive format — gzip'd NDJSON of `RawRecord`
  rows with a manifest header, reimported through the normal ingestion path). All five spec §11
  open items are now resolved (trigger DSL and mono-repo layout were ADR-0001/0002, landed in
  #1).
- **Tests:** n/a — docs-only change.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0003-fabric-onelake-connector-auth-flow.md,
  docs/adr/0004-analysis-service-invocation-pattern.md,
  docs/adr/0005-archive-format-specification.md

---

## [2026-07-18] fix/0001-branch-registry-order — Fix new-branch.sh registry/checkout ordering
- **Type:** fix
- **Branch:** fix/0001-branch-registry-order
- **Summary:** `scripts/new-branch.sh` bumped the counter and appended a row to
  `docs/branch-registry.md` on whatever branch it was invoked from, *before* checking out fresh
  `main` — so if that branch's copy of the registry differed from `main`'s (e.g. because a
  previous branch's own registry edit hadn't been merged yet), `git checkout main` failed with
  "local changes would be overwritten," exactly as hit when creating this fix's own branch from
  `docs/0001-adr-open-items`. Reordered: checkout+pull fresh `main` first, read the counter from
  that clean copy, create the branch, *then* edit the registry (so the edit lands as part of the
  new branch's own commit, as originally intended).
- **Tests:** Manually reproduced the failure (created a branch while on a branch with a locally
  modified `docs/branch-registry.md`), confirmed the original ordering failed with the exact
  "local changes would be overwritten by checkout" error, then confirmed the fixed script
  creates a branch cleanly from that same starting state.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a
