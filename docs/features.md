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

---

## [2026-07-18] feature/0010-retention-service — Retention/Archival Service

- **Type:** feature
- **Branch:** feature/0010-retention-service
- **Summary:** New crate `crates/retention-service` (spec §6, service #12): enforces per-tenant
  retention policy by archiving `RawRecord` rows older than their TTL to S3-compatible object
  storage in the ADR-0005 NDJSON+gzip format, then hard-deleting them from the hot store
  (archive-then-delete, never the reverse), and supports reimport of an archived batch back
  through the pipeline (spec §9). Ships with its own retention-policy CRUD + immutable audit
  log (same in-same-transaction pattern as config-admin-service, CLAUDE.md §5) and a MinIO
  container added to docker-compose as the self-hosted S3-compatible test/dev backend behind a
  new `ArchiveStore` trait (`S3ArchiveStore` impl via `aws-sdk-s3`). Extends
  `ingestion-service` with the two endpoints Retention Service needs to reach the raw store
  without touching its Postgres schema directly (spec §2 principle 1): tenant-scoped
  `GET /v1/records?older_than=&limit=` and `DELETE /v1/records/:id`. See ADR-0011 for the full
  v1 scope decision (self-owned policy store, S3-compatible backend, why reimport bypasses
  Ingestion Gateway).
- **Bug found and fixed in this PR, not shipped:** the first cut of `list_older_than`/`delete`
  on `ingestion-service` had no `tenant_id` scoping at all — any tenant's sweep would list and
  delete every tenant's aged records, and a sweep batch could get mis-attributed to the wrong
  tenant in the archive. Caught by the manual end-to-end smoke test (two tenants, only one with
  a retention policy — the unpolicied tenant's equally-old record was being swept anyway),
  invisible to per-service unit tests using tenant-blind stub data. Fixed by threading
  `tenant_id` through the repository trait, both HTTP endpoints (via `X-Tenant-Id`, matching
  every other tenant-scoped read path in this codebase), the `RawRecordClient` trait, and
  `sweep`'s call sites; added `list_older_than_is_scoped_to_tenant` and
  `delete_returns_false_when_tenant_does_not_match` regression tests in ingestion-service, plus
  a tenant-scoping test in retention-service's own client test double, so this can't regress
  silently again.
- **Tests:** `cargo test --workspace --lib --bins` — all crates green (retention-service alone:
  40 unit/handler tests covering repository CRUD + audit trail, archive encode/decode
  round-trip, sweep pagination/disabled-policy/non-Raw-data-class/archive-failure paths,
  reimport partial-failure handling, and full HTTP handler round trips). Live-infrastructure
  integration tests: `retention_policy_integration_test.rs` (4 tests) against real Postgres,
  same transactional-audit-row pattern verified as config-admin-service;
  `s3_archive_store_integration_test.rs` (3 tests) against a real MinIO container — write/read
  round trip, not-found handling, idempotent bucket creation. Beyond automated tests, ran a
  genuine end-to-end smoke test with real `ingestion-service` and `retention-service` binaries
  against live Postgres + MinIO: seeded old records for two different tenants, created a
  retention policy for only one, triggered a sweep, and confirmed only that tenant's record was
  archived and deleted while the other tenant's equally-old record was untouched (this is what
  caught the tenant-isolation bug above) — then triggered reimport of the archived batch and
  confirmed the record reappeared in the hot store with its original payload intact. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean after waiving three new
  advisories (RUSTSEC-2026-0098/-0099/-0104, rustls-webpki 0.101.7 name-constraint/CRL bugs
  transitive via `aws-sdk-s3`'s pinned old rustls stack — documented rationale in both
  `.cargo/audit.toml` and `deny.toml`; not exploitable against a non-attacker-controlled S3
  endpoint, no newer `aws-smithy-http-client` release exists yet). `cargo llvm-cov` — 94.11%
  overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0011-retention-archival-service-v1-scope-self-hosted-s3-archival-self-owned-policy-store.md

---

## [2026-07-18] feature/0011-observability — Platform Observability

- **Type:** feature
- **Branch:** feature/0011-observability
- **Summary:** New crate `crates/observability` (spec §6, service #13). `GET /v1/health`
  fans `GET /healthz` out concurrently to every service in an operator-configured
  `SERVICE_REGISTRY` (`name=url` pairs) and reports per-service up/down plus an overall
  platform status (503 if any one service is down, so the endpoint itself doubles as an
  external liveness check). `GET /v1/backlog` reads per-stage queue depths from RabbitMQ's
  management HTTP API (already enabled in docker-compose) for the four pipeline queues
  (`normalization-service.record.ingested`, `analysis-service.record.normalized`,
  `trigger-engine.record.analyzed`, `action-executor.event.created`), giving a single ordered
  view of where the ingest → normalize → analyze → act chain is backing up. Per-service
  `/metrics` request/latency instrumentation is deliberately deferred — it needs a shared
  `common` instrumentation helper and touches every existing service, which is its own scoped
  follow-up, not something to gesture at with stub endpoints here (ADR-0012).
- **Tests:** `cargo test --workspace --lib --bins` — all thirteen crates green (20 unit tests in
  observability alone: registry parsing, health fan-out aggregation logic against an in-memory
  checker double, and both the HTTP health checker and RabbitMQ backlog reader against real
  stub axum servers). Live integration test
  (`tests/rabbitmq_backlog_integration_test.rs`) against a real RabbitMQ management API —
  confirms one depth entry per pipeline stage and correctly reports zero backlog for
  not-yet-declared queues rather than erroring. Beyond automated tests, ran a genuine
  end-to-end smoke test with the real `observability` binary: registered a mix of a real
  running `ingestion-service` and two intentionally-unreachable services, confirmed
  `/v1/health` correctly reported the real service up, the fake ones down, and 503 overall;
  confirmed `/v1/backlog` returned all four pipeline stages against live RabbitMQ. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean, no new advisories. `cargo
  llvm-cov` — 94.27% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0012-platform-observability-v1-scope-health-aggregation-and-rabbitmq-backlog-visibility.md

---

## [2026-07-18] feature/0012-connectors — Connectors (zendesk, graph-mail, graph-teams, sql, fabric, generic)

- **Type:** feature
- **Branch:** feature/0012-connectors
- **Summary:** Six new connector crates under `crates/connectors/` (spec §6, service #1) plus
  a shared `connector-runtime` library (ADR-0013): `HttpIngestionClient` (posts polled records
  to Ingestion Gateway's `POST /v1/ingest`), `run_poll_cycle` (one CronJob poll cycle — poll,
  post every record, count successes/failures without aborting the batch on one failure), and
  `entra_client_credentials::fetch_access_token` (the OAuth2 client-credentials/app-only flow
  ADR-0003 specifies, shared by the three Entra-backed connectors). `generic` polls a
  configurable JSON HTTP endpoint. `sql` runs an operator-configured `SELECT` against any
  Postgres-wire-protocol database via a dynamic row-to-JSON mapper. `zendesk` polls the
  Incremental Ticket Export API (HTTP Basic `{email}/token`). `graph-mail`/`graph-teams` poll
  Microsoft Graph mail/channel messages via Entra app-only auth. `fabric` polls Fabric's SQL
  analytics endpoint over real TDS (`tiberius` crate) with an Entra AAD token in place of a
  username/password, reusing the `sql` connector's row-mapping approach — OneLake and
  connector-config-via-Config/Admin-Service remain deferred follow-ups (ADR-0013).
- **Tests:** `cargo test --workspace --lib --bins` — all twenty crates green (connector-runtime:
  12 tests against real stub HTTP servers; each connector: unit tests against real stub HTTP
  servers matching its source API's shape — Zendesk incremental-export JSON, Graph's `{value:
  [...]}` list shape, generic's bare JSON array — covering the happy path, auth failure, rate
  limiting, and unreachable-source cases). `sql`'s live Postgres integration test creates a real
  temp table and confirms row→JSON mapping end to end. `fabric`'s live integration test proves
  the real TCP connect + TDS handshake + `AuthMethod::aad_token` login attempt against a real
  SQL Server container (standing in for Fabric, since both speak TDS) and confirms a rejected
  AAD login is correctly classified `ConnectorError::AuthFailed` — the happy-path query against
  real Fabric data can't be proven without a real Fabric tenant, the same inherent limitation
  ADR-0009 already documents for OIDC's browser hop (no `raw_record_contract_test.rs` exists
  for `fabric` for this reason, documented in its `lib.rs`). Beyond automated tests, ran two
  genuine end-to-end smoke tests with real binaries: `connector-generic` against a real stub
  HTTP source through a live `ingestion-gateway` (API-key auth) → `ingestion-service` → real
  Postgres, and `connector-sql` against real seeded Postgres rows through the same chain — both
  confirmed the exact source records landed in the hot store under the correct tenant. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean, no new advisories. `cargo
  llvm-cov` — 93.49% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0013-connectors-v1-scope-shared-poller-runtime-env-driven-per-tenant-config-fabric-sql-endpoint-only.md

---

## [2026-07-18] feature/0013-console-ui — Console UI

- **Type:** feature
- **Branch:** feature/0013-console-ui
- **Summary:** New `ui/` crate (spec §7, the last of the thirteen planned services/components)
  — a server-rendered Rust web app (`axum` + `askama` compile-time-checked templates), not a
  WASM SPA (ADR-0014): every other service in this repo is tested via
  `tower::ServiceExt::oneshot` with zero browser-automation tooling anywhere in the stack, and a
  WASM SPA's natural test story needs a headless browser driver this environment doesn't have —
  so the console is built to fit the same proven test methodology instead of introducing a new
  one for the highest-uncertainty piece of the platform. Ships: a dark-mode console shell (left
  nav, OpenShift/Instana-direction styling), a login page posting to Auth Service's local-login
  endpoint with Console UI's own `HttpOnly`-cookie session layer (in-memory session store keyed
  by a random id — Auth Service has no session/cookie layer of its own, ADR-0009 said that's
  this UI's job), and three authenticated read views: Events (via Query Gateway), Triggers (via
  Config/Admin Service), and Platform Health (via Observability). Topology graph, configurable
  dashboards, reporting, event type management, a real trigger builder, data lifecycle UI, and
  RBAC/admin UI are explicitly deferred, documented follow-ups (ADR-0014) — not stub pages.
- **Tests:** `cargo test --workspace --lib --bins` — all twenty-one crates green (35 tests in
  `kizashi-ui`: session store CRUD, cookie-parsing/session-guard redirect logic, every HTTP
  client (auth/events/triggers/health) against real stub servers matching each backend's real
  response shape, and every page handler — signed-in render, signed-out redirect, and
  backend-failure error display — via `tower::ServiceExt::oneshot`, the same pattern as every
  other service in this repo). Beyond automated tests, ran a genuine end-to-end smoke test with
  the real `kizashi-ui` binary against six other real running services (auth-service,
  query-gateway, dashboard-api, config-admin-service, observability, Postgres): logged in with
  a real Argon2id-hashed local user, confirmed the session cookie was set, loaded `/events`,
  `/triggers` (seeded a real trigger via Config/Admin Service and confirmed it rendered), and
  `/health` (showing real live service status) all while signed in, then logged out and
  confirmed both the expired cookie and unauthenticated requests correctly redirect to
  `/login`. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean, no new
  advisories. `cargo llvm-cov` — 93.78% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0014-console-ui-v1-scope-server-rendered-rust-web-app-shell-plus-read-only-events-triggers-health-views.md

---

## [2026-07-18] chore/0002-local-dev-launcher — Local dev launcher (Makefile + scripts)

- **Type:** chore
- **Branch:** chore/0002-local-dev-launcher
- **Summary:** No Dockerfiles or docker-compose entries exist for the thirteen application
  services, six connectors, or the UI (only infra is containerized) — every manual smoke test
  this project has run so far required hand-invoking binaries with hand-built env vars. Adds
  `scripts/run-local.sh` (builds the workspace, launches every service as a background process
  with its own `logs/<name>.log`/`run/<name>.pid`, waiting on `/healthz` between dependency
  tiers), `scripts/stop-local.sh`, `scripts/status-local.sh`, `scripts/seed-local-demo.sh`
  (idempotent — seeds a fixed demo tenant/local-user/API-key so the Console UI and
  `POST /v1/ingest` are usable immediately), and a root `Makefile` wrapping all of them
  (`make run`, `make seed`, `make status`, `make stop`, `make logs SERVICE=...`, `make test`,
  `make ci`). Also adds `auth-service --bin hash_password` (offline Argon2id hash generator —
  every real deployment needs some way to seed its first local user before an admin UI exists
  to do it through the API; the seed script uses it rather than duplicating the hashing logic),
  makes docker-compose.yml's infra host ports overridable via `.env`
  (`POSTGRES_PORT`/`RABBITMQ_PORT`/etc., defaulting to the existing values) since a machine
  with other projects already bound to 5432 previously had no way to work around it without
  editing the checked-in file, adds `RABBITMQ_MANAGEMENT_URL` to `.env.example` (missing since
  the observability PR — required, no default, would `.expect()`-panic without it), and adds
  `GET /healthz` to `kizashi-ui` (every other service has one; the UI didn't, which
  `status-local.sh` needs).
- **What running it for the first time actually found**: the launcher surfaced a real ordering
  bug in how the pipeline's RabbitMQ exchanges come up. Every stage's `RabbitMqEventPublisher`
  declares its own exchange on startup; every stage's consumer only `queue_bind`s, which
  requires that exchange to already exist. `analysis-service` (a `record.normalized` consumer)
  starting before `normalization-service` (the `record.normalized` publisher) panicked with
  `NOT_FOUND - no exchange 'record.normalized'`. This ordering constraint — ingestion-service →
  normalization-service → analysis-service → trigger-engine → action-executor, strictly in
  that order — was never documented or encoded anywhere before now; `scripts/run-local.sh`
  encodes it. The seed script also needed two passes to get right: the first demo password
  contained spaces and broke `run/demo-tenant.env` when sourced, and the fixed-id upsert
  originally used `ON CONFLICT (key_hash) DO NOTHING`, which errored on a primary-key collision
  the moment the API key's value changed between runs — fixed to `ON CONFLICT (id) DO UPDATE`
  so re-running always converges to the script's current constants.
- **Tests:** `cargo test --workspace --lib --bins` — all crates green (`kizashi-ui` grew to 36
  tests with the new `/healthz`; `auth-service` gained
  `tests/hash_password_bin_test.rs`, which runs the actual compiled binary via
  `CARGO_BIN_EXE_hash_password` and confirms its output round-trips through `verify_password`,
  not just "the binary exits 0"). Beyond automated tests: ran `make run` against a genuinely
  fresh local Postgres/RabbitMQ/ClickHouse/MinIO stack, confirmed all thirteen services plus
  the UI came up healthy via `make status`, ran `make seed` twice in a row to confirm
  idempotency, logged into the real Console UI as the seeded demo user (real session cookie,
  real redirect), loaded `/events`, `/triggers`, and `/health` (showing all eleven registered
  services `up`), then posted a real record through `POST /v1/ingest` with the seeded API key
  and confirmed via direct Postgres query that it reached `ingestion_service.raw_records` and
  was correctly left un-normalized (no mapping configured for that tenant/source-type — the
  correct no-op, not a bug) — proving the exchange-ordering fix actually holds under a real
  `record.ingested` publish. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` —
  clean, no new advisories (no new dependencies added).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a (operational tooling, not an architectural decision)

## [2026-07-18] feature/0014-docker-images — Containerize all services, connectors, and the UI
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** `scripts/run-local.sh` (prior chore) launched every binary as a plain background
  process on the host — `docker ps` only ever showed the four infra containers
  (Postgres/RabbitMQ/ClickHouse/MinIO), not the actual application. This adds one shared
  multi-stage root `Dockerfile` (builder stage compiles `--bin "${BIN}"`, runtime stage is
  `debian:bookworm-slim` running as non-root `kizashi`), reused across all 20 binaries via
  `--build-arg BIN=<name>` rather than 20 near-identical Dockerfiles, plus `.dockerignore`, and
  extends `docker-compose.yml` with a `build:`+`healthcheck:`+`depends_on:` entry for each of the
  13 services and `kizashi-ui` (internal port always 8080; host ports match the map
  `scripts/run-local.sh` already established), and the 6 connectors under a `connectors` compose
  profile (one-shot CronJob-shaped binaries invoked via `docker compose run --rm`, not
  long-running services, so they don't auto-start with `docker compose up`). `depends_on:
  condition: service_healthy` chains encode both ordinary HTTP-dependency order and the AMQP
  exchange-declaration order discovered in the prior local-launcher PR (ingestion-service →
  normalization-service → analysis-service → trigger-engine → action-executor — each stage's
  publisher declares its own exchange on startup, so a consumer starting first panics with
  `NOT_FOUND - no exchange`).
- **What building/running it for real found**: every migration-running service reads its
  migrations directory via `env!("CARGO_MANIFEST_DIR")` — an absolute build-time source path —
  so a runtime image containing only the compiled binary would panic at startup with a missing
  migrations directory; fixed by also copying the `crates/` source tree into the runtime stage
  (`COPY --from=builder /app/crates /app/crates`), verified by actually running the built
  `ingestion-service` image against real containerized Postgres/RabbitMQ and confirming
  `/healthz` returned 200 (i.e. migrations genuinely ran). Separately, `clickhouse/clickhouse-server`
  came up `unhealthy` under `docker compose up` even though the server itself was fine: its
  `[::]` (IPv6 wildcard) listener fails at startup in this Docker networking environment ("DNS
  error: Address family for hostname not supported"), and the pre-existing healthcheck
  (`wget --spider -q localhost:8123/ping`) resolved `localhost` to `::1` first and got
  connection-refused, even though the server was correctly listening and serving on
  `0.0.0.0:8123` the whole time — fixed by pointing the healthcheck at `127.0.0.1` explicitly
  (confirmed the app-service healthchecks don't share this problem: `curl`, unlike `wget`,
  falls through to the next resolved address on refusal). This ClickHouse healthcheck bug
  predates this branch but was only surfaced by actually bringing the full stack up as
  containers with `depends_on: condition: service_healthy` gating on it.
- **Tests:** `docker compose up -d --build` — all 17 containers (4 infra + 13 services) reached
  `healthy`. Ran a real end-to-end smoke test through the *containerized* stack, not the host
  processes: `scripts/seed-local-demo.sh` against the containerized Postgres (via
  `docker compose exec`), logged into the containerized Console UI's `/login` (200), hit
  `GET /healthz` on both `kizashi-ui` and `ingestion-gateway` through their published host
  ports, then `POST /v1/ingest` through the containerized `ingestion-gateway` with the seeded
  API key and confirmed via direct Postgres query the row reached
  `ingestion_service.raw_records` correctly tenant-scoped and correctly left un-normalized (no
  `NormalizationMapping` configured for that connector/source-type — the correct no-op, not a
  bug). Ran the full local CI gate (`scripts/ci-local.sh`) with `.env` loaded and a throwaway
  local `mssql` container standing in for the CI-only Fabric/TDS integration test dependency
  (mirroring `.github/workflows/`'s own `docker run mcr.microsoft.com/mssql/server` step, not a
  new dependency): `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.73% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a (deployment packaging of already-decided architecture, not a new architectural
  decision — Kubernetes/Helm, the actual "how do we deploy" decision per spec §10, is a
  follow-up item in the approved gap-closing roadmap, not part of this change)

## [2026-07-18] feature/0014-docker-images — Fix `/` 404, tenant-UUID login, and Console UI branding
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Real usage of the just-containerized stack (this branch) surfaced three
  independent UX defects in the Console UI, fixed together since all three sit on the same
  login/landing path: (1) `GET /` was entirely unrouted and 404'd — the exact URL a person
  types first — fixed with a new `root_handler.rs` that redirects `/` to `/events`, which
  itself already bounces an unauthenticated visitor to `/login`; (2) local login required
  typing a raw tenant UUID, which no human can be expected to know, because there was no
  first-class `Tenant` entity anywhere in the system — every service only ever carried a bare
  `tenant_id` foreign key. Added a new `tenants` table + `TenantRepository` to auth-service
  (`crates/auth-service/migrations/0002_create_tenants.sql`), changed
  `POST /v1/auth/local/login` to accept `tenant_name` and resolve it internally (still returns
  a generic 401 for unknown-workspace/unknown-username/wrong-password alike, so none of the
  three is enumerable), and threaded the rename through Console UI's `AuthClient`/login form
  (now labeled "Workspace"); (3) the UI had no visual identity at all — no logo/wordmark, no
  centered login layout, table/nav styling was minimal to the point of looking broken. Gave
  `layout.html` a real theme (CSS custom properties, a "&#9670; Kizashi" wordmark, a centered
  login card with focus states, zebra/hover table rows, a data-URI SVG favicon so the browser
  tab isn't blank) without adding any new dependency (still zero JS, per ADR-0014).
- **Tests:** New: `root_handler_test.rs` (`root_redirects_to_events`),
  `tenant_repository_test.rs` (`finds_a_tenant_id_by_name`,
  `returns_none_for_an_unknown_tenant_name`), plus new/updated cases in
  `local_login_handler_test.rs` (`unknown_tenant_name_is_rejected_with_401_not_404`,
  `tenant_repository_failure_returns_500`), `auth_client_test.rs`
  (`http_client_returns_the_token_and_tenant_id_on_valid_credentials`), and
  `login_handler_test.rs` (`post_login_with_an_unknown_workspace_rerenders_the_form_with_an_error`).
  `cargo test -p auth-service --lib` — 33 passed. `cargo test -p kizashi-ui --lib` — 37 passed.
  Rebuilt and redeployed the `auth-service` and `kizashi-ui` containers, re-ran
  `scripts/seed-local-demo.sh` (now also seeds a `tenants` row, workspace name `acme`), and
  drove a real login through the actual running containers end to end: `GET /` → 303 to
  `/events` (previously 404), `POST /login` with `tenant_name=acme` → 303 to `/events` with a
  valid session cookie, `GET /events` with that cookie → 200. Full local CI gate
  (`scripts/ci-local.sh`, `.env` loaded, throwaway local `mssql` for the Fabric/TDS test) —
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.72%
  line coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (the `tenants` table is additive schema, not a change to the multi-tenancy
  model itself — `tenant_id` remains the system-wide scoping key everywhere except this one
  human-facing login form)

## [2026-07-18] feature/0014-docker-images — Agent registry, live status, drill-down, and reports
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the largest gap this session's live audit surfaced: there was no
  first-class "Agent" concept anywhere in the system — the 6 connector binaries were
  configured only by env vars, with no service that knew of their existence, no way to
  register/list/disable one, and no way to see what a given connector had actually ingested.
  Adds `common::Agent` (id, tenant_id, connector_type, name, config, enabled) and a full
  CRUD registry in Config/Admin Service (`agents` table, `AgentRepository`, audit-logged
  create/update/delete like every other admin entity) at `/v1/agents`. Since connectors don't
  self-report a heartbeat anywhere, "live status" is derived rather than tracked separately:
  Ingestion Service gained `GET /v1/records/stats` (per-`connector_id` record count and
  last-ingested time, aggregated straight off `raw_records`) and
  `GET /v1/records/by-connector` (recent records for one connector, tenant-scoped). The
  matching convention this establishes: an agent's registered `name` is what the deployed
  connector's own `CONNECTOR_ID` env var must be set to, so a registration can be joined
  against real ingestion activity without any new bookkeeping table. Console UI gained three
  pages: `/agents` (register form + live status table, join done in `agents_handler.rs`),
  `/agents/:id` (per-agent drill-down — its own recent records), and `/reports` (ingestion
  volume per connector alongside event counts per type, reusing the existing events feed). Also
  gave the whole UI a second visual pass: form styling (`.panel`, `form.inline`), a `.btn-danger`
  for destructive actions, and nav links for the two new pages.
- **Tests:** `cargo test -p config-admin-service --lib` — 35 passed (12 new: `agent_repository`
  CRUD + tenant scoping + not-found cases, `agent_handlers` tenant-mismatch/404/500 cases).
  `cargo test -p ingestion-service --lib` — 39 passed (10 new: `stats_by_connector` aggregation
  + tenant scoping, `list_by_connector` ordering/limit/tenant scoping, both handlers'
  success/400/500 cases). `cargo test -p kizashi-ui --lib` — 56 passed (19 new across
  `agents_client`, `ingestion_stats_client`, `agents_handler`, `agent_detail_handler`,
  `reports_handler`). Beyond unit tests: rebuilt and redeployed the `config-admin-service`,
  `ingestion-service`, and `kizashi-ui` containers and drove the entire feature through the real
  running stack — logged in, registered a `zendesk`/`support-poller` agent (status correctly
  showed "never run"), posted a real record through `POST /v1/ingest` with `connector_id:
  support-poller`, confirmed the Agents page's status flipped to "active" with a record count
  of 1 (proving the name/connector_id join actually works against live data, not just mocks),
  confirmed the drill-down page showed that record, confirmed the Reports page showed the same
  connector's volume, then deleted the agent and confirmed removal. Full local CI gate
  (`.env` loaded, throwaway local `mssql` for the Fabric/TDS test): `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean,
  `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.11% line coverage
  (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (Agent is additive schema/API following the exact CRUD+audit-log pattern
  TriggerDefinition/NormalizationMapping already established in ADR-0010, not a new
  architectural decision. Deferred/out of scope for this change, tracked separately: a data
  viewer/search page, AI-assisted prompt generation for agent config, and dynamic
  EventTypeDefinition/trigger-condition authoring in the UI.)

## [2026-07-18] feature/0014-docker-images — Data Viewer: search + record detail
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Adds the "data viewer/search" piece of the AIOps-console gap list. Ingestion
  Service gains `RawRecordRepository::search` (every filter optional and AND-ed: connector_id,
  source_type, an ingested-at range, and a substring match against the raw payload's text
  representation via `ILIKE`) exposed as `GET /v1/records/search`, and `get_by_id` exposed as
  `GET /v1/records/:id` for a single-record detail fetch. The free-text match is deliberately a
  plain `ILIKE` scan, not a dedicated search index (Elasticsearch/pg_trgm/tsvector) — v1 scope
  is "find records that mention X," documented in-code as a known limitation to revisit before
  it's exercised at the platform's actual target scale (thousands of inboxes, hundreds of
  connector APIs — flagged directly by the user during this work). Also added
  `idx_raw_records_tenant_connector_ingested_at`, a composite index covering the shape every
  new Agent-related query (`stats_by_connector`, `list_by_connector`, `search`) actually filters
  and sorts by, since the three single-column indexes from the original migration force a
  bitmap-AND plan instead of a single index scan. Console UI gains `/data` (search form +
  results table) and `/data/:id` (pretty-printed raw + normalized payload).
- **Tests:** `cargo test -p ingestion-service --lib` — 50 passed (11 new: `get_by_id`
  tenant-scoping, `search`'s four filter dimensions individually and combined, both new
  handlers' success/400/500/404 cases). `cargo test -p kizashi-ui --lib` — 64 passed (8 new
  across `ingestion_stats_client`, `data_handler`, `data_detail_handler`). Beyond unit tests:
  rebuilt and redeployed `ingestion-service`/`kizashi-ui`, confirmed the new composite index
  exists via `\d ingestion_service.raw_records` against the real container, posted two records
  with different subjects through the real `ingestion-gateway`, searched `/data?q=printer`
  through the real running Console UI and confirmed only the matching record came back (not
  the other one — proving the filter is real, not a no-op), then opened its `/data/:id` detail
  page and confirmed the full raw payload rendered correctly (HTML-escaped by askama). Full
  local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.10% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/index on the existing `RawRecord` schema, not a new
  architectural decision. The scale-driven follow-ups this change explicitly defers — a real
  search index and a dynamic per-agent connector scheduling model to replace one static
  container per connector type — are tracked separately, not silently dropped.)

## [2026-07-18] feature/0014-docker-images — Structured email search + Data Viewer pagination
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Two fixes driven directly by user feedback on the just-shipped Data Viewer.
  First: `raw_payload` was opaque JSON with no defined shape, so there was no way to search
  "subject contains X" or "from Y" or "has attachment Z" — a real gap for the email/message
  connectors this platform targets (Graph Mail, and the planned IMAP connector). Added
  `common::EmailPayload` (subject, from, to/cc/bcc, body, headers, attachments — attachment
  metadata only, never inline bytes; a real attachment's content belongs in the object store
  retention-service already archives into, referenced by `storage_key`) as the documented
  `raw_payload` shape for `SourceType::Message` records from an email connector. Extended
  `RecordSearchFilter`/`GET /v1/records/search` with `subject`/`email_from`/
  `attachment_filename`, each a substring match against the corresponding JSON field (a
  record with no `subject` field simply never matches — not an error), plus a GIN index on
  `raw_payload` so those lookups can use an index scan instead of a full scan at scale. Second:
  every list page (Data Viewer, Agents, Events, Triggers) had a hardcoded `limit` with a silent
  cutoff and no way to see more — flagged directly as not enterprise-grade. Added real
  offset-based pagination to Data Viewer search: the backend fetches one extra row to compute
  `has_more` without a second `COUNT(*)` query (which would scan the same rows twice, at
  exactly the scale pagination exists to handle), and the UI renders Previous/Next as plain
  `<form method="get">` submissions carrying every current filter as hidden fields — no JS,
  consistent with the rest of the app. Agents/Events/Triggers pagination is still open
  (tracked, not silently dropped).
- **Tests:** `cargo test -p common --lib` — 39 passed (2 new: `EmailPayload` round-trip and
  default-field handling). `cargo test -p ingestion-service --lib` — 57 passed (10 new: each
  email filter individually, a no-subject-field non-match case, `has_more` when results exceed
  the page size, offset skipping earlier pages). `cargo test -p kizashi-ui --lib` — 67 passed
  (2 new: pagination controls render correctly on page 0 vs. page 1, with vs. without more
  results). Beyond unit tests: rebuilt and redeployed `ingestion-service`/`kizashi-ui`, posted
  two email-shaped records with different subject/from/attachment through the real
  `ingestion-gateway`, then through the real running Console UI confirmed each of
  subject/email_from/attachment_filename search independently found only its matching record
  (and a deliberately-wrong search term correctly found nothing), then posted 30 more records
  and confirmed real pagination through the live UI: page 1 shows a Next link and no Previous,
  page 2 shows Previous and no Next (30 records, 25-per-page default). Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.29%
  line coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive schema/query/UI work on already-established patterns, not a new
  architectural decision)

## [2026-07-18] feature/0014-docker-images — Agent deploy-script generator
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Reframes what the Agents page is for. The prior "register an agent" form wrote a
  database row that meant nothing on its own — no connector was actually deployed, and
  registering/enabling/disabling it had zero effect on any running process (the row only ever
  correlated with real ingestion if an operator separately, manually configured a connector's
  `CONNECTOR_ID` env var to match by hand). This adds a 3-step deploy-script generator
  (`/agents/generate`) that produces ready-to-run scripts — `docker compose run` (matching the
  `connectors` profile services already in `docker-compose.yml`), bash, and PowerShell (both
  `cargo run -p connector-<type>`) — for each of the 6 connector types, with every required
  env var (pulled directly from each connector's actual `std::env::var(...)` calls in its own
  `main.rs`, not guessed) prefilled with whatever the operator typed into the form. No secret is
  ever fabricated or stored: the API key and every connector credential is exactly what the
  operator entered, substituted directly into the output. The old "register an
  already-deployed agent" form still exists on `/agents` for catalog/status purposes, now
  explicitly labeled as not itself deploying anything.
- **Tests:** `cargo test -p kizashi-ui --lib` — 78 passed (13 new: `connector_field_catalog`'s
  per-type field coverage and secret-marking, the 3-step handler's happy paths, 404-style
  fallback for an unknown connector type, and — critically — a test asserting a submitted
  value the operator typed (an API key) actually appears verbatim in the rendered script, not
  just that the page renders). Beyond unit tests: rebuilt and redeployed `kizashi-ui`, walked
  the real 3-step flow through the live container end to end (select Zendesk → confirm the
  Zendesk-specific fields appear → submit real values → confirmed all three script variants
  render with the submitted API key, subdomain, and token verbatim, and the Docker/bash/
  PowerShell commands reference the correct connector binary/service name). Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.33%
  line coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (UI/workflow addition, not a new architectural decision)

## [2026-07-18] feature/0014-docker-images — Reverse the no-JS constraint, add charts, Overview dashboard
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** ADR-0014 chose a zero-client-JS Console UI specifically because this build
  environment has no browser-automation tooling to test JS the same disciplined way every
  other crate in this repo is tested. The user explicitly overrode that constraint, wanting
  real graphs and interactive components. Writes **ADR-0015**, reversing only the no-JS part
  of ADR-0014 (the server-rendered shell, session handling, and every existing read view stay
  exactly as they are — this is additive, not a rewrite) and explicitly ruling out a full
  React/SPA migration as its own much larger decision, not something to back into as a side
  effect of "add some graphs." Concretely: a small vendored (not CDN-loaded — this is an
  on-prem-capable enterprise product) dependency-free bar-chart renderer
  (`ui/static/charts.js`, baked into the binary via `include_str!`, served at
  `GET /static/charts.js`) reads real server-rendered JSON out of a `<script
  type="application/json">` tag and draws an SVG bar chart — the underlying HTML table is still
  there and still correct if JS fails or is disabled, a deliberate progressive-enhancement
  choice, not an afterthought. Wired onto the Reports page (ingestion volume by connector,
  events by type). Also ships a new `/overview` landing dashboard (KPI cards: agent count/
  active count, total records ingested, event count, platform health with services-up ratio,
  reusing existing backends — no new data path) and makes it the new post-login/root landing
  page (was `/events`). Gave the nav a visual pass alongside this: icon-prefixed links, a
  divider before Log out, `.kpi-card`/`.pill` CSS building blocks for future pages.
- **Security note:** JSON embedded inside a `<script>` tag has every `<` escaped to `<`
  (`chart_json` in `reports_handler.rs`) so an operator-controlled string containing the
  literal text `</script>` can never prematurely close the tag and inject markup — a
  regression test (`chart_data_escapes_a_connector_id_that_could_close_the_script_tag`) pins
  this down explicitly with exactly that payload.
- **Tests:** `cargo test -p kizashi-ui --lib` — 82 passed (7 new: `static_assets` serves the
  right content-type, `overview_handler`'s KPI math against real seeded data across three
  backends, the redirect-target rename from `/events` to `/overview` in both `root_handler`
  and `login_handler`, and the chart-data XSS-escaping regression test). Beyond unit tests:
  `node --check ui/static/charts.js` confirms the vendored JS is syntactically valid (no build
  step exists to catch this otherwise). Rebuilt and redeployed `kizashi-ui`, confirmed through
  the real running container: `/` redirects to `/overview`, the KPI cards render, `GET
  /static/charts.js` serves with `content-type: text/javascript`, and the Reports page's
  `<script type="application/json">` blocks contain real ingestion/event data accumulated
  across this session's earlier smoke tests. **Not verified — flagged explicitly per CLAUDE.md
  §0, not silently claimed**: the SVG bar chart's actual visual rendering in a real browser.
  This environment has no browser-automation tooling (the exact gap ADR-0014 named and
  ADR-0015 accepts as a tradeoff); server-side correctness (data shape, escaping, JS syntax
  validity) is verified, DOM/visual rendering is not. Full local CI gate: `cargo fmt --all
  --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.40% line
  coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** [0015](../docs/adr/0015-console-ui-reverses-adr-0014-no-js-constraint-adds-client-side-js-for-charts-and-components.md)

## [2026-07-18] feature/0014-docker-images — Enforce Agent enabled/disabled status at ingestion
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** `Agent.enabled` was stored since the registry shipped but never checked anywhere
  — disabling an agent in the Console UI had zero effect on whether its data was accepted.
  Closes that gap for real. Config/Admin Service gains `AgentRepository::find_by_name` and
  `GET /v1/agents/by-name/:name`, the lookup Ingestion Gateway needs (agents are keyed by id,
  but ingestion only ever has a `connector_id` string to check against). Ingestion Gateway
  gains an `AgentStatusClient` and checks it on every `POST /v1/ingest`: a `connector_id` with
  no matching registered `Agent` still ingests normally (permissive default — most connectors
  today have no registered row at all, and this must never break them), a matching *enabled*
  agent ingests normally, and a matching *disabled* agent is rejected with 403. A status-lookup
  failure (Config/Admin Service down, network blip) also fails open — availability of the
  ingest path matters more than this soft-enforcement check, so one dependency having a bad
  moment must never take down ingestion for every connector. Console UI's Agents page gains an
  actual Enable/Disable toggle button (previously there was no way to flip the flag at all
  through the UI) and a status pill replacing the old plain yes/no text.
- **Tests:** `cargo test -p config-admin-service --lib` — 40 passed (5 new:
  `find_by_name`'s tenant-scoping and not-found cases, the `by-name` handler's 200/404). `cargo
  test -p ingestion-gateway --lib` — 21 passed (7 new: `AgentStatusClient` against a real stub
  server for enabled/disabled/404/unreachable, and the proxy handler's three enforcement
  cases — disabled rejects, unregistered passes, lookup-failure fails open). `cargo test -p
  kizashi-ui --lib` — 85 passed (3 new: `update_agent` against a real stub server, the toggle
  handler flipping state and redirecting, and its login-required case). Beyond unit tests:
  rebuilt and redeployed `ingestion-gateway`/`config-admin-service`/`kizashi-ui`, registered a
  real agent through the live UI, ingested through the live `ingestion-gateway` while enabled
  (201), disabled it through the UI's new toggle button, ingested again with the same
  `connector_id` and got the real 403 with the exact expected error message, confirmed an
  unrelated unregistered `connector_id` still ingests fine (permissive default holds), then
  re-enabled and confirmed ingestion resumes (201) before cleaning up the test agent. Full
  local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.38% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (closes a gap in the already-established Agent registry, not a new
  architectural decision)

## [2026-07-18] feature/0014-docker-images — Events pagination
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Events was one of three list pages flagged as having a hardcoded limit with a
  silent cutoff and no way to see more (Data Viewer got real pagination earlier; this closes
  the same gap for Events). Dashboard API's `EventFilter` gains `offset`, the ClickHouse query
  gains a matching `OFFSET`, and `GET /v1/events` now returns `{events, has_more}` instead of a
  bare array — `has_more` computed the same way as the Data Viewer's search (fetch one extra
  row, no second `COUNT(*)` query against ClickHouse). Query Gateway needed no changes — it
  already passes the full query string through via `OriginalUri` untouched. Console UI's
  `/events` gains the same Previous/Next `<form method="get">` pagination controls as the Data
  Viewer. Agents and Triggers pagination remain open — flagged, not silently dropped; Triggers
  in particular is low-volume (operator-configured, not per-record data) so it's lower priority
  than Events/Data Viewer, which both read from tables that grow with real traffic.
- **Tests:** `cargo test -p dashboard-api --lib` — 18 passed (3 new: offset skips earlier
  pages at the repository level, the handler's `has_more` computation, and the response-shape
  change reflected in the existing scoped-events test). `cargo test -p kizashi-ui --lib` — 85
  passed (`EventsPage`/`EventsClient` trait signature change threaded through
  `events_handler`, `overview_handler`, and `reports_handler`'s call sites, plus 2 new
  pagination-control-rendering tests mirroring the Data Viewer's). Beyond unit tests: rebuilt
  and redeployed `dashboard-api`/`kizashi-ui`, confirmed `/events` and `/events?page=1` both
  return 200 through the real running stack (query-gateway → dashboard-api → ClickHouse) with
  the new response shape, proving the plumbing holds end-to-end. Full live-data pagination
  boundary behavior (Next/Previous appearing at exactly the right count) is unit-tested with
  controlled data, not independently re-verified against real ClickHouse volume in this pass —
  the demo tenant has no real event traffic to page through without standing up the full
  ingest→normalize→analyze→trigger pipeline, called out explicitly rather than implied. Full
  local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.45% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/response-shape change, not a new architectural decision)

## [2026-07-18] feature/0014-docker-images — Agents pagination, and a real correctness fix it forced
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the last of the three flagged list pages (Data Viewer and Events already
  had real pagination). `AgentRepository::list` gains `limit`/`offset`, `GET /v1/agents` now
  returns `{agents, has_more}` (fetch-one-extra pattern, same as Events/Data Viewer), and
  `/agents` gets the same Previous/Next controls. Doing this properly surfaced a real
  correctness bug in the process: `agent_detail_handler` and the enable/disable toggle both
  found "the agent" by calling `list_agents` and searching the result for a matching id — which
  only worked because `list_agents` used to return every agent unpaginated. Once it's
  paginated, that lookup silently breaks the moment an agent isn't on the first page (toggling
  an agent on page 2 would appear to succeed — 303 redirect, no error — while doing nothing).
  Fixed by adding `AgentsClient::get_agent`/`GET /v1/agents/:id` (config-admin-service already
  had this route; the UI just wasn't using it) and switching both call sites to fetch by id
  directly instead of paging through a list. Triggers pagination remains open — still lower
  priority, operator-configured data that doesn't grow with traffic the way agents/events/raw
  records do.
- **Tests:** `cargo test -p config-admin-service --lib` — 42 passed (3 new: `list` respects
  limit/offset at the repository level, the handler's `has_more` computation, the existing
  scoped-list test updated for the new response shape). `cargo test -p kizashi-ui --lib` — 88
  passed (`AgentsClient` trait signature change threaded through every call site, 2 new
  pagination-control-rendering tests, `get_agent` tested against a real stub server). Beyond
  unit tests: rebuilt and redeployed `config-admin-service`/`kizashi-ui`, registered 30 real
  agents through the live UI, confirmed page 1 shows Next-only and page 2 shows Previous-only,
  then — the test that actually matters — found an agent that only exists on page 2, toggled
  it, and confirmed on a fresh page load that it actually flipped from enabled to disabled
  (proving the `get_agent` fix holds against live data, not just the bug's absence in a unit
  test), then cleaned up all 30 test agents. Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo
  test --workspace --all-features` all green, `cargo llvm-cov` 94.44% line coverage (85%
  floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/response-shape change plus a bugfix, not a new architectural
  decision)

## [2026-07-18] feature/0014-docker-images — Triggers pagination (last of the four flagged list pages)
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the last remaining item from the pagination backlog (Data Viewer, Events,
  and Agents were already done). `TriggerDefinitionRepository::list` gains `limit`/`offset`
  (Postgres impl adds `LIMIT $2 OFFSET $3`, ordered by `name`), `GET /v1/trigger-definitions`
  now returns `{triggers, has_more}` using the same fetch-one-extra pattern as every other
  paginated list endpoint in this codebase, and `/triggers` gets the same Previous/Next
  `<form method="get">` controls as Agents/Events/Data Viewer. `TriggersClient::list_triggers`
  and the Console UI handler/template were updated to match, mirroring `AgentsClient`/
  `agents_handler.rs` exactly. Triggers had no existing "get one by id" call site (no detail
  page), so this pass did not surface the same list-vs-lookup bug the Agents pagination work
  found — there was nothing to fix beyond the list endpoint itself.
- **Tests:** `cargo test -p config-admin-service --lib` — 43 passed (1 new: `list` respects
  limit/offset at the repository level; the existing scoped-list test and the full CRUD
  round-trip test were both updated for the new response shape). `cargo test -p kizashi-ui
  --lib` — 92 passed (`TriggersClient` trait signature change threaded through
  `triggers_handler`, 2 new pagination-control-rendering tests mirroring Agents/Events).
  Beyond unit tests: rebuilt and redeployed `config-admin-service`/`kizashi-ui`, seeded 30
  real trigger definitions directly against the live `config-admin-service` API, confirmed
  `/triggers` shows Next-only on page 0 and Previous-only on page 1 with the 30th trigger
  landing on page 1 as expected, then deleted all 30 test triggers (and their audit-log rows)
  to leave the demo tenant clean. Full local CI gate: `cargo fmt --all --check` clean, `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo test
  --workspace --all-features` all green (0 failures across every crate, verified against a
  throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 93.90% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing `unmaintained` advisories already allow-listed — `instant`,
  `rustls-pemfile` — no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/response-shape change, not a new architectural decision)

## [2026-07-18] feature/0014-docker-images — Audit log immutability enforced at the database level
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** `config_admin_service.config_audit_log` and `retention_service.retention_audit_log`
  were append-only by application convention only (no Rust code path ever issues UPDATE/DELETE
  against them) — nothing at the database level stopped a bug or a manual `psql` session from
  mutating or deleting an audit row, a real gap against CLAUDE.md §5's "every admin/config
  change is logged immutably" bar for a product that expects compliance audits. Since
  `common::connect_with_schema` and every service's `main.rs` run migrations and runtime
  queries through the same connection pool and the same shared `kizashi` Postgres role (no
  role separation exists anywhere in this codebase), a `REVOKE UPDATE, DELETE` approach would
  have required introducing a second privileged migration-only role — a much larger,
  unprecedented change. Went with a `BEFORE UPDATE OR DELETE` trigger on each table that
  `RAISE EXCEPTION`s instead — a single plain `.sql` migration per service, no new role, no
  `docker-compose.yml`/`.env.example`/`common` changes, works regardless of which role issues
  the mutation.
- **Tests:** TDD'd against real Postgres: wrote the regression tests first, ran them without
  the migration present to confirm they fail for the expected reason (`rows_affected: 1`, i.e.
  the row-level trigger genuinely wasn't there yet), then added the migration and reran.
  `cargo test -p config-admin-service --test repository_integration_test` — 6 passed (2 new:
  `config_audit_log_rejects_update_at_the_database_level`,
  `config_audit_log_rejects_delete_at_the_database_level`, both asserting the real Postgres
  error text). `cargo test -p retention-service --test retention_policy_integration_test` — 6
  passed (2 new, same pattern for `retention_audit_log`). Beyond integration tests: rebuilt and
  redeployed `config-admin-service`/`retention-service`, created a real trigger definition and
  a real retention policy through their live HTTP APIs (so each had a genuine audit row), then
  ran a raw `UPDATE`/`DELETE` against each audit table directly via `psql` against the live
  running Postgres container and confirmed Postgres itself rejected all four attempts with
  `... is append-only: UPDATE/DELETE is not permitted` — proving the trigger is live against
  the real running stack, not just the test database. Full local CI gate: `cargo fmt --all
  --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean, `cargo test --workspace --all-features` all green (0 failures across every crate,
  verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 93.90% line coverage (85% floor — unchanged, since the new
  code is pure SQL plus integration tests, neither counted in the Rust line-coverage ratchet),
  `cargo audit` / `cargo deny check` clean (same two pre-existing allow-listed `unmaintained`
  advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — closes a gap flagged in the standing gap-closing roadmap
  (Phase 1b, security/compliance), not a spec §11 open item.

## [2026-07-18] feature/0014-docker-images — API key lifecycle management (create/list/revoke)
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Closes gap-closing-roadmap Phase 1c: until now `ApiKeyStore` only had
  `tenant_for_key` (lookup) — there was no way to actually create or revoke a connector API
  key except a manual `INSERT`/`UPDATE` against Postgres, a real problem for a resold
  enterprise product whose customers need to self-serve issue and rotate credentials.
  `ApiKeyStore` gains `create`/`list`/`revoke`, all backed by Postgres, with `create`/`revoke`
  each writing an audit row in the same transaction as the key mutation (CLAUDE.md §5's
  "new mutable config entity ships with an audit-log write in the same PR" rule) — this
  required standing up ingestion-gateway's *first* audit log (`ingestion_gateway_audit_log`,
  ported from config-admin-service's `audit_log.rs`), which ships with the same
  `BEFORE UPDATE OR DELETE` immutability trigger just added to the other two audit tables, from
  day one rather than as a follow-up gap. New endpoints: `POST /v1/api-keys` (returns the
  plaintext key once — only its SHA-256 hash is ever persisted, matching the existing
  `tenant_for_key` convention), `GET /v1/api-keys` (tenant-scoped summaries, no key material),
  `DELETE /v1/api-keys/:id` (idempotent revoke), `GET /v1/api-keys/:id/audit-log`. Console UI
  gets a new `/api-keys` page (nav: "API Keys") — create form, table with Revoke buttons, and
  a one-time plaintext-key reveal panel shown only on the response immediately after creation,
  never persisted or retrievable again. Required adding `INGESTION_GATEWAY_URL` (the internal
  address) alongside the existing `INGESTION_GATEWAY_PUBLIC_URL` (the address a *deployed
  connector* should point at, not necessarily reachable from inside the UI container) — Console
  UI needed a way to reach ingestion-gateway's admin API that's distinct from the
  connector-facing address.
- **Tests:** `cargo test -p ingestion-gateway --lib` — 32 passed (in-memory `ApiKeyStore`/
  `AuditLogReader` test doubles, HTTP handler tests for create/list/revoke/audit-log, a
  never-exposes-key-material assertion on the list response, a missing-tenant-header 401
  case). `cargo test -p ingestion-gateway --test api_key_store_integration_test` — 6 passed
  against real Postgres (create writes a Created audit row and the key resolves; revoke writes
  a Deleted audit row and the key stops resolving; revoking an already-revoked key writes no
  duplicate audit row; list is tenant-scoped; the new `ingestion_gateway_audit_log` rejects
  UPDATE/DELETE at the database level, same pattern as the previous PR's immutability tests).
  `cargo test -p kizashi-ui --lib` — 106 passed (`ApiKeysClient` HTTP-client tests against a
  real stub server, 5 new handler tests including the one-time-reveal assertion). Beyond unit
  tests: rebuilt and redeployed `ingestion-gateway`/`kizashi-ui`, logged into the live UI,
  created a real key through `/api-keys`, confirmed the plaintext was shown, used it to
  authenticate a real `POST /v1/ingest` call (got 422 from the payload-shape check, not 401 —
  proving auth passed), revoked it through the UI, and confirmed the exact same key now gets
  401 "invalid API key" on the same ingest call — the complete lifecycle proven against the
  real running stack, not just test doubles. Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo
  test --workspace --all-features` all green (0 failures across every crate, verified against
  a throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 93.76% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — closes a gap flagged in the standing gap-closing roadmap (Phase 1c,
  security/compliance), not a spec §11 open item.

## [2026-07-18] feature/0014-docker-images — RBAC v1: role on local users, write-path enforcement on config-admin-service
- **Type:** feature
- **Summary:** Closes gap-closing-roadmap Phase 1a's highest-priority item: until now every
  service trusted `X-Tenant-Id` with zero role/permission check — any authenticated session
  could create/update/delete triggers and mappings regardless of who it belonged to. Adds
  `common::Role` (`Viewer < Operator < Admin`, ordered) and threads it end-to-end through the
  identity chain that already exists: `auth_service.local_users` gains a `role` column (new
  migration, existing rows default to `admin` so the demo login isn't locked out) →
  `SessionClient::mint_session` gains a `role` param → `query-gateway`'s `/internal/tokens` +
  `TokenStore` + `query_api_tokens` table carry it (`tenant_for_token` renamed
  `session_for_token`, now returns `(tenant_id, role)`) → `LoginResponse` returns it → Console
  UI's `Session` struct carries it. `config-admin-service`'s `create_trigger`/`update_trigger`/
  `create_mapping`/`update_mapping` now require an `X-Role` header at least `Operator`, 403
  otherwise, 401 if the header is missing entirely — the same trust-boundary pattern
  `X-Tenant-Id` already uses, since no gateway sits in front of this service (ADR-0010) to
  enforce roles at a proxy layer. OIDC logins (which have no local role source) default to the
  least-privileged `Viewer` rather than being left unroled or guessing something permissive.
  See ADR-0016 for the full v1-scope decision, including what's explicitly deferred:
  `retention-service`, `action-executor`, and `ingestion-gateway`'s API-key endpoints remain
  unenforced (tracked, not silently dropped), and there's no "assign another user's role" UI
  yet — that's a direct SQL update for now, same interim state API keys were in before Phase
  1c's UI shipped.
- **Tests:** `cargo test -p common role` — 5 passed (ordering, `at_least`, `Display`/`FromStr`
  round-trip, snake_case serialization). `cargo test -p auth-service --lib` — 33 passed
  (`LocalUser`/`SessionClient` role threading, a new assertion that a successful login mints
  with the user's actual role and returns it in the response body). `cargo test -p auth-service
  --test local_user_repository_integration_test` — 1 passed against real Postgres, now
  asserting the stored role round-trips. `cargo test -p query-gateway --lib` — 14 passed
  (`TokenStore`/`session_for_token` role threading). `cargo test -p query-gateway --test
  token_store_integration_test` — 2 passed against real Postgres (stored role round-trips;
  minted tokens carry the role they were minted with). `cargo test -p config-admin-service
  --lib` — 47 passed (4 new: missing-role-header 401, viewer-rejected 403 on both
  trigger-create and mapping-create, operator-allowed 201 — the actual enforcement contract).
  `cargo test -p kizashi-ui --lib` — 101 passed (every `Session`/`AppState` construction site
  across the test suite updated for the new field; no behavioral change to any existing UI test
  since role isn't yet consumed by nav or any write-path client). Beyond unit/integration
  tests: rebuilt and redeployed `auth-service`/`query-gateway`/`config-admin-service`/
  `kizashi-ui`, confirmed the demo login still returns `"role":"admin"` and Console UI login
  still works end-to-end, then — the test that actually proves the enforcement — sent a real
  trigger-create request directly at the live `config-admin-service` three ways: no `X-Role`
  header (401), `X-Role: viewer` (403), `X-Role: operator` (201), all against real running
  Postgres with the real migration applied. Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (one
  `await_holding_lock` finding caught and fixed — a `MutexGuard` held across an `.await` in a
  new test), `cargo test --workspace --all-features` all green (0 failures across every crate,
  verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 93.81% line coverage (85% floor), `cargo audit` / `cargo deny
  check` clean (same two pre-existing allow-listed `unmaintained` advisories, no new
  advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** [0016-rbac-v1-scope-role-on-local-user-x-role-header-trust-config-admin-write-path-enforcement.md](../adr/0016-rbac-v1-scope-role-on-local-user-x-role-header-trust-config-admin-write-path-enforcement.md)

## [2026-07-19] feature/0014-docker-images — RBAC enforcement extended to retention-service
- **Type:** feature
- **Summary:** First of ADR-0016's explicitly-deferred follow-ups: `retention-service`'s
  `create_policy`/`update_policy` now require `X-Role` at least `Operator`, mirroring
  `config-admin-service`'s enforcement exactly (`role_from_headers`/`require_operator` helpers,
  same 401-missing/403-insufficient/pass-through-Operator-or-above contract). No new migration
  needed — `retention-service` doesn't mint its own sessions; it trusts the same `X-Role` header
  Console UI/callers already forward. `action-executor`'s trigger CRUD and
  `ingestion-gateway`'s API-key create/revoke remain unenforced, still tracked in ADR-0016 as
  the next follow-ups.
- **Tests:** `cargo test -p retention-service --lib` — 43 passed (3 new: missing-role 401,
  viewer-rejected 403, operator-allowed 201 on `create_policy`, mirroring
  config-admin-service's role tests exactly). Beyond unit tests: rebuilt and redeployed
  `retention-service`, sent a real policy-create request three ways against the live service —
  no `X-Role` (401), `X-Role: viewer` (403), `X-Role: operator` (201) — against real running
  Postgres. Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` clean, `cargo test --workspace --all-features`
  all green (0 failures across every crate, verified against a throwaway local `mssql`
  container standing in for CI's Fabric TDS dependency), `cargo llvm-cov` 93.84% line coverage
  (85% floor), `cargo audit` / `cargo deny check` clean (same two pre-existing allow-listed
  `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — implements a follow-up explicitly scoped out of ADR-0016's v1, not a new
  architectural decision.

## [2026-07-19] feature/0014-docker-images — Instana-style Pipeline Map view
- **Type:** feature
- **Summary:** Continues ADR-0015's Instana-style APM direction (#30) with the feature that
  actually reads as "Instana" — a live topology map, not another table. New `/pipeline` page
  renders the ingest → normalize → analyze → trigger → act chain as connected boxes: each stage
  node colored by its real `/v1/health` status (green dot = up, red = down), each connecting
  edge labeled with the message type it carries and colored by real `/v1/backlog` queue depth
  (grey = empty, amber = building up, red = past the critical threshold). Both data sources
  already existed in Observability (ADR-0012) — this wires Console UI to consume the backlog
  endpoint for the first time via a new `BacklogClient`, alongside the existing `HealthClient`.
  A backlog-lookup failure degrades the page to "topology with no backlog numbers" rather than
  an error page (health is the load-bearing signal; backlog is enrichment), while a health
  failure does show the error page since the topology has nothing meaningful to render without
  it. Template built as a flat, pre-interleaved `Vec<TopologyItem>` (stage/edge alternating)
  rather than having the template zip two arrays — Askama's expression grammar makes index
  arithmetic (`edges[loop.index0 - 1]`) fragile, so the ordering is resolved in Rust and the
  template just iterates and matches.
- **Tests:** `cargo test -p kizashi-ui --lib` — 108 passed (2 new for `BacklogClient` against a
  real stub server; 5 new for the pipeline handler: all five stages render with correct
  up/down status, redirects to login when signed out, shows an error when health fails,
  degrades gracefully with "n/a" backlog numbers when backlog fails, and a 500-message queue
  renders as `edge-critical`). Beyond unit tests: rebuilt and redeployed `kizashi-ui`, logged
  into the live stack, loaded `/pipeline` for real, and confirmed all five stages rendered
  "up" with 0-message queues on every edge — matching the actual idle state of the real running
  pipeline (no synthetic data, genuine live health/backlog reads through the full
  Console-UI-to-Observability-to-RabbitMQ path). Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo
  test --workspace --all-features` all green (0 failures across every crate, verified against
  a throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 93.91% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — additive UI feature consuming already-decided ADR-0012/ADR-0015 capabilities,
  not a new architectural decision.

## [2026-07-19] feature/0014-docker-images — Role-aware nav: hide write actions from Viewers
- **Type:** feature
- **Summary:** Closes ADR-0016's last still-open Console UI v1 item: "role-aware nav (hide
  admin actions from viewer)." `/agents` and `/api-keys` now compute
  `can_write = session.role.at_least(Role::Operator)` and gate the register/create forms and
  every per-row Enable/Disable/Remove/Revoke button behind it — a `Viewer` sees the same data
  (agent list, key list) with none of the mutation controls. This is presentation-layer only:
  `agents`-write and `ingestion-gateway`'s API-key endpoints don't enforce role server-side yet
  (only config-admin-service's trigger/mapping writes and retention-service's policy writes
  do, per ADR-0016 and its retention-service follow-up) — noted explicitly in code comments so
  it isn't mistaken for a security boundary.
- **Tests:** `cargo test -p kizashi-ui --lib` — 112 passed (4 new: a `Viewer` session sees the
  agent/key data but none of the write UI; an `Operator` session sees both). Beyond unit
  tests: rebuilt and redeployed `kizashi-ui`, inserted a real `viewer`-role user directly into
  the running `auth_service.local_users` table (via the existing `hash_password` bin for a
  real Argon2 hash), logged in as that user through the live UI, and confirmed zero matches for
  every write control on both `/agents` and `/api-keys` — then logged back in as the existing
  `admin`-role demo user and confirmed all controls are present, proving the gate works both
  directions against the real running stack, not just template unit tests. Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green (0 failures across every
  crate, verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 93.96% line coverage (85% floor), `cargo audit` / `cargo deny
  check` clean (same two pre-existing allow-listed `unmaintained` advisories, no new
  advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — implements a follow-up explicitly scoped into ADR-0016's v1 Console UI item,
  not a new architectural decision.

## [2026-07-19] feature/0014-docker-images — RBAC enforcement extended to ingestion-gateway API keys
- **Type:** feature
- **Summary:** Closes ADR-0016's last remaining deferred write path.
  `action-executor` turned out to have no HTTP write surface at all (it's a pure RabbitMQ
  consumer with only `/healthz`), so there was nothing to gate there — that leaves
  `ingestion-gateway`'s `create_api_key`/`revoke_api_key` as the real remaining item, now
  requiring `X-Role` at least `Operator` via the same `role_from_headers`/`require_operator`
  pattern as every other write path. Because Console UI's Agents/API-Keys pages actively call
  these endpoints (unlike config-admin-service's trigger/mapping writes, which have no UI form
  yet), enabling enforcement without also updating the caller would have broken the live
  create/revoke flow verified working in the previous PR — so `ApiKeysClient::create_api_key`/
  `revoke_api_key` gained a `role: Role` parameter, forwarded as `X-Role`, with
  `api_keys_handler.rs` passing `session.role` through. Every write-path service in the
  platform's admin surface (config-admin-service, retention-service, ingestion-gateway) is now
  role-gated; the only remaining gap from ADR-0016 is the "assign another user's role" admin UI,
  still explicitly out of scope for v1.
- **Tests:** `cargo test -p ingestion-gateway --lib` — 34 passed (2 new: missing-role 401,
  viewer-rejected 403 on `create_api_key`; existing create/revoke tests updated to send
  `X-Role`). `cargo test -p kizashi-ui --lib` — 112 passed (`ApiKeysClient` trait signature
  change threaded through every call site; the HTTP-client stub server now rejects a missing
  `X-Role` on create, proving the client actually sends it). Beyond unit tests: rebuilt and
  redeployed `ingestion-gateway`/`kizashi-ui`, created a real key through the live UI as the
  `admin`-role demo user (confirming the enforcement-plus-forwarding change didn't break the
  working flow), then sent the same create request directly at `ingestion-gateway` three ways —
  no `X-Role` (401), `X-Role: viewer` (403), `X-Role: operator` (201) — against the real
  running service. Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace
  --all-features` all green (0 failures across every crate, verified against a throwaway local
  `mssql` container standing in for CI's Fabric TDS dependency), `cargo llvm-cov` 93.98% line
  coverage (85% floor), `cargo audit` / `cargo deny check` clean (same two pre-existing
  allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — implements the last follow-up explicitly scoped out of ADR-0016's v1, not a
  new architectural decision.
