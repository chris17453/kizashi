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

---

## [2026-07-18] feature/0001-ingestion-service — Ingestion Service
- **Type:** feature
- **Branch:** feature/0001-ingestion-service
- **Summary:** First deployable pipeline service (spec §6, service #3): `POST /v1/records`
  validates a submitted record (non-empty `connector_id`, non-nil `tenant_id`, non-null
  `raw_payload`), persists it as a `RawRecord` row in Postgres (migration
  `0001_create_raw_records.sql`, tenant/connector/ingested_at indexed per CLAUDE.md §5), then
  publishes the same record to the `record.ingested` fanout exchange over RabbitMQ. Repository
  and publisher are behind traits (`RawRecordRepository`, `EventPublisher`) with Postgres/
  RabbitMQ implementations and in-memory test doubles, so handler logic is unit-testable
  without a live stack while still getting real end-to-end coverage. A publish failure is
  logged but does not roll back the (already-durable) write — the raw store is the source of
  truth, not the bus.
- **Tests:** `cargo test --workspace --lib --bins` — 39 passed, 0 failed (28 in `common`, 11 in
  `ingestion-service`, all with in-memory repository/publisher doubles). Ran
  `cargo test -p ingestion-service --test ingest_integration_test --test
  record_ingested_contract_test` against real Postgres 16 + RabbitMQ 3 containers — 3 passed,
  0 failed: full round trip (HTTP POST → Postgres row → `record.ingested` message consumed off
  a bound queue) plus the `record.ingested` wire-shape contract test. `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
  Upgraded sqlx 0.7→0.8 (default-features off) after `cargo audit` failed CI on
  RUSTSEC-2024-0363 (fixed in sqlx ≥0.8.1); re-ran the full test suite (42 tests) against fresh
  Postgres/RabbitMQ containers to confirm the upgrade didn't change behavior, and switched from
  the `sqlx::migrate!` macro to the runtime `sqlx::migrate::Migrator::new(...)` API so the
  "macros" feature (which unconditionally compiles the mysql/sqlite backends, not just
  postgres) isn't needed. One remaining advisory, RUSTSEC-2023-0071 (rsa Marvin Attack,
  transitive via sqlx's always-compiled mysql backend, no fix available upstream, unreachable
  since Kizashi never opens a MySQL connection), is explicitly waived with rationale in
  `.cargo/audit.toml` per CLAUDE.md §5. Also fixed `cargo deny check` (bans/licenses), which
  had never run clean before: added `publish = false` workspace-wide (internal path deps read
  as "wildcard dependencies" to crates.io-publishable crates), allowed the CDLA-Permissive-2.0
  license (webpki-roots' CA-bundle license, not a code license), and waived
  RUSTSEC-2024-0384/RUSTSEC-2025-0134 (unmaintained-crate warnings, not vulnerabilities,
  transitive via lapin) alongside RUSTSEC-2023-0071 in `deny.toml`.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0002-ingestion-gateway — Ingestion Gateway
- **Type:** feature
- **Branch:** feature/0002-ingestion-gateway
- **Summary:** The single agent-facing entry point (spec §6, service #2), sitting in front of
  Ingestion Service. `POST /v1/ingest` requires an `X-Api-Key` header, resolves it to a tenant
  via `ApiKeyStore` (Postgres-backed, keys stored only as SHA-256 hashes — the plaintext key is
  never persisted), applies a per-tenant fixed-window `RateLimiter`, then forwards the request
  to Ingestion Service with `tenant_id` overwritten from the *authenticated* identity — a
  caller-supplied `tenant_id` in the request body is always discarded, so a misconfigured or
  malicious connector cannot write into a tenant it doesn't hold a key for (spec §8 tenant
  isolation). Missing/invalid keys return 401, rate-limit exhaustion returns 429, a malformed
  body returns 400, and an unreachable Ingestion Service returns 502.
- **Tests:** `cargo test -p ingestion-gateway --lib` — 14 passed, 0 failed, all against
  in-memory doubles (`InMemoryApiKeyStore`, a deterministic `TestClock`-driven `RateLimiter`,
  and a real in-process axum server standing in for Ingestion Service so the HTTP proxy path is
  genuinely exercised, not mocked). `cargo test -p ingestion-gateway --test
  api_key_store_integration_test` against a real Postgres 16 container — 1 passed, 0 failed
  (stores a key, resolves it, confirms an unknown key and a revoked key both resolve to
  nothing). `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` and `cargo deny check` — clean (same waivers
  as feature/0001-ingestion-service, no new advisories).

  Also fixed a real cross-service bug this PR exposed: ingestion-service and
  ingestion-gateway both connect to the same shared Postgres instance, and both shipped a
  first migration file named `0001_...` — sqlx tracks applied migrations by version number in
  one shared `_sqlx_migrations` table, so the moment both services' migrators ran against that
  database, the second one hit a `VersionMismatch` (CI caught this; it can't reproduce with
  either service tested alone). Added `common::db::connect_with_schema`, used by both services
  in `main.rs` and their integration tests: every service now gets its own Postgres schema
  (`ingestion_service`, `ingestion_gateway`), applied to every pooled connection via
  `after_connect`, so table names and migration histories can never collide across services
  sharing one database. Verified by running both services' live-Postgres integration tests
  together against one container and confirming each landed its tables in its own schema
  (`\dn` / `information_schema.tables`).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a
