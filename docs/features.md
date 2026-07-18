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
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0001-trigger-condition-dsl-shape.md, docs/adr/0002-mono-repo-layout.md
