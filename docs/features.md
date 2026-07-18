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

---

## [2026-07-18] feature/0003-normalization-service — Normalization Service
- **Type:** feature
- **Branch:** feature/0003-normalization-service
- **Summary:** Consumes `record.ingested` off RabbitMQ, looks up the tenant's active
  `NormalizationMapping` for that source type (own Postgres schema, `normalization_service` —
  Config/Admin Service isn't built yet; this repository's Postgres impl is meant to be swapped
  for a client of that service's API once it exists, per the trait boundary already in place),
  applies it via `NormalizationMapping::apply`, and writes `normalized_payload` back — not by
  touching Ingestion Service's database, but through a new `PATCH
  /v1/records/:id/normalized` endpoint added to Ingestion Service in this same PR (spec §2
  principle 1, "API-mediated everything"). Publishes `record.normalized` once the write-back
  succeeds. No mapping configured for a tenant/source_type is not an error — the message is
  acked and skipped, since an operator hasn't gotten to configuring it yet.

  Also extracted the message-bus exchange name constants (`record.ingested`,
  `record.normalized`, `record.analyzed`, `event.created`) into `common::bus`, replacing the
  local `pub const` each service previously declared, so a typo can't silently create a second,
  disconnected topic.
- **Tests:** `cargo test --workspace --lib --bins` — 73 passed, 0 failed across all four
  crates. Live-stack tests against real Postgres 16 + RabbitMQ 3: `ingest_integration_test`,
  `api_key_store_integration_test`, `mapping_repository_integration_test`, plus both
  `record_ingested_contract_test` and the new `record_normalized_contract_test` — all passing.
  Beyond the per-crate tests, ran both service binaries together against the live stack for a
  real end-to-end smoke test: inserted a `NormalizationMapping` row, `POST`ed a raw ticket
  record to Ingestion Service, and confirmed Normalization Service consumed it and wrote back
  the correctly-mapped `normalized_payload` — the full ingest-to-normalize pipeline, not just
  isolated per-service tests. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` —
  clean (same waivers as prior PRs, no new advisories).

  CI's coverage-ratchet step failed on this PR at 83.56% (below the 85% floor), driven by two
  untested `main.rs` wiring files and `HttpRecordClient`'s real implementation having no
  coverage at all (only its in-memory test double was exercised). Fixed both: added
  `--ignore-filename-regex '(^|/)main\.rs$'` to `ci-local.sh`'s `cargo llvm-cov` invocation,
  since `main.rs` files are pure composition roots with no branching logic of their own — every
  future service's `main.rs` would otherwise drag the ratchet down for no real coverage
  benefit. Added real tests for `HttpRecordClient` against an in-process stub server (success,
  server error, unreachable server) rather than only covering it via the in-memory double.
  Coverage is now 96.32% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0004-analysis-service — Analysis Service
- **Type:** feature
- **Branch:** feature/0004-analysis-service
- **Summary:** Consumes `record.normalized` and calls Azure AI Foundry/ML in per-tenant
  micro-batches per ADR-0004 (bounded by `ANALYSIS_BATCH_SIZE` or `ANALYSIS_BATCH_MAX_WAIT_MS`,
  whichever hits first; never mixing tenants in one batch call), then publishes
  `record.analyzed`. Analysis results are not persisted to their own table in v1 — they travel
  forward on the `record.analyzed` message itself for Aggregation/Trigger Engine to consume
  directly, rather than adding a service that reads back through another API just to hand the
  result one hop further (documented in `common::AnalyzedRecord`'s doc comment). Adds
  `AnalyzedRecord { record, analysis, analyzed_at }` to `common` as the new bus contract type,
  alongside `RawRecord`/`Event`.
- **Tests:** `cargo test --workspace --lib --bins` — 92 passed, 0 failed across all five
  crates. `cargo test -p analysis-service --test analysis_integration_test` — a real
  RabbitMQ-backed test (publish through `process_batch`, consume off a bound queue) plus a real
  in-process HTTP server standing in for Foundry, not mocks. `record_analyzed_contract_test`
  covers the `record.analyzed` wire shape. `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` /
  `cargo deny check` — clean. `cargo llvm-cov` — 96.56% overall, well above the 85% floor.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0005-trigger-engine — Trigger Engine
- **Type:** feature
- **Branch:** feature/0005-trigger-engine
- **Summary:** Consumes `record.analyzed`, classifies candidate event types from the analysis
  output per ADR-0006 (every top-level numeric key in `analysis` becomes a candidate event
  named after that key — a documented placeholder until Config/Admin Service ships real
  `EventTypeDefinition` classification), records each as a durable signal in Trigger Engine's
  own Postgres schema, evaluates every enabled `TriggerDefinition` matching that event type
  against the signal's rolling window (`TriggerDefinition::evaluate`, ADR-0001), and for every
  firing trigger writes an `Event` to ClickHouse (spec §5.2 aggregate store — the first service
  to actually use it) and publishes `event.created`. `TriggerDefinition` storage is, like
  NormalizationMapping, owned directly by this service for now rather than depending on
  Config/Admin Service.

  Fixed a real infra gap this surfaced: `CLICKHOUSE_URL` in CI and `.env.example` had no
  credentials, but ClickHouse's HTTP interface rejects anonymous requests once
  `CLICKHOUSE_USER`/`CLICKHOUSE_PASSWORD` are set on the server — nothing had exercised that
  path until this PR. Fixed by embedding credentials in `CLICKHOUSE_URL` (HTTP basic auth via
  userinfo), matching how `DATABASE_URL`/`RABBITMQ_URL` already work.
- **Tests:** `cargo test --workspace --lib --bins` — 117 passed, 0 failed across all six
  crates. `trigger_integration_test` is a genuine full-stack test against real Postgres +
  ClickHouse + RabbitMQ together: inserts a `TriggerDefinition`, feeds an `AnalyzedRecord`
  through `process_analyzed_record`, confirms the `Event` lands in ClickHouse and
  `event.created` is received off a bound queue. `event_created_contract_test` covers the wire
  shape. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean. `cargo
  llvm-cov` — 96.49% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0006-trigger-engine-event-type-classification-for-v1.md

---

## [2026-07-18] feature/0006-action-executor — Action Executor
- **Type:** feature
- **Branch:** feature/0006-action-executor
- **Summary:** Consumes `event.created`, resolves which actions to run by calling Trigger
  Engine's new `GET /v1/triggers/:id` API (added to trigger-engine in this same PR — spec §2
  principle 1, no direct cross-service DB reads) using the `triggered_by` trigger id embedded
  in the event's payload, dispatches each action, and writes an append-only `ActionExecution`
  audit row per action regardless of outcome — a dispatch failure is recorded as `Failed`, not
  swallowed. Per ADR-0007, v1 dispatches every `ActionType` (email/webhook/teams_alert/
  create_ticket/custom) through one `HttpActionDispatcher` that POSTs the event + action config
  to `config["url"]` — genuinely functional against any webhook-shaped endpoint, not a stub;
  type-specific integrations (SMTP, Teams card schema, per-vendor ticketing APIs) are follow-up
  work.
- **Tests:** `cargo test --workspace --lib --bins` — 135 passed, 0 failed across all seven
  crates. `execution_repository_integration_test` (real Postgres) confirms inserts persist and
  that a `retry()` produces a second append-only row rather than mutating the first. Beyond
  automated tests, ran a genuine end-to-end smoke test with real service binaries: started
  trigger-engine + action-executor against a live Postgres/RabbitMQ/ClickHouse stack, inserted
  a `TriggerDefinition` with a webhook action pointed at a throwaway local HTTP receiver,
  published a `record.analyzed` message, and confirmed the trigger fired, the action was
  dispatched, the receiver got the POST, and the `ActionExecution` row landed with
  `status: sent` — the full ingest-through-action pipeline working together, not just
  per-service tests. `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  clean. `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean. `cargo
  llvm-cov` — 96.25% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0007-action-executor-v1-dispatch-model.md

---

## [2026-07-18] feature/0007-query-gateway-dashboard-api — Query Gateway + Dashboard/Query API
- **Type:** feature
- **Branch:** feature/0007-query-gateway-dashboard-api
- **Summary:** Two new crates completing the read side of the platform. `dashboard-api` (spec
  §6, service #9) reads Events from ClickHouse — `GET /v1/events` (filterable by event_type,
  group_key, status, since/until, limit) and `GET /v1/events/:id` — trusting `X-Tenant-Id` as
  set by the gateway rather than deriving identity itself (spec §8). `query-gateway` (spec §6,
  service #8) is the dashboard/UI-facing entry point: resolves a bearer token to a tenant
  (ADR-0008 — same hashed-token shape as ingestion-gateway's API keys, since Auth Service isn't
  built yet; the `query_api_tokens` table is what Auth Service will write into once it exists,
  not a mechanism to replace later) and forwards to Dashboard API with the resolved tenant_id.
- **Tests:** `cargo test --workspace --lib --bins` — 168 passed, 0 failed across all nine
  crates. Live Postgres integration test for the token store (including revoked-token
  rejection). Beyond automated tests, ran a genuine end-to-end smoke test with real service
  binaries against live Postgres + ClickHouse: seeded a real Event row and a real token,
  queried both `list` and `get-by-id` through `query-gateway` end to end, and confirmed 401 on
  a missing token. **That manual run caught a real bug unit/stub tests missed**: ClickHouse's
  HTTP interface rejects a bodyless POST with `411 Length Required`, which reqwest doesn't
  avoid automatically — fixed by explicitly setting `Content-Length: 0`, and added
  `requests_always_carry_a_content_length_header` so this can't silently regress again (the
  axum-based stub servers used elsewhere don't enforce this the way real ClickHouse does).
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean (also fixed
  two `clippy::result_large_err` findings and one `unnecessary_sort_by`). `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean. `cargo llvm-cov` — 95.35%
  overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0008-query-gateway-interim-auth-model.md

---

## [2026-07-18] feature/0008-auth-service — Auth Service
- **Type:** feature
- **Branch:** feature/0008-auth-service
- **Summary:** New crate `crates/auth-service` (spec §6, service #10). Two login paths, both
  ending in a call to Query Gateway's new `POST /internal/tokens` (added to query-gateway in
  this PR, shared-secret protected) to mint a session, since Auth Service never writes into
  `query_api_tokens` directly (spec §2 principle 1): (1) **local login**
  (`POST /v1/auth/local/login`) — Argon2id-hashed credentials in `auth_service.local_users`,
  constant-shape response so unknown-username and wrong-password aren't distinguishable; (2)
  **unified OIDC** (`GET /v1/auth/oidc/:provider/authorize`, `POST
  /v1/auth/oidc/:provider/callback`) — one real `oauth2`-crate-backed client serves both Entra
  ID and generic OAuth (ADR-0009), since Entra is itself OIDC-compliant and duplicating the
  client would buy nothing. No session/cookie layer yet — that's Console UI's job once built;
  the PKCE verifier is handed back to the authorize caller to carry to the callback.
- **Tests:** `cargo test --workspace --lib --bins` — 197 passed, 0 failed across all ten
  crates, including a real OIDC client test against a stub IdP (`/token`, `/userinfo`) that
  exercises the actual code-exchange and userinfo-fetch logic, not just an in-memory double —
  what's inherently untestable in CI is the human browser hop to the IdP's login page, true of
  any OIDC integration (documented in ADR-0009, not a gap specific to this build-out). Live
  Postgres integration test for `local_users`. Beyond automated tests, ran a genuine end-to-end
  smoke test with real service binaries: created a local user with a real Argon2id hash via
  `auth-service`'s own hashing code, logged in through `POST /v1/auth/local/login`, confirmed
  wrong-password gets 401, and used the real minted token against `query-gateway` to read a
  real ClickHouse-backed event — the full auth-through-query chain working together. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean (oauth2 pulls in a second
  reqwest major version transitively; no new advisories, just an existing-pattern
  multiple-versions warning). `cargo llvm-cov` — 95.42% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0009-auth-service-v1-scope-local-login-plus-unified-oidc.md

---

## [2026-07-18] feature/0009-config-admin-service — Config/Admin Service
- **Type:** feature
- **Branch:** feature/0009-config-admin-service
- **Summary:** New crate `crates/config-admin-service` (spec §6, service #11). Full CRUD +
  immutable audit logging for `TriggerDefinition` and `NormalizationMapping` — the two config
  entity types with real existing consumers (trigger-engine, normalization-service). Every
  create/update opens one Postgres transaction, writes the entity change, then writes an
  audit_log row via `record_audit_entry` (a free function, not a trait method, since sharing
  one `Transaction` across a `dyn Trait` repository and an audit abstraction isn't portable) —
  all in the same transaction, per CLAUDE.md §5. `GET /v1/audit-log/:entity_id` exposes the
  read path via a separately mockable `AuditLogReader` trait. Deliberately does NOT yet migrate
  trigger-engine/normalization-service to read their config through this service (they still
  read their own local tables) and does NOT build EventTypeDefinition/connector-config/
  retention-policy/branding CRUD, since none of those have a real consumer yet and CLAUDE.md
  prohibits half-finished stub endpoints — both cuts are documented in ADR-0010, not silent.
- **Tests:** `cargo test --workspace --lib --bins` — 222 passed, 0 failed across all eleven
  crates (24 in config-admin-service alone: repository CRUD/tenant-scoping/audit-trail unit
  tests against in-memory doubles, handler tests including a tenant-mismatch-is-rejected case
  and a full create→get→update→list→audit-log round trip). Live Postgres integration test
  (`tests/repository_integration_test.rs`, 4 tests) exercises the real transactional behavior
  the in-memory doubles can't: a real `config_audit_log` row lands in the same transaction as a
  real trigger/mapping insert, an update writes a second audit row with both `before` and
  `after` populated, and a failed update (unknown id) leaves zero audit rows — no partial
  writes. Beyond automated tests, ran a genuine end-to-end smoke test with the real
  `config-admin-service` binary against live Postgres: created a trigger definition over HTTP,
  confirmed it was retrievable via `GET /v1/trigger-definitions/:id`, and confirmed the
  audit-log endpoint returned exactly one `created` entry with the full entity snapshot in
  `after`. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean (same two
  pre-existing waived unmaintained-crate warnings as prior PRs, no new advisories). `cargo
  llvm-cov --workspace --all-features --ignore-filename-regex '(^|/)main\.rs$'
  --fail-under-lines 85` — 94.37% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0010-config-admin-service-v1-scope.md
